//! Monomorphization: duplicate generic functions for explicit instantiation sites.

use std::collections::HashMap;

use phx_syntax::ast::decl::{Function, TopLevelDecl, TopLevelItem};

use super::check::TypeChecker;
use super::subst::Substitution;
use super::types::TypeId;
use crate::resolver::{Def, DefId, DefKind, ResolutionKey, ResolvedProgram};
use crate::typeck::TypedProgram;

/// One explicit generic function instantiation from a call site.
#[derive(Debug, Clone)]
pub struct MonoInst {
    /// Generic function definition.
    pub base_fn: DefId,
    /// Concrete type arguments in generic-parameter order.
    pub args: Vec<TypeId>,
    /// Name-use ids at call sites to retarget to the specialized function.
    pub call_sites: Vec<phx_syntax::AstNodeId>,
}

/// Lowers collected instantiations into specialized defs, layouts, and resolution patches.
pub fn monomorphize(typed: &mut TypedProgram, insts: &[MonoInst]) {
    if insts.is_empty() {
        return;
    }
    let mut resolution_patches: HashMap<ResolutionKey, DefId> = HashMap::new();
    let expr_base = typed
        .expr_types
        .keys()
        .map(|id| id.index())
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    for inst in insts {
        let Some(param_defs) = generic_param_defs_for_base(&typed.resolved, inst.base_fn) else {
            continue;
        };
        if param_defs.len() != inst.args.len() {
            continue;
        }
        let mut subst = Substitution::new();
        for (param, arg) in param_defs.iter().zip(&inst.args) {
            subst.insert(*param, *arg);
        }
        let spec_def =
            alloc_specialized_def(&mut typed.resolved, inst.base_fn, &inst.args, &typed.types);
        typed.specialized_from.insert(spec_def, inst.base_fn);

        let Some(f) = find_function(&typed.resolved, inst.base_fn) else {
            continue;
        };
        let mut checker = TypeChecker::new_with_substitution(&typed.resolved, subst);
        checker.set_expr_id_base(expr_base);
        checker.check_function_specialized(f, spec_def);
        let (checker_types, expr_types, bag, layouts, program_layout, value_types) =
            checker.finish_all();
        if bag.has_errors() {
            continue;
        }
        typed.types = checker_types;
        typed.expr_types.extend(expr_types);
        typed.functions.extend(layouts);
        typed.layout = program_layout;
        if let Some(&fn_ty) = value_types.get(&spec_def) {
            let _ = fn_ty;
        }
        for node_id in &inst.call_sites {
            for module in &typed.resolved.modules {
                resolution_patches.insert(
                    ResolutionKey {
                        module: module.id,
                        node_id: *node_id,
                    },
                    spec_def,
                );
            }
        }
    }
    for (key, spec) in resolution_patches {
        typed.resolved.resolutions.insert(key, spec);
    }
}

fn find_function(resolved: &ResolvedProgram, def: DefId) -> Option<&Function> {
    for module in &resolved.modules {
        for item in &module.program.items {
            let TopLevelItem {
                decl: TopLevelDecl::Function(f),
                ..
            } = &item.inner
            else {
                continue;
            };
            if fn_def_id(resolved, module.id, f.name.symbol) == Some(def) {
                return Some(f);
            }
        }
    }
    None
}

fn fn_def_id(resolved: &ResolvedProgram, module: u32, name: phx_syntax::Symbol) -> Option<DefId> {
    resolved
        .defs
        .iter()
        .enumerate()
        .find(|(_, d)| d.kind == DefKind::Fn && d.name == name && d.module == module)
        .map(|(i, _)| DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
}

fn generic_param_defs_for_base(resolved: &ResolvedProgram, base: DefId) -> Option<Vec<DefId>> {
    let f = find_function(resolved, base)?;
    generic_param_defs(resolved, f)
}

fn generic_param_defs(resolved: &ResolvedProgram, f: &Function) -> Option<Vec<DefId>> {
    let params = f.generics.as_ref()?;
    let module = resolved.root;
    let mut defs = Vec::new();
    for param in params {
        let id = resolved.defs.iter().enumerate().find_map(|(i, d)| {
            if d.kind == DefKind::GenericParam && d.name == param.name.symbol && d.module == module
            {
                Some(DefId::from_raw(u32::try_from(i).ok()?))
            } else {
                None
            }
        })?;
        defs.push(id);
    }
    Some(defs)
}

fn alloc_specialized_def(
    resolved: &mut ResolvedProgram,
    base: DefId,
    args: &[TypeId],
    types: &super::types::TypeInterner,
) -> DefId {
    let base_def = &resolved.defs[base.index() as usize];
    let suffix: String = args
        .iter()
        .map(|a| super::display::format_type(types, &resolved.interner, &resolved.defs, *a))
        .map(|s| {
            s.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>()
        })
        .collect();
    let mangled = format!("{}${}", resolved.interner.resolve(base_def.name), suffix);
    let sym = resolved
        .interner
        .intern(&mangled)
        .unwrap_or_else(|_| phx_syntax::Symbol::from_raw(0));
    let def = Def::new(
        DefKind::Fn,
        sym,
        base_def.span,
        base_def.module,
        base_def.exported,
        base_def.scope_depth,
    );
    let id = DefId::from_raw(u32::try_from(resolved.defs.len()).unwrap_or(u32::MAX));
    resolved.defs.push(def);
    id
}
