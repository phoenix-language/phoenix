//! Type substitution for explicit generic instantiation.

use std::collections::HashMap;

use crate::resolver::DefId;

use super::types::{Ty, TypeId, TypeInterner};

/// Maps generic parameter definitions to concrete types at an instantiation site.
#[derive(Debug, Clone, Default)]
pub struct Substitution {
    map: HashMap<DefId, TypeId>,
}

impl Substitution {
    /// Creates an empty substitution.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Binds generic parameter `param` to concrete type `ty`.
    pub fn insert(&mut self, param: DefId, ty: TypeId) {
        self.map.insert(param, ty);
    }

    /// Returns the concrete type for `param`, if bound.
    #[must_use]
    pub fn get(&self, param: DefId) -> Option<TypeId> {
        self.map.get(&param).copied()
    }

    /// Applies `subst` throughout `id`, interning fresh nodes when needed.
    #[must_use]
    pub fn apply(types: &mut TypeInterner, id: TypeId, subst: &Self) -> TypeId {
        if subst.map.is_empty() {
            return id;
        }
        Self::apply_inner(types, id, subst, &mut Vec::new())
    }

    fn apply_inner(
        types: &mut TypeInterner,
        id: TypeId,
        subst: &Self,
        depth: &mut Vec<TypeId>,
    ) -> TypeId {
        if depth.contains(&id) {
            return id;
        }
        depth.push(id);
        let out = match types.get(id).clone() {
            Ty::Named { def, args } => {
                if let Some(concrete) = subst.get(def) {
                    depth.pop();
                    return concrete;
                }
                let args: Vec<_> = args
                    .iter()
                    .map(|a| Self::apply_inner(types, *a, subst, depth))
                    .collect();
                types.intern(&Ty::Named { def, args })
            }
            Ty::Tuple(elems) => {
                let elems: Vec<_> = elems
                    .iter()
                    .map(|e| Self::apply_inner(types, *e, subst, depth))
                    .collect();
                types.intern(&Ty::Tuple(elems))
            }
            Ty::Array { elem, len } => {
                let elem = Self::apply_inner(types, elem, subst, depth);
                types.intern(&Ty::Array { elem, len })
            }
            Ty::Slice(inner) => {
                let inner = Self::apply_inner(types, inner, subst, depth);
                types.intern(&Ty::Slice(inner))
            }
            Ty::Ref { mut_, inner } => {
                let inner = Self::apply_inner(types, inner, subst, depth);
                types.intern(&Ty::Ref { mut_, inner })
            }
            Ty::Ptr { mut_, inner } => {
                let inner = Self::apply_inner(types, inner, subst, depth);
                types.intern(&Ty::Ptr { mut_, inner })
            }
            Ty::Fn { params, ret } => {
                let params: Vec<_> = params
                    .iter()
                    .map(|p| Self::apply_inner(types, *p, subst, depth))
                    .collect();
                let ret = Self::apply_inner(types, ret, subst, depth);
                types.intern(&Ty::Fn { params, ret })
            }
            Ty::Primitive(_) | Ty::Unit | Ty::Error | Ty::Var(_) => id,
        };
        depth.pop();
        out
    }
}
