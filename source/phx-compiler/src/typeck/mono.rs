//! Monomorphization: duplicate generic functions and type layouts for explicit instantiation sites.

use std::collections::HashMap;

use phx_diagnostics::{TypeCheckBag, TypeCheckError};
use phx_syntax::ast::decl::{Function, ImplMember, TopLevelDecl};
use phx_syntax::ast::types::GenericParam;

use super::bindings::FunctionLayout;
use super::bounds::validate_instantiation_bounds;
use super::check::TypeChecker;
use super::layout::{EnumLayout, StructLayout, TypeMonoKey, VariantKind, VariantLayout};
use super::mangle;
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
    /// Module that owns emitted bytecode for this specialization (usually the call-site crate).
    pub owner_module: u32,
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
pub(crate) fn monomorphize(
    typed: &mut TypedProgram,
    fn_insts: &[MonoInst],
    type_insts: &[TypeMonoInst],
) -> TypeCheckBag {
    let mut bag = TypeCheckBag::new();
    monomorphize_functions(typed, fn_insts, &mut bag);
    monomorphize_types(typed, type_insts, &mut bag);
    patch_specialized_drop_fns(typed);
    patch_associated_fn_sites(typed);
    patch_method_call_sites(typed);
    bag
}

fn patch_method_call_sites(typed: &mut TypedProgram) {
    let sites: Vec<_> = typed.method_call_sites.keys().copied().collect();
    for site in sites {
        let template = match typed.method_call_sites.get(&site) {
            Some(def) => *def,
            None => continue,
        };
        if typed.specialized_from.contains_key(&template) {
            continue;
        }
        for inst in &typed.mono_insts {
            if inst.base_fn != template {
                continue;
            }
            if let Some(spec) = specialized_fn_for_inst(typed, template, &inst.args) {
                typed.method_call_sites.insert(site, spec);
                break;
            }
        }
    }
}

fn patch_associated_fn_sites(typed: &mut TypedProgram) {
    let sites: Vec<_> = typed.associated_fn_sites.keys().copied().collect();
    for site in sites {
        let template = match typed.associated_fn_sites.get(&site) {
            Some(def) => *def,
            None => continue,
        };
        if typed.specialized_from.contains_key(&template) {
            continue;
        }
        for inst in &typed.mono_insts {
            if inst.base_fn != template {
                continue;
            }
            if let Some(spec) = specialized_fn_for_inst(typed, template, &inst.args) {
                typed.associated_fn_sites.insert(site, spec);
                break;
            }
        }
    }
}

fn patch_specialized_drop_fns(typed: &mut TypedProgram) {
    let mut patches: Vec<(usize, usize, DefId)> = Vec::new();
    for (layout_idx, layout) in typed.functions.iter().enumerate() {
        for (event_idx, event) in layout.drop_events.iter().enumerate() {
            let super::types::Ty::Named { def, args } = typed.types.get(event.ty).clone() else {
                continue;
            };
            let Some(template_drop) = super::builtins::resolve_drop_fn(
                &typed.layout,
                &typed.resolved,
                &typed.std_trait_kernel,
                def,
                &args,
            ) else {
                continue;
            };
            if event.drop_fn != template_drop {
                continue;
            }
            if let Some(spec) = specialized_fn_for_inst(typed, template_drop, &args) {
                patches.push((layout_idx, event_idx, spec));
            }
        }
    }
    for (layout_idx, event_idx, spec) in patches {
        typed.functions[layout_idx].drop_events[event_idx].drop_fn = spec;
    }
}

/// Resolves the monomorphized `DefId` for `base_fn` instantiated at `args`, if any.
#[must_use]
pub(crate) fn specialized_fn_for_inst(
    typed: &TypedProgram,
    base_fn: DefId,
    args: &[super::types::TypeId],
) -> Option<DefId> {
    let expected =
        mangle::mangle_symbol_for_specialization(&typed.resolved, base_fn, args, &typed.types);
    typed.specialized_from.iter().find_map(|(spec, base)| {
        if *base != base_fn {
            return None;
        }
        typed
            .resolved
            .interner
            .resolves_to(typed.resolved.defs[spec.index() as usize].name, &expected)
            .then_some(*spec)
    })
}

#[allow(clippy::too_many_lines)]
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
        let Some(f) = super::trait_defaults::lookup_function(typed, inst.base_fn).cloned() else {
            continue;
        };
        let combined_generics = combined_generic_params(&typed.resolved, inst.base_fn, &f);
        if !validate_instantiation_bounds(
            &typed.resolved,
            &typed.layout,
            &mut typed.types,
            &typed.std_trait_kernel,
            &typed.value_types,
            Some(&combined_generics),
            &param_defs,
            &inst.args,
            base_def.module,
            base_def.span,
            bag,
        ) {
            continue;
        }
        let mut subst = Substitution::new();
        for (param, arg) in param_defs.iter().zip(&inst.args) {
            subst.insert(*param, *arg);
        }
        let base_module = base_def.module;
        let base_span = base_def.span;
        let spec_def = match alloc_specialized_def(
            &mut typed.resolved,
            inst.base_fn,
            &inst.args,
            &typed.types,
            inst.owner_module,
        ) {
            Ok(id) => id,
            Err(AllocSpecializedDefError::Intern(phx_syntax::InternError::TableFull)) => {
                bag.push(
                    base_module,
                    TypeCheckError::UnsupportedFeature {
                        feature: "interner table full during monomorphization",
                        span: base_span,
                    },
                );
                continue;
            }
            Err(AllocSpecializedDefError::ProgramTooLarge) => {
                bag.push(
                    base_module,
                    TypeCheckError::ProgramTooLarge { span: base_span },
                );
                continue;
            }
        };
        typed.specialized_from.insert(spec_def, inst.base_fn);
        if typed
            .fn_effective_unsafe
            .get(&inst.base_fn)
            .copied()
            .unwrap_or(false)
        {
            typed.fn_effective_unsafe.insert(spec_def, true);
        }
        let skip_impl_body = find_impl_generics_for_fn(&typed.resolved, inst.base_fn)
            .is_some_and(|params| !params.is_empty())
            && f.generics.as_ref().is_none_or(Vec::is_empty);
        if skip_impl_body {
            clone_specialized_function_layout(typed, inst.base_fn, spec_def, &subst);
        } else {
            let mut checker = TypeChecker::new_with_substitution(
                &typed.resolved,
                subst,
                std::mem::take(&mut typed.types),
            );
            checker.set_expr_id_base(expr_base);
            checker.seed_layout_tables(&typed.layout);
            checker.seed_std_kernel(&typed.std_kernel);
            checker.seed_std_trait_kernel(&typed.std_trait_kernel);
            checker.seed_intrinsic_kernel(&typed.intrinsic_kernel);
            checker.seed_value_types(&typed.value_types);
            checker.seed_fn_effective_unsafe(&typed.fn_effective_unsafe);
            checker.check_function_specialized(&f, spec_def, inst.base_fn, &inst.args);
            let (
                checker_types,
                expr_types,
                checker_bag,
                layouts,
                _program_layout,
                value_types,
                spec_aliases,
                try_sites,
                associated_fn_sites,
                method_call_sites,
                indirect_call_sites,
                intrinsic_call_sites,
                size_of_literals,
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
            typed.try_sites.extend(try_sites);
            typed.associated_fn_sites.extend(associated_fn_sites);
            typed.method_call_sites.extend(method_call_sites);
            typed.indirect_call_sites.extend(indirect_call_sites);
            typed.intrinsic_call_sites.extend(intrinsic_call_sites);
            typed.size_of_literals.extend(size_of_literals);
            typed.value_types.extend(value_types);
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
        for def in typed.associated_fn_sites.values_mut() {
            if *def == inst.base_fn {
                *def = spec_def;
            }
        }
        for def in typed.method_call_sites.values_mut() {
            if *def == inst.base_fn {
                *def = spec_def;
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
        let generic_params = generic_params_for_def(&typed.resolved, inst.base_def);
        if !validate_instantiation_bounds(
            &typed.resolved,
            &typed.layout,
            &mut typed.types,
            &typed.std_trait_kernel,
            &typed.value_types,
            generic_params.as_deref(),
            &param_defs,
            &inst.args,
            base_def.module,
            base_def.span,
            bag,
        ) {
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
            match &item.inner.decl {
                TopLevelDecl::Function(f)
                    if fn_def_id(resolved, module.id, f.name.symbol) == Some(def) =>
                {
                    return Some(f);
                }
                TopLevelDecl::Impl { members, .. } => {
                    for m in members {
                        if let ImplMember::Method(f) = m {
                            if fn_def_id(resolved, module.id, f.name.symbol) == Some(def) {
                                return Some(f);
                            }
                        }
                    }
                }
                _ => {}
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

pub(crate) fn generic_param_defs_for_fn_base(
    resolved: &ResolvedProgram,
    base: DefId,
) -> Option<Vec<DefId>> {
    let f = find_function(resolved, base)?;
    let params = combined_generic_params(resolved, base, f);
    if params.is_empty() {
        return Some(Vec::new());
    }
    generic_param_defs(resolved, Some(&params), base)
}

fn combined_generic_params(
    resolved: &ResolvedProgram,
    base: DefId,
    f: &Function,
) -> Vec<GenericParam> {
    let mut params = find_impl_generics_for_fn(resolved, base).unwrap_or_default();
    if let Some(fn_generics) = f.generics.as_ref() {
        params.extend(fn_generics.clone());
    }
    params
}

fn clone_specialized_function_layout(
    typed: &mut TypedProgram,
    base_fn: DefId,
    spec_def: DefId,
    subst: &Substitution,
) {
    let Some(base_layout) = typed.functions.iter().find(|l| l.def == base_fn) else {
        return;
    };
    let mut layout = base_layout.clone();
    layout.def = spec_def;
    layout.return_type = Substitution::apply(&mut typed.types, layout.return_type, subst);
    for binding in &mut layout.bindings {
        binding.ty = Substitution::apply(&mut typed.types, binding.ty, subst);
    }
    reserve_impl_method_scratch_temps(typed, &mut layout);
    typed.functions.push(layout);
}

/// Scratch locals for `emit_ref_receiver_from_stack_value` when body typeck omitted match temps.
const IMPL_METHOD_SCRATCH_TEMPS: u32 = 4;

fn reserve_impl_method_scratch_temps(typed: &mut TypedProgram, layout: &mut FunctionLayout) {
    use phx_syntax::scratch_binding_symbol;

    use super::bindings::{Binding, BindingKind, LocalSlot};
    use super::builtins::unit;

    let scratch_ty = unit(&mut typed.types);
    for _ in 0..IMPL_METHOD_SCRATCH_TEMPS {
        let symbol = scratch_binding_symbol(
            u32::try_from(layout.match_temp_slots.len()).unwrap_or(u32::MAX),
        );
        let slot = LocalSlot::from_raw(u32::try_from(layout.bindings.len()).unwrap_or(u32::MAX));
        layout.bindings.push(Binding {
            symbol,
            slot,
            ty: scratch_ty,
            kind: BindingKind::MatchTemp,
            scope_depth: 0,
            utf8_rodata: None,
        });
        layout.match_temp_slots.push(slot);
    }
}

fn find_impl_generics_for_fn(resolved: &ResolvedProgram, base: DefId) -> Option<Vec<GenericParam>> {
    let base_def = resolved.defs.get(base.index() as usize)?;
    for module in &resolved.modules {
        if module.id != base_def.module {
            continue;
        }
        for item in &module.program.items {
            if let TopLevelDecl::Impl {
                generics, members, ..
            } = &item.inner.decl
            {
                for member in members {
                    if let ImplMember::Method(f) = member {
                        if fn_def_id(resolved, module.id, f.name.symbol) == Some(base) {
                            return generics.clone();
                        }
                    }
                }
            }
        }
    }
    None
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

/// Returns generic parameters declared on a struct, enum, type alias, or function template.
#[must_use]
pub fn generic_params_for_def(
    resolved: &ResolvedProgram,
    base: DefId,
) -> Option<Vec<GenericParam>> {
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
    owner_module: u32,
) -> Result<DefId, AllocSpecializedDefError> {
    let base_def = &resolved.defs[base.index() as usize];
    let mangled = mangle::mangle_symbol_for_specialization(resolved, base, args, types);
    let sym = resolved
        .interner
        .intern(&mangled)
        .map_err(AllocSpecializedDefError::Intern)?;
    let def = Def::new(
        DefKind::Fn,
        sym,
        base_def.span,
        owner_module,
        base_def.exported,
        base_def.scope_depth,
    );
    let id = DefId::try_from_index(resolved.defs.len())
        .map_err(|_| AllocSpecializedDefError::ProgramTooLarge)?;
    resolved.defs.push(def);
    Ok(id)
}

/// Failure while allocating a monomorphized function definition.
enum AllocSpecializedDefError {
    /// Interner table is full.
    Intern(phx_syntax::InternError),
    /// Definition table would exceed `u32::MAX`.
    ProgramTooLarge,
}

/// Cross-crate generic export requested by a consumer package build.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CrossCrateMonoReq {
    /// Path-dependency package name (e.g. `math`).
    pub dep_package: String,
    /// Logical module path of the generic template (e.g. `math`).
    pub logical_module: String,
    /// Unmangled template function name (e.g. `id`).
    pub base_name: String,
    /// Concrete type argument spellings in generic-parameter order.
    pub arg_types: Vec<String>,
    /// Mangled export symbol (e.g. `id$s32`).
    pub mangled_name: String,
}

/// Collects monomorphization requests for exported generics defined in path dependencies.
#[must_use]
pub(crate) fn collect_cross_crate_mono_reqs(
    typed: &TypedProgram,
    workspace_package: &str,
    module_logical: impl Fn(u32) -> Option<String>,
) -> Vec<CrossCrateMonoReq> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for inst in &typed.mono_insts {
        let base_def = &typed.resolved.defs[inst.base_fn.index() as usize];
        let Some(logical) = module_logical(base_def.module) else {
            continue;
        };
        if module_in_workspace_package(&logical, workspace_package) {
            continue;
        }
        let dep_package = logical
            .split("::")
            .next()
            .unwrap_or(logical.as_str())
            .to_owned();
        let base_name = typed.resolved.interner.resolve_display(base_def.name);
        let arg_types: Vec<_> = inst
            .args
            .iter()
            .map(|a| {
                super::display::format_type(
                    &typed.types,
                    &typed.resolved.interner,
                    &typed.resolved.defs,
                    *a,
                )
            })
            .collect();
        let mangled_name = mangle::mangle_symbol_for_specialization(
            &typed.resolved,
            inst.base_fn,
            &inst.args,
            &typed.types,
        );
        let key = (logical.clone(), mangled_name.clone());
        if seen.insert(key) {
            out.push(CrossCrateMonoReq {
                dep_package,
                logical_module: logical,
                base_name,
                arg_types,
                mangled_name,
            });
        }
    }
    out
}

/// Applies an injected monomorphization worklist (e.g. when rebuilding a dependency lib).
///
/// # Errors
///
/// Returns a [`TypeCheckBag`] when specialization fails.
#[must_use]
pub(crate) fn apply_mono_worklist(
    typed: &mut TypedProgram,
    reqs: &[CrossCrateMonoReq],
    module_logical: impl Fn(u32) -> Option<String>,
) -> TypeCheckBag {
    let mut insts = Vec::new();
    for req in reqs {
        if specialized_export_exists(typed, &req.mangled_name) {
            continue;
        }
        let Some(base_fn) = find_fn_by_name(
            &typed.resolved,
            &module_logical,
            &req.logical_module,
            &req.base_name,
        ) else {
            continue;
        };
        let mut args = Vec::with_capacity(req.arg_types.len());
        let mut ok = true;
        for spelling in &req.arg_types {
            let Some(arg) = resolve_type_spelling(typed, spelling) else {
                ok = false;
                break;
            };
            args.push(arg);
        }
        if !ok {
            continue;
        }
        let owner_module = typed
            .resolved
            .modules
            .iter()
            .find(|m| m.logical_path == req.logical_module)
            .map(|m| m.id)
            .unwrap_or_else(|| typed.resolved.defs[base_fn.index() as usize].module);
        insts.push(MonoInst {
            base_fn,
            args,
            call_sites: Vec::new(),
            owner_module,
        });
    }
    monomorphize(typed, &insts, &[])
}

fn module_in_workspace_package(logical: &str, workspace: &str) -> bool {
    logical == workspace || logical.starts_with(&format!("{workspace}::"))
}

fn specialized_export_exists(typed: &TypedProgram, mangled_name: &str) -> bool {
    typed
        .resolved
        .defs
        .iter()
        .any(|d| d.kind == DefKind::Fn && typed.resolved.interner.resolves_to(d.name, mangled_name))
}

/// Returns true when `def_id` is an impl method on a generic type (not a monomorphized specialization).
#[must_use]
pub(crate) fn is_generic_impl_method_template(typed: &TypedProgram, def_id: DefId) -> bool {
    if typed.specialized_from.contains_key(&def_id) {
        return false;
    }
    let Some(type_def) = impl_type_def_for_method(typed, def_id) else {
        return false;
    };
    generic_param_defs_for_type(&typed.resolved, type_def).is_some_and(|params| !params.is_empty())
}

fn impl_type_def_for_method(typed: &TypedProgram, fn_def: DefId) -> Option<DefId> {
    let base_def = typed.resolved.defs.get(fn_def.index() as usize)?;
    let interner = &typed.resolved.interner;
    for module in &typed.resolved.modules {
        if module.id != base_def.module {
            continue;
        }
        for item in &module.program.items {
            let TopLevelDecl::Impl {
                type_name, members, ..
            } = &item.inner.decl
            else {
                continue;
            };
            if !members
                .iter()
                .any(|m| matches!(m, ImplMember::Method(f) if fn_def_matches(typed, f, fn_def)))
            {
                continue;
            }
            let name = interner.resolve(type_name.symbol).unwrap_or("<?>");
            return find_type_def_in_module(&typed.resolved, module.id, name);
        }
    }
    None
}

fn find_type_def_in_module(resolved: &ResolvedProgram, module: u32, name: &str) -> Option<DefId> {
    for (i, def) in resolved.defs.iter().enumerate() {
        if def.module != module {
            continue;
        }
        if !matches!(def.kind, DefKind::Struct | DefKind::Enum) {
            continue;
        }
        if resolved.interner.resolves_to(def.name, name) {
            return Some(DefId::from_raw(u32::try_from(i).ok()?));
        }
    }
    None
}

/// Returns true when `def_id` is a generic function template (not a monomorphized specialization).
#[must_use]
pub(crate) fn is_generic_fn_template(typed: &TypedProgram, def_id: DefId) -> bool {
    if typed.specialized_from.contains_key(&def_id) {
        return false;
    }
    if typed.specialized_from.values().any(|&base| base == def_id) {
        return true;
    }
    fn_decl_has_type_params(typed, def_id)
}

fn fn_decl_has_type_params(typed: &TypedProgram, def_id: DefId) -> bool {
    let Some(def) = typed.resolved.defs.get(def_id.index() as usize) else {
        return false;
    };
    for module in &typed.resolved.modules {
        if module.id != def.module {
            continue;
        }
        for item in &module.program.items {
            match &item.inner.decl {
                TopLevelDecl::Function(f) if fn_def_matches(typed, f, def_id) => {
                    return f.generics.as_ref().is_some_and(|g| !g.is_empty());
                }
                TopLevelDecl::Impl { members, .. } => {
                    for m in members {
                        if let ImplMember::Method(f) = m {
                            if fn_def_matches(typed, f, def_id) {
                                return f.generics.as_ref().is_some_and(|g| !g.is_empty());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    false
}

fn fn_def_matches(typed: &TypedProgram, f: &Function, def: DefId) -> bool {
    typed
        .resolved
        .defs
        .get(def.index() as usize)
        .is_some_and(|d| d.name == f.name.symbol && d.kind == DefKind::Fn)
}

fn find_fn_by_name(
    resolved: &ResolvedProgram,
    module_logical: &impl Fn(u32) -> Option<String>,
    logical_module: &str,
    name: &str,
) -> Option<DefId> {
    resolved.defs.iter().enumerate().find_map(|(i, d)| {
        if d.kind != DefKind::Fn {
            return None;
        }
        let log = module_logical(d.module)?;
        if log != logical_module {
            return None;
        }
        if !resolved.interner.resolves_to(d.name, name) {
            return None;
        }
        Some(DefId::from_raw(u32::try_from(i).ok()?))
    })
}

fn resolve_type_spelling(typed: &mut TypedProgram, spelling: &str) -> Option<TypeId> {
    use phx_syntax::token::Keyword;
    let norm = spelling.trim();
    let keywords = [
        Keyword::S8,
        Keyword::S16,
        Keyword::S32,
        Keyword::S64,
        Keyword::S128,
        Keyword::U8,
        Keyword::U16,
        Keyword::U32,
        Keyword::U64,
        Keyword::U128,
        Keyword::F32,
        Keyword::F64,
        Keyword::Bool,
    ];
    for kw in keywords {
        if format!("{kw:?}").eq_ignore_ascii_case(norm) {
            return Some(typed.types.intern(&super::types::Ty::Primitive(kw)));
        }
    }
    if norm.eq_ignore_ascii_case("()") || norm == "unit" {
        return Some(typed.types.intern(&super::types::Ty::Unit));
    }
    None
}
