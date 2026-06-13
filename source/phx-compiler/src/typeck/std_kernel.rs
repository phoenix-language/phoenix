//! Canonical `std::core::option` / `std::core::result` definitions for `?` sugar.
//!
//! Built once per program when std modules are linked. Does not introduce `Ty::Option` /
//! `Ty::Result` — only records [`DefId`]s of ordinary std generic enums.

use phx_syntax::Interner;

use super::bindings::LocalSlot;
use super::layout::ProgramLayout;
use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::{DefId, DefKind, ResolvedProgram};

/// Logical module paths for std core enums.
const STD_OPTION_MODULE: &str = "std::core::option";
const STD_RESULT_MODULE: &str = "std::core::result";

/// Canonical std `Option` / `Result` definition ids and variant tags.
#[derive(Debug, Clone, Default)]
pub struct StdKernel {
    /// `std::core::option::Option` enum template.
    pub option_enum: Option<DefId>,
    /// `std::core::result::Result` enum template.
    pub result_enum: Option<DefId>,
    /// `Some` variant ctor.
    pub some_variant: Option<DefId>,
    /// `None` variant ctor.
    pub none_variant: Option<DefId>,
    /// `Ok` variant ctor.
    pub ok_variant: Option<DefId>,
    /// `Err` variant ctor.
    pub err_variant: Option<DefId>,
}

impl StdKernel {
    /// Scans `resolved` for bundled std core enum definitions.
    #[must_use]
    pub fn build(resolved: &ResolvedProgram, _layout: &ProgramLayout) -> Self {
        let interner = &resolved.interner;
        let option_mod = module_id(resolved, STD_OPTION_MODULE);
        let result_mod = module_id(resolved, STD_RESULT_MODULE);
        let mut kernel = Self::default();
        if let Some(mod_id) = option_mod {
            kernel.option_enum = find_def(resolved, interner, mod_id, "Option", DefKind::Enum);
            kernel.some_variant =
                find_def(resolved, interner, mod_id, "Some", DefKind::EnumVariant);
            kernel.none_variant =
                find_def(resolved, interner, mod_id, "None", DefKind::EnumVariant);
        }
        if let Some(mod_id) = result_mod {
            kernel.result_enum = find_def(resolved, interner, mod_id, "Result", DefKind::Enum);
            kernel.ok_variant = find_def(resolved, interner, mod_id, "Ok", DefKind::EnumVariant);
            kernel.err_variant = find_def(resolved, interner, mod_id, "Err", DefKind::EnumVariant);
        }
        kernel
    }

    /// Returns `true` when `ty` is a monomorphized std `Option<…>`.
    #[must_use]
    pub fn is_std_option(&self, types: &TypeInterner, ty: TypeId) -> bool {
        self.option_enum
            .is_some_and(|def| enum_template(types, ty) == Some(def))
    }

    /// Returns `true` when `ty` is a monomorphized std `Result<…, …>`.
    #[must_use]
    pub fn is_std_result(&self, types: &TypeInterner, ty: TypeId) -> bool {
        self.result_enum
            .is_some_and(|def| enum_template(types, ty) == Some(def))
    }

    /// Payload type `T` from `Option<T>`.
    #[must_use]
    pub fn option_payload_ty(&self, types: &TypeInterner, ty: TypeId) -> Option<TypeId> {
        if !self.is_std_option(types, ty) {
            return None;
        }
        let Ty::Named { args, .. } = types.get(ty) else {
            return None;
        };
        args.first().copied()
    }

    /// `(ok, err)` types from `Result<ok, err>`.
    #[must_use]
    pub fn result_ok_err_tys(&self, types: &TypeInterner, ty: TypeId) -> Option<(TypeId, TypeId)> {
        if !self.is_std_result(types, ty) {
            return None;
        }
        let Ty::Named { args, .. } = types.get(ty) else {
            return None;
        };
        if args.len() < 2 {
            return None;
        }
        Some((args[0], args[1]))
    }

    /// Success variant tag (`Some` / `Ok`) for a std `Option` or `Result` scrutinee.
    #[must_use]
    pub fn success_tag_for(
        &self,
        layout: &ProgramLayout,
        types: &TypeInterner,
        ty: TypeId,
    ) -> Option<u32> {
        let enum_def = enum_template(types, ty)?;
        if self.option_enum == Some(enum_def) {
            variant_tag(layout, enum_def, self.some_variant)
        } else if self.result_enum == Some(enum_def) {
            variant_tag(layout, enum_def, self.ok_variant)
        } else {
            None
        }
    }

    /// Failure variant tag (`None` / `Err`) for a std `Option` or `Result` scrutinee.
    #[must_use]
    pub fn failure_tag_for(
        &self,
        layout: &ProgramLayout,
        types: &TypeInterner,
        ty: TypeId,
    ) -> Option<u32> {
        let enum_def = enum_template(types, ty)?;
        if self.option_enum == Some(enum_def) {
            variant_tag(layout, enum_def, self.none_variant)
        } else if self.result_enum == Some(enum_def) {
            variant_tag(layout, enum_def, self.err_variant)
        } else {
            None
        }
    }
}

fn module_id(resolved: &ResolvedProgram, logical_path: &str) -> Option<u32> {
    resolved
        .modules
        .iter()
        .find(|m| m.logical_path == logical_path)
        .map(|m| m.id)
}

fn find_def(
    resolved: &ResolvedProgram,
    interner: &Interner,
    module: u32,
    name: &str,
    kind: DefKind,
) -> Option<DefId> {
    for (i, def) in resolved.defs.iter().enumerate() {
        if def.module == module && def.kind == kind && interner.resolves_to(def.name, name) {
            return Some(DefId::from_raw(u32::try_from(i).ok()?));
        }
    }
    None
}

fn variant_tag(layout: &ProgramLayout, enum_def: DefId, variant: Option<DefId>) -> Option<u32> {
    let variant = variant?;
    layout.variants.get(&variant).and_then(|meta| {
        if meta.enum_def == enum_def {
            Some(meta.tag)
        } else {
            None
        }
    })
}

fn enum_template(types: &TypeInterner, ty: TypeId) -> Option<DefId> {
    match types.get(ty) {
        Ty::Named { def, .. } => Some(*def),
        _ => None,
    }
}

/// How the failure arm of `expr?` is lowered.
#[derive(Debug, Clone)]
pub enum TryFailureMode {
    /// V0-042: return scrutinee enum unchanged (Option or identical Result).
    ReturnScrutinee,
    /// V0-059: convert Err payload via monomorphized `From::from`.
    ConvertErr {
        /// Err payload type from the scrutinee `Result`.
        err_in_ty: TypeId,
        /// Err type of the enclosing function return `Result`.
        err_out_ty: TypeId,
        /// Enclosing function return type (`Result<T, E_out>`).
        return_result_ty: TypeId,
        /// `From::from` on `E_out: From<E_in>`.
        from_fn: DefId,
    },
}

/// Metadata for lowering `expr?` recorded during type-check.
#[derive(Debug, Clone)]
pub struct TrySiteMeta {
    /// Enum template definition (`Option` or `Result`).
    pub enum_def: DefId,
    /// Monomorphization type arguments for the scrutinee enum.
    pub enum_args: Vec<TypeId>,
    /// Scrutinee enum type (`Option<…>` or `Result<…, …>`).
    pub scrutinee_ty: TypeId,
    /// Unwrapped success payload type.
    pub success_ty: TypeId,
    /// Tag for `Some` / `Ok`.
    pub success_tag: u32,
    /// Tag for `None` / `Err`.
    pub failure_tag: u32,
    /// Local slot holding the scrutinee enum during `?` lowering.
    pub temp_slot: LocalSlot,
    /// Failure-arm lowering strategy.
    pub failure_mode: TryFailureMode,
}
