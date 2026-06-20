//! Lower v2 [`PxiType`] trees into the type checker on import.
//!
//! ## Pass role
//!
//! When a dependent module loads a fresh dependency `.pxi`, the resolver has already bound
//! imported symbols to [`DefId`] values. This module converts each export's structured
//! `type` field into interned [`TypeId`] values so the importer can seed
//! [`TypedProgram::value_types`](crate::typeck::TypedProgram::value_types) without
//! re-parsing the dependency body.
//!
//! ## Named paths
//!
//! [`PxiType::Named`] paths (`logical_module::TypeName`) are resolved through
//! [`build_named_def_paths`] and [`PxiImportCtx::named_defs`]. Unresolved names lower to
//! [`Ty::Error`] rather than aborting the import pass.
//!
//! ## Cycles
//!
//! Recursive type trees (for example nested generics) are lowered with a pointer cache so
//! shared subtrees and self-referential shapes intern once.

use std::collections::HashMap;

use phx_syntax::Interner;
use phx_syntax::token::Keyword;

use crate::resolver::{DefId, DefKind};
use crate::typeck::{Ty, TypeId, TypeInterner};

use super::type_ast::{PxiField, PxiType};

/// Context for lowering imported [`PxiType`] values into the importer's type arena.
pub struct PxiImportCtx<'a> {
    /// Type interner that receives lowered [`Ty`] nodes.
    pub types: &'a mut TypeInterner,
    /// Symbol interner (reserved for future path resolution helpers).
    #[allow(dead_code)]
    pub interner: &'a Interner,
    /// `logical_module::TypeName` → [`DefId`] for type definitions already bound in this crate.
    pub named_defs: &'a HashMap<String, DefId>,
}

/// Lowers a [`PxiType`] tree to an interned [`TypeId`].
///
/// Unknown [`PxiType::Named`] paths become [`Ty::Error`]. Struct and enum shapes that appear
/// only as inline trees (not via `Named`) are tuple-expanded per Phoenix layout rules.
#[must_use]
pub fn pxi_type_to_ty(ctx: &mut PxiImportCtx<'_>, pxi: &PxiType) -> TypeId {
    let mut cache: HashMap<*const PxiType, TypeId> = HashMap::new();
    pxi_type_to_ty_inner(ctx, pxi, &mut cache)
}

fn pxi_type_to_ty_inner(
    ctx: &mut PxiImportCtx<'_>,
    pxi: &PxiType,
    cache: &mut HashMap<*const PxiType, TypeId>,
) -> TypeId {
    let ptr = std::ptr::from_ref(pxi);
    if let Some(&id) = cache.get(&ptr) {
        return id;
    }
    let id = match pxi {
        PxiType::Primitive(name) => {
            if name == "str" {
                ctx.types.intern(&Ty::Str)
            } else {
                let k = parse_keyword(name).unwrap_or(Keyword::S32);
                ctx.types.intern(&Ty::Primitive(k))
            }
        }
        PxiType::Unit => ctx.types.intern(&Ty::Unit),
        PxiType::Named { path, args } => {
            if let Some(&def) = ctx.named_defs.get(path) {
                let args: Vec<_> = args
                    .iter()
                    .map(|a| pxi_type_to_ty_inner(ctx, a, cache))
                    .collect();
                ctx.types.intern(&Ty::Named { def, args })
            } else {
                ctx.types.intern(&Ty::Error)
            }
        }
        PxiType::Tuple(elems) => {
            let elems: Vec<_> = elems
                .iter()
                .map(|e| pxi_type_to_ty_inner(ctx, e, cache))
                .collect();
            ctx.types.intern(&Ty::Tuple(elems))
        }
        PxiType::Array { elem, len } => {
            let e = pxi_type_to_ty_inner(ctx, elem, cache);
            ctx.types.intern(&Ty::Array { elem: e, len: *len })
        }
        PxiType::Slice(elem) => {
            let e = pxi_type_to_ty_inner(ctx, elem, cache);
            ctx.types.intern(&Ty::Slice(e))
        }
        PxiType::Ref { mut_, inner } => {
            let i = pxi_type_to_ty_inner(ctx, inner, cache);
            ctx.types.intern(&Ty::Ref {
                mut_: *mut_,
                inner: i,
            })
        }
        PxiType::Ptr { mut_, inner } => {
            let i = pxi_type_to_ty_inner(ctx, inner, cache);
            ctx.types.intern(&Ty::Ptr {
                mut_: *mut_,
                inner: i,
            })
        }
        PxiType::Fn { params, ret } => {
            let params: Vec<_> = params
                .iter()
                .map(|p| pxi_type_to_ty_inner(ctx, p, cache))
                .collect();
            let r = pxi_type_to_ty_inner(ctx, ret, cache);
            ctx.types.intern(&Ty::Fn { params, ret: r })
        }
        PxiType::Struct { fields } => struct_from_fields(ctx, fields, cache),
        PxiType::Enum { variants: _ } => {
            // Importers use enum types via Named paths; full enum layout comes from same module.
            ctx.types.intern(&Ty::Error)
        }
        PxiType::Alias(inner) => pxi_type_to_ty_inner(ctx, inner, cache),
    };
    cache.insert(ptr, id);
    id
}

fn struct_from_fields(
    ctx: &mut PxiImportCtx<'_>,
    fields: &[PxiField],
    cache: &mut HashMap<*const PxiType, TypeId>,
) -> TypeId {
    if fields.is_empty() {
        return ctx.types.intern(&Ty::Unit);
    }
    if fields.len() == 1 {
        return pxi_type_to_ty_inner(ctx, &fields[0].ty, cache);
    }
    let elems: Vec<_> = fields
        .iter()
        .map(|f| pxi_type_to_ty_inner(ctx, &f.ty, cache))
        .collect();
    ctx.types.intern(&Ty::Tuple(elems))
}

fn parse_keyword(name: &str) -> Option<Keyword> {
    Some(match name {
        "s8" => Keyword::S8,
        "s16" => Keyword::S16,
        "s32" => Keyword::S32,
        "s64" => Keyword::S64,
        "s128" => Keyword::S128,
        "u8" => Keyword::U8,
        "u16" => Keyword::U16,
        "u32" => Keyword::U32,
        "u64" => Keyword::U64,
        "u128" => Keyword::U128,
        "bool" => Keyword::Bool,
        "f32" => Keyword::F32,
        "f64" => Keyword::F64,
        "str" => Keyword::Str,
        _ => return None,
    })
}

/// Builds `logical_module::Name` → [`DefId`] for aggregate and alias definitions.
///
/// Includes structs, enums, type aliases, and traits from `resolved` so imported
/// [`PxiType::Named`] paths can be wired to local [`DefId`] values during hydration.
#[must_use]
pub fn build_named_def_paths(
    resolved: &crate::resolver::ResolvedProgram,
) -> HashMap<String, DefId> {
    let mut map = HashMap::new();
    let interner = &resolved.interner;
    for (i, def) in resolved.defs.iter().enumerate() {
        if !matches!(
            def.kind,
            DefKind::Struct | DefKind::Enum | DefKind::TypeAlias | DefKind::Trait
        ) {
            continue;
        }
        let def_id = DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX));
        let module_path = resolved
            .modules
            .iter()
            .find(|m| m.id == def.module)
            .map_or("main", |m| m.logical_path.as_str());
        let name = interner.resolve(def.name).unwrap_or("<?>");
        map.insert(format!("{module_path}::{name}"), def_id);
    }
    map
}

/// Seeds `value_types` for one imported export from its structured `.pxi` type.
///
/// Struct and enum exports store the lowered aggregate type directly. Type aliases unwrap
/// the alias target; all other defs store the lowered export type as-is.
pub fn seed_value_type(
    ctx: &mut PxiImportCtx<'_>,
    def_id: DefId,
    kind: DefKind,
    pxi: &PxiType,
    value_types: &mut HashMap<DefId, TypeId>,
) {
    if matches!(kind, DefKind::Struct | DefKind::Enum)
        && matches!(pxi, PxiType::Struct { .. } | PxiType::Enum { .. })
    {
        let tid = pxi_type_to_ty(ctx, pxi);
        value_types.insert(def_id, tid);
        return;
    }
    let ty = if let (DefKind::TypeAlias, PxiType::Alias(inner)) = (kind, pxi) {
        pxi_type_to_ty(ctx, inner)
    } else {
        pxi_type_to_ty(ctx, pxi)
    };
    value_types.insert(def_id, ty);
}
