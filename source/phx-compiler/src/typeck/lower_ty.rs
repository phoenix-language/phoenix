//! Lower AST [`Type`] nodes to interned [`Ty`].

use phx_syntax::ast::Node;
use phx_syntax::ast::ident::TypeName;
use phx_syntax::ast::types::Type;
use phx_syntax::token::Keyword;

use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::{DefId, DefKind};

/// Maps type names to definition ids (module + generic scopes).
/// Returns `true` when `ty` names post-MVP std `Option` / `Result` (not compiler builtins).
#[must_use]
pub fn is_post_mvp_std_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Generic {
            name: Keyword::Option | Keyword::Result,
            ..
        } | Type::Primitive(Keyword::Option | Keyword::Result)
    )
}

pub type TypeDefMap = std::collections::HashMap<phx_syntax::Symbol, DefId>;

/// Lowers `ty` using `type_defs` for named types.
#[must_use]
pub fn lower_type(types: &mut TypeInterner, type_defs: &TypeDefMap, ty: &Type) -> TypeId {
    lower_type_inner(types, type_defs, ty)
}

fn lower_type_inner(types: &mut TypeInterner, type_defs: &TypeDefMap, ty: &Type) -> TypeId {
    match ty {
        Type::Primitive(k) => types.intern(&Ty::Primitive(*k)),
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
                types.intern(&Ty::Unit)
            }
        }
        Type::Generic { name, args: _ } => match name {
            Keyword::Option | Keyword::Result => types.intern(&Ty::Unit),
            other => types.intern(&Ty::Primitive(*other)),
        },
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
        _ => types.intern(&Ty::Unit),
    }
}

fn lower_type_node(types: &mut TypeInterner, type_defs: &TypeDefMap, node: &Node<Type>) -> TypeId {
    lower_type_inner(types, type_defs, &node.inner)
}

fn lookup_type_def(type_defs: &TypeDefMap, name: &TypeName) -> Option<DefId> {
    type_defs.get(&name.symbol).copied()
}

/// Builds a map of type names to defs from resolved definitions.
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

/// Pushes generic params into `type_defs` for the duration of a scope.
pub fn push_generics(
    type_defs: &mut TypeDefMap,
    defs: &[crate::resolver::Def],
    generics: Option<&[phx_syntax::ast::types::GenericParam]>,
) {
    if let Some(params) = generics {
        for param in params {
            if let Some(id) = find_def_by_name(defs, param.name.symbol) {
                type_defs.insert(param.name.symbol, id);
            }
        }
    }
}

fn find_def_by_name(defs: &[crate::resolver::Def], name: phx_syntax::Symbol) -> Option<DefId> {
    defs.iter()
        .enumerate()
        .find(|(_, d)| d.name == name)
        .map(|(i, _)| DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
}
