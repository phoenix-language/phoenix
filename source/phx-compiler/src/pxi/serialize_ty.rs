//! Map typeck [`Ty`] and layout tables to [`PxiType`] for `.pxi` v2 emission.
//!
//! ## Pass role
//!
//! Called by [`crate::pxi::emit::build_pxi_for_module`] when constructing format-v2 export
//! records. Converts interned [`TypeId`] values and per-definition layout metadata into the
//! structured type trees stored in each [`PxiExport::ty`](super::format::PxiExport::ty).
//!
//! ## Path qualification
//!
//! [`Ty::Named`] nodes serialize as `logical_module::Name` strings relative to the module
//! being emitted. Cross-module references keep the exporter's logical path prefix so importers
//! can resolve them through [`super::import_types::build_named_def_paths`].
//!
//! ## Signatures
//!
//! [`signature_string`] preserves the v1-compatible textual signature used in manifest diffs
//! alongside the structured v2 `type` object.

use phx_syntax::token::Keyword;
use phx_syntax::{Interner, Symbol};

use crate::resolver::{Def, DefId};
use crate::typeck::{EnumLayout, ProgramLayout, StructLayout, VariantKind};
use crate::typeck::{Ty, TypeId, TypeInterner, format_type};

use super::type_ast::{PxiField, PxiType, PxiVariant, PxiVariantPayload};

/// Serializes one interned type as a [`PxiType`] tree for `logical_module`.
///
/// Type variables, error types, and unit collapse to [`PxiType::Unit`] in export form.
#[must_use]
pub fn ty_to_pxi(
    types: &TypeInterner,
    interner: &Interner,
    defs: &[Def],
    layout: &ProgramLayout,
    logical_module: &str,
    ty: TypeId,
) -> PxiType {
    ty_to_pxi_inner(types, interner, defs, layout, logical_module, ty)
}

#[allow(clippy::only_used_in_recursion)]
fn ty_to_pxi_inner(
    types: &TypeInterner,
    interner: &Interner,
    defs: &[Def],
    layout: &ProgramLayout,
    logical_module: &str,
    ty: TypeId,
) -> PxiType {
    match types.get(ty) {
        Ty::Primitive(k) => PxiType::Primitive(keyword_name(*k).to_owned()),
        Ty::Unit | Ty::Error | Ty::Var(_) => PxiType::Unit,
        Ty::Named { def, args } => {
            let path = named_path(defs, interner, logical_module, *def);
            let args: Vec<_> = args
                .iter()
                .map(|&a| ty_to_pxi_inner(types, interner, defs, layout, logical_module, a))
                .collect();
            PxiType::Named { path, args }
        }
        Ty::Tuple(elems) => {
            let elems: Vec<_> = elems
                .iter()
                .map(|&e| ty_to_pxi_inner(types, interner, defs, layout, logical_module, e))
                .collect();
            PxiType::Tuple(elems)
        }
        Ty::Array { elem, len } => PxiType::Array {
            elem: Box::new(ty_to_pxi_inner(
                types,
                interner,
                defs,
                layout,
                logical_module,
                *elem,
            )),
            len: *len,
        },
        Ty::Slice(elem) => PxiType::Slice(Box::new(ty_to_pxi_inner(
            types,
            interner,
            defs,
            layout,
            logical_module,
            *elem,
        ))),
        Ty::Str => PxiType::Primitive("str".to_owned()),
        Ty::Ref { mut_, inner } => PxiType::Ref {
            mut_: *mut_,
            inner: Box::new(ty_to_pxi_inner(
                types,
                interner,
                defs,
                layout,
                logical_module,
                *inner,
            )),
        },
        Ty::Ptr { mut_, inner } => PxiType::Ptr {
            mut_: *mut_,
            inner: Box::new(ty_to_pxi_inner(
                types,
                interner,
                defs,
                layout,
                logical_module,
                *inner,
            )),
        },
        Ty::Fn { params, ret } => PxiType::Fn {
            params: params
                .iter()
                .map(|&p| ty_to_pxi_inner(types, interner, defs, layout, logical_module, p))
                .collect(),
            ret: Box::new(ty_to_pxi_inner(
                types,
                interner,
                defs,
                layout,
                logical_module,
                *ret,
            )),
        },
    }
}

fn pxi_symbol_name(interner: &Interner, sym: Symbol) -> String {
    interner.resolve_display(sym)
}

/// Builds the structured export type for a struct definition from its layout table.
#[must_use]
pub fn struct_export_type(
    types: &TypeInterner,
    interner: &Interner,
    defs: &[Def],
    layout: &ProgramLayout,
    logical_module: &str,
    def_id: DefId,
    sl: &StructLayout,
) -> PxiType {
    let fields: Vec<PxiField> = sl
        .fields
        .iter()
        .map(|(sym, tid)| PxiField {
            name: pxi_symbol_name(interner, *sym),
            ty: ty_to_pxi(types, interner, defs, layout, logical_module, *tid),
        })
        .collect();
    let _ = def_id;
    PxiType::Struct { fields }
}

/// Builds export type for an enum definition.
#[must_use]
pub fn enum_export_type(
    types: &TypeInterner,
    interner: &Interner,
    defs: &[Def],
    layout: &ProgramLayout,
    logical_module: &str,
    el: &EnumLayout,
) -> PxiType {
    let variants: Vec<PxiVariant> = el
        .variants
        .iter()
        .map(|v| {
            let payload = match &v.kind {
                VariantKind::Unit => PxiVariantPayload::Unit,
                VariantKind::Tuple(payload) => PxiVariantPayload::Tuple(
                    payload
                        .iter()
                        .map(|&t| ty_to_pxi(types, interner, defs, layout, logical_module, t))
                        .collect(),
                ),
                VariantKind::Struct(fields) => PxiVariantPayload::Struct(
                    fields
                        .iter()
                        .map(|(sym, tid)| PxiField {
                            name: pxi_symbol_name(interner, *sym),
                            ty: ty_to_pxi(types, interner, defs, layout, logical_module, *tid),
                        })
                        .collect(),
                ),
            };
            PxiVariant {
                name: pxi_symbol_name(interner, v.name),
                tag: v.tag,
                payload,
            }
        })
        .collect();
    PxiType::Enum { variants }
}

/// Builds a function export type from parameter and return types.
#[must_use]
pub fn fn_export_type(
    types: &TypeInterner,
    interner: &Interner,
    defs: &[Def],
    layout: &ProgramLayout,
    logical_module: &str,
    params: &[TypeId],
    ret: TypeId,
) -> PxiType {
    PxiType::Fn {
        params: params
            .iter()
            .map(|&p| ty_to_pxi(types, interner, defs, layout, logical_module, p))
            .collect(),
        ret: Box::new(ty_to_pxi(
            types,
            interner,
            defs,
            layout,
            logical_module,
            ret,
        )),
    }
}

/// Returns the v1-compatible textual type signature for manifest diffing.
///
/// Delegates to [`format_type`] so legacy `signature` fields stay aligned with structured
/// v2 exports.
#[must_use]
pub fn signature_string(
    types: &TypeInterner,
    interner: &Interner,
    defs: &[Def],
    ty: TypeId,
) -> String {
    format_type(types, interner, defs, ty)
}

fn named_path(defs: &[Def], interner: &Interner, logical_module: &str, def: DefId) -> String {
    let Some(d) = defs.get(def.index() as usize) else {
        return format!("{logical_module}::?");
    };
    let name = interner.resolve(d.name).unwrap_or("<?>");
    let _ = d.module;
    format!("{logical_module}::{name}")
}

#[allow(clippy::match_same_arms)]
fn keyword_name(k: Keyword) -> &'static str {
    match k {
        Keyword::S8 => "s8",
        Keyword::S16 => "s16",
        Keyword::S32 => "s32",
        Keyword::S64 => "s64",
        Keyword::S128 => "s128",
        Keyword::U8 => "u8",
        Keyword::U16 => "u16",
        Keyword::U32 => "u32",
        Keyword::U64 => "u64",
        Keyword::U128 => "u128",
        Keyword::Bool => "bool",
        Keyword::F32 => "f32",
        Keyword::F64 => "f64",
        Keyword::Str => "str",
        _ => "s32",
    }
}
