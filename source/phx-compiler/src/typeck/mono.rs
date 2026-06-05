//! Monomorphization: duplicate generic functions and type layouts for explicit instantiation sites.

use std::collections::HashMap;

use phx_diagnostics::{TypeCheckBag, TypeCheckError};
use phx_syntax::ast::decl::{Function, TopLevelDecl, TopLevelItem};
use phx_syntax::ast::types::GenericParam;

use super::check::TypeChecker;
use super::layout::{EnumLayout, StructLayout, TypeMonoKey, VariantKind, VariantLayout};
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

/// Kind of generic user type being monomorphized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeMonoKind {
    /// `Name :: <…> struct { … }`
    Struct,
    /// `Name :: <…> enum { … }`
    Enum,
    /// `type Name<…> = …`
    Alias,
}

/// One explicit generic type instantiation from a use site.
#[derive(Debug, Clone)]
pub struct TypeMonoInst {
    /// Generic struct / enum / alias template definition.
    pub base_def: DefId,
    /// What kind of template is being specialized.
    pub kind: TypeMonoKind,
    /// Concrete type arguments in generic-parameter order.
    pub args: Vec<TypeId>,
}

/// Lowers collected function and type instantiations into specialized defs and layouts.
///
/// Returns a diagnostic bag when any specialization fails (arity mismatch or re-check errors).
#[must_use]
pub fn monomorphize(
    typed: &mut TypedProgram,
    fn_insts: &[MonoInst],
    type_insts: &[TypeMonoInst],
) -> TypeCheckBag {
    let mut bag = TypeCheckBag::new();
    monomorphize_functions(typed, fn_insts, &mut bag);
    monomorphize_types(typed, type_insts, &mut bag);
    bag
}

fn monomorphize_functions(typed: &mut TypedProgram, insts: &[MonoInst], bag: &mut TypeCheckBag) {
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
        let Some(param_defs) = generic_param_defs_for_fn_base(&typed.resolved, inst.base_fn) else {
            continue;
        };
        let base_def = &typed.resolved.defs[inst.base_fn.index() as usize];
        if param_defs.len() != inst.args.len() {
            bag.push(
                base_def.module,
                TypeCheckError::ArityMismatch {
                    expected: param_defs.len(),
                    found: inst.args.len(),
                    span: base_def.span,
                },
            );
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
        let mut checker =
            TypeChecker::new_with_substitution(&typed.resolved, subst, typed.types.clone());
        checker.set_expr_id_base(expr_base);
        checker.check_function_specialized(f, spec_def);
        let (
            checker_types,
            expr_types,
            checker_bag,
            layouts,
            _program_layout,
            value_types,
            spec_aliases,
        ) = checker.finish_all();
        if checker_bag.has_errors() {
            for located in checker_bag.into_errors() {
                bag.push_located(located);
            }
            continue;
        }
        typed.types = checker_types;
        typed.expr_types.extend(expr_types);
        typed.functions.extend(layouts);
        typed.specialized_aliases.extend(spec_aliases);
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

fn monomorphize_types(typed: &mut TypedProgram, insts: &[TypeMonoInst], bag: &mut TypeCheckBag) {
    if insts.is_empty() {
        return;
    }
    let mut next_type_id = typed
        .layout
        .type_ids
        .values()
        .chain(typed.layout.specialized_type_ids.values())
        .copied()
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    for inst in insts {
        if inst.kind == TypeMonoKind::Alias {
            continue;
        }
        let Some(param_defs) = generic_param_defs_for_type(&typed.resolved, inst.base_def) else {
            continue;
        };
        let base_def = &typed.resolved.defs[inst.base_def.index() as usize];
        if param_defs.len() != inst.args.len() {
            bag.push(
                base_def.module,
                TypeCheckError::ArityMismatch {
                    expected: param_defs.len(),
                    found: inst.args.len(),
                    span: base_def.span,
                },
            );
            continue;
        }

        let key = TypeMonoKey::new(inst.base_def, inst.args.clone());
        if typed.layout.specialized_type_ids.contains_key(&key) {
            continue;
        }

        let mut subst = Substitution::new();
        for (param, arg) in param_defs.iter().zip(&inst.args) {
            subst.insert(*param, *arg);
        }

        match inst.kind {
            TypeMonoKind::Struct => {
                specialize_struct(typed, inst.base_def, &key, &subst, next_type_id);
                next_type_id = next_type_id.saturating_add(1);
            }
            TypeMonoKind::Enum => {
                specialize_enum(typed, inst.base_def, &key, &subst, next_type_id);
                next_type_id = next_type_id.saturating_add(1);
            }
            TypeMonoKind::Alias => {}
        }
    }
}

fn specialize_struct(
    typed: &mut TypedProgram,
    base: DefId,
    key: &TypeMonoKey,
    subst: &Substitution,
    type_id: u32,
) {
    let Some(template) = typed.layout.structs.get(&base).cloned() else {
        return;
    };
    let fields: Vec<_> = template
        .fields
        .iter()
        .map(|(name, ty)| (*name, Substitution::apply(&mut typed.types, *ty, subst)))
        .collect();
    typed
        .layout
        .specialized_structs
        .insert(key.clone(), StructLayout { fields });
    typed
        .layout
        .specialized_type_ids
        .insert(key.clone(), type_id);
}

fn specialize_enum(
    typed: &mut TypedProgram,
    base: DefId,
    key: &TypeMonoKey,
    subst: &Substitution,
    type_id: u32,
) {
    let Some(template) = typed.layout.enums.get(&base).cloned() else {
        return;
    };
    let variants: Vec<VariantLayout> = template
        .variants
        .iter()
        .map(|v| {
            let kind = match &v.kind {
                VariantKind::Unit => VariantKind::Unit,
                VariantKind::Tuple(ts) => {
                    let pts: Vec<_> = ts
                        .iter()
                        .map(|t| Substitution::apply(&mut typed.types, *t, subst))
                        .collect();
                    VariantKind::Tuple(pts)
                }
                VariantKind::Struct(fs) => {
                    let fields: Vec<_> = fs
                        .iter()
                        .map(|(n, t)| (*n, Substitution::apply(&mut typed.types, *t, subst)))
                        .collect();
                    VariantKind::Struct(fields)
                }
            };
            VariantLayout {
                def: v.def,
                name: v.name,
                tag: v.tag,
                kind,
            }
        })
        .collect();
    typed.layout.specialized_enums.insert(
        key.clone(),
        EnumLayout {
            enum_def: base,
            variants,
        },
    );
    typed
        .layout
        .specialized_type_ids
        .insert(key.clone(), type_id);
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

fn generic_param_defs_for_fn_base(resolved: &ResolvedProgram, base: DefId) -> Option<Vec<DefId>> {
    let f = find_function(resolved, base)?;
    generic_param_defs(resolved, f.generics.as_deref(), base)
}

/// Returns generic parameter defs for a struct, enum, or type alias template.
#[must_use]
pub fn generic_param_defs_for_type(resolved: &ResolvedProgram, base: DefId) -> Option<Vec<DefId>> {
    generic_param_defs(
        resolved,
        generic_params_for_def(resolved, base).as_deref(),
        base,
    )
}

fn generic_params_for_def(resolved: &ResolvedProgram, base: DefId) -> Option<Vec<GenericParam>> {
    let def = resolved.defs.get(base.index() as usize)?;
    for module in &resolved.modules {
        if module.id != def.module {
            continue;
        }
        for item in &module.program.items {
            match &item.inner.decl {
                TopLevelDecl::Struct { name, generics, .. }
                    if def.kind == DefKind::Struct && name.symbol == def.name =>
                {
                    return generics.clone();
                }
                TopLevelDecl::Enum { name, generics, .. }
                    if def.kind == DefKind::Enum && name.symbol == def.name =>
                {
                    return generics.clone();
                }
                TopLevelDecl::TypeAlias { name, generics, .. }
                    if def.kind == DefKind::TypeAlias && name.symbol == def.name =>
                {
                    return generics.clone();
                }
                _ => {}
            }
        }
    }
    None
}

fn generic_param_defs(
    resolved: &ResolvedProgram,
    generics: Option<&[GenericParam]>,
    base: DefId,
) -> Option<Vec<DefId>> {
    let params = generics?;
    if params.is_empty() {
        return Some(Vec::new());
    }
    let module = resolved.defs[base.index() as usize].module;
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
