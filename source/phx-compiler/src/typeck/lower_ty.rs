//! Lower AST [`Type`] syntax nodes to interned [`Ty`] values.
//!
//! Translates surface type annotations into the type checker's internal representation.
//! Shared by declaration collection (fields, signatures, trait bounds) and expression
//! checking (casts, `as` targets, type-ascription contexts).
//!
//! # Role in type checking
//!
//! Runs during the [`super::check`] walk whenever a type appears in the AST. Does not perform
//! semantic validation beyond name lookup — unresolved type names produce [`Ty::Error`] via
//! [`error_type`]. Generic parameters are injected into the lookup map by [`push_generics`]
//! for the duration of a scoped check (function body, impl block, etc.).
//!
//! # Type name resolution
//!
//! [`build_type_def_map`] indexes all struct, enum, type alias, generic param, and trait
//! definitions by interned name symbol. [`lower_type`] resolves [`Type::Named`] through this
//! map; a missing entry yields [`Ty::Error`], which downstream passes treat as a poison type.
//!
//! # Supported syntax shapes
//!
//! Maps each [`Type`] variant to the corresponding [`Ty`]:
//!
//! - Primitives and `()` → [`Ty::Primitive`] / [`Ty::Unit`]; the `str` keyword → [`Ty::Str`].
//! - Named types with optional generic arguments → [`Ty::Named`].
//! - Function, reference, pointer, tuple, array, and slice forms → matching [`Ty`] variants.
//! - [`Type::SelfAssoc`] is not yet supported and lowers to [`Ty::Error`].

use phx_syntax::ast::Node;
use phx_syntax::ast::ident::TypeName;
use phx_syntax::ast::types::Type;

use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::{DefId, DefKind, ResolutionKey, ResolvedProgram};

/// Maps interned type name symbols to their defining [`DefId`].
///
/// Built once per compilation unit by [`build_type_def_map`] and extended locally by
/// [`push_generics`] when entering generic scopes.
pub type TypeDefMap = std::collections::HashMap<phx_syntax::Symbol, DefId>;

/// Lowers a syntax [`Type`] annotation to an interned [`TypeId`].
///
/// Recursively lowers nested types (generics, tuple elements, reference inner types, etc.).
/// Unresolved named types intern [`Ty::Error`].
#[must_use]
pub fn lower_type(types: &mut TypeInterner, type_defs: &TypeDefMap, ty: &Type) -> TypeId {
    lower_type_inner(types, type_defs, ty)
}

fn lower_type_inner(types: &mut TypeInterner, type_defs: &TypeDefMap, ty: &Type) -> TypeId {
    match ty {
        Type::Primitive(k) => {
            if *k == phx_syntax::token::Keyword::Str {
                types.intern(&Ty::Str)
            } else {
                types.intern(&Ty::Primitive(*k))
            }
        }
        Type::Named { name, generics } => {
            let def = lookup_type_def(type_defs, name);
            let args = generics
                .as_ref()
                .map(|gs| {
                    gs.iter()
                        .map(|g| lower_type_node(types, type_defs, g))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(def) = def {
                types.intern(&Ty::Named { def, args })
            } else {
                types.intern(&Ty::Error)
            }
        }
        Type::Function { params, ret } => {
            let ps: Vec<_> = params
                .iter()
                .map(|p| lower_type_node(types, type_defs, p))
                .collect();
            let r = lower_type_node(types, type_defs, ret);
            types.intern(&Ty::Fn { params: ps, ret: r })
        }
        Type::Ref { mut_, inner } => {
            let i = lower_type_node(types, type_defs, inner);
            types.intern(&Ty::Ref {
                mut_: *mut_,
                inner: i,
            })
        }
        Type::Ptr { mut_, inner } => {
            let i = lower_type_node(types, type_defs, inner);
            types.intern(&Ty::Ptr {
                mut_: *mut_,
                inner: i,
            })
        }
        Type::Tuple(ts) => {
            let elems: Vec<_> = ts
                .iter()
                .map(|t| lower_type_node(types, type_defs, t))
                .collect();
            types.intern(&Ty::Tuple(elems))
        }
        Type::Unit => types.intern(&Ty::Unit),
        Type::Array { elem, len } => {
            let e = lower_type_node(types, type_defs, elem);
            let length = u32::try_from(len.value).unwrap_or(0);
            types.intern(&Ty::Array {
                elem: e,
                len: length,
            })
        }
        Type::Slice(inner) => {
            let i = lower_type_node(types, type_defs, inner);
            types.intern(&Ty::Slice(i))
        }
        Type::SelfAssoc { .. } => types.intern(&Ty::Error),
    }
}

/// Returns the interned poison type ([`Ty::Error`]) for unresolved or invalid types.
///
/// Callers use this id to propagate failure without aborting the check walk. Multiple calls
/// may share the same interned [`Ty::Error`] node within a [`TypeInterner`].
#[must_use]
pub fn error_type(types: &mut TypeInterner) -> TypeId {
    types.intern(&Ty::Error)
}

fn lower_type_node(types: &mut TypeInterner, type_defs: &TypeDefMap, node: &Node<Type>) -> TypeId {
    lower_type_inner(types, type_defs, &node.inner)
}

fn lookup_type_def(type_defs: &TypeDefMap, name: &TypeName) -> Option<DefId> {
    type_defs.get(&name.symbol).copied()
}

/// Builds a name-to-definition map from the resolved definition table.
///
/// Includes structs, enums, type aliases, generic parameters, and traits — every definition
/// kind that may appear as a type name in surface syntax.
#[must_use]
pub fn build_type_def_map(defs: &[crate::resolver::Def]) -> TypeDefMap {
    let mut map = TypeDefMap::new();
    for (index, def) in defs.iter().enumerate() {
        let id = DefId::from_raw(u32::try_from(index).unwrap_or(u32::MAX));
        if matches!(
            def.kind,
            DefKind::Struct
                | DefKind::Enum
                | DefKind::TypeAlias
                | DefKind::GenericParam
                | DefKind::Trait
        ) {
            map.insert(def.name, id);
        }
    }
    map
}

/// Inserts generic parameter names into `type_defs` for the duration of a scoped check.
///
/// Resolves each parameter through [`ResolvedProgram::resolutions`] first, then falls back to
/// a module-local [`DefKind::GenericParam`] search. Parameters that cannot be resolved are
/// skipped silently — [`lower_type`] will produce [`Ty::Error`] if they are referenced.
pub fn push_generics(
    type_defs: &mut TypeDefMap,
    resolved: &ResolvedProgram,
    module: u32,
    generics: Option<&[phx_syntax::ast::types::GenericParam]>,
) {
    if let Some(params) = generics {
        for param in params {
            let key = ResolutionKey {
                module,
                node_id: param.name.id,
            };
            let id = resolved
                .resolutions
                .get(&key)
                .copied()
                .or_else(|| find_generic_param(&resolved.defs, module, param.name.symbol));
            if let Some(id) = id {
                type_defs.insert(param.name.symbol, id);
            }
        }
    }
}

fn find_generic_param(
    defs: &[crate::resolver::Def],
    module: u32,
    name: phx_syntax::Symbol,
) -> Option<DefId> {
    defs.iter().enumerate().find_map(|(i, d)| {
        if d.module == module && d.kind == DefKind::GenericParam && d.name == name {
            Some(DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
        } else {
            None
        }
    })
}
