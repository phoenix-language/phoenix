//! Type substitution for explicit generic instantiation.
//!
//! [`Substitution`] maps generic parameter [`DefId`]s to concrete [`TypeId`]s when checking or
//! cloning monomorphized definitions.

use std::collections::HashMap;

use crate::resolver::{DefId, DefKind, ResolvedProgram};

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

    /// Returns the concrete type for `param`, if bound directly or via same-symbol alias.
    #[must_use]
    pub fn concrete_for_generic(&self, def: DefId, resolved: &ResolvedProgram) -> Option<TypeId> {
        if let Some(concrete) = self.map.get(&def).copied() {
            return Some(concrete);
        }
        let def_rec = resolved.defs.get(def.index() as usize)?;
        if def_rec.kind != DefKind::GenericParam {
            return None;
        }
        for (&param, &concrete) in &self.map {
            let Some(param_def) = resolved.defs.get(param.index() as usize) else {
                continue;
            };
            if param_def.kind == DefKind::GenericParam
                && param_def.name == def_rec.name
                && param_def.module == def_rec.module
            {
                return Some(concrete);
            }
        }
        None
    }

    /// Binds every same-symbol [`DefKind::GenericParam`] in `module` to match existing entries.
    ///
    /// Struct and impl templates each declare their own generic parameters; lowering may
    /// resolve to either declaration's [`DefId`]. Monomorphization keys the impl params.
    pub fn extend_generic_param_aliases(&mut self, resolved: &ResolvedProgram, module: u32) {
        let mut aliases = Vec::new();
        for (&primary, &concrete) in &self.map {
            let Some(primary_def) = resolved.defs.get(primary.index() as usize) else {
                continue;
            };
            if primary_def.kind != DefKind::GenericParam {
                continue;
            }
            let symbol = primary_def.name;
            for (i, def) in resolved.defs.iter().enumerate() {
                if def.module != module || def.kind != DefKind::GenericParam || def.name != symbol {
                    continue;
                }
                let Ok(id) = u32::try_from(i) else {
                    continue;
                };
                let id = DefId::from_raw(id);
                if id != primary {
                    aliases.push((id, concrete));
                }
            }
        }
        for (id, concrete) in aliases {
            self.map.entry(id).or_insert(concrete);
        }
    }

    /// Applies `subst` throughout `id`, interning fresh nodes when needed.
    #[must_use]
    pub fn apply(
        types: &mut TypeInterner,
        id: TypeId,
        subst: &Self,
        resolved: &ResolvedProgram,
    ) -> TypeId {
        if subst.map.is_empty() {
            return id;
        }
        Self::apply_inner(types, id, subst, resolved, &mut Vec::new())
    }

    fn apply_inner(
        types: &mut TypeInterner,
        id: TypeId,
        subst: &Self,
        resolved: &ResolvedProgram,
        depth: &mut Vec<TypeId>,
    ) -> TypeId {
        if depth.contains(&id) {
            return id;
        }
        depth.push(id);
        let out = match types.get(id).clone() {
            Ty::Named { def, args } => {
                if let Some(concrete) = subst.concrete_for_generic(def, resolved) {
                    depth.pop();
                    return concrete;
                }
                let args: Vec<_> = args
                    .iter()
                    .map(|a| Self::apply_inner(types, *a, subst, resolved, depth))
                    .collect();
                types.intern(&Ty::Named { def, args })
            }
            Ty::Tuple(elems) => {
                let elems: Vec<_> = elems
                    .iter()
                    .map(|e| Self::apply_inner(types, *e, subst, resolved, depth))
                    .collect();
                types.intern(&Ty::Tuple(elems))
            }
            Ty::Array { elem, len } => {
                let elem = Self::apply_inner(types, elem, subst, resolved, depth);
                types.intern(&Ty::Array { elem, len })
            }
            Ty::Slice(inner) => {
                let inner = Self::apply_inner(types, inner, subst, resolved, depth);
                types.intern(&Ty::Slice(inner))
            }
            Ty::Ref { mut_, inner } => {
                let inner = Self::apply_inner(types, inner, subst, resolved, depth);
                types.intern(&Ty::Ref { mut_, inner })
            }
            Ty::Ptr { mut_, inner } => {
                let inner = Self::apply_inner(types, inner, subst, resolved, depth);
                types.intern(&Ty::Ptr { mut_, inner })
            }
            Ty::Fn { params, ret } => {
                let params: Vec<_> = params
                    .iter()
                    .map(|p| Self::apply_inner(types, *p, subst, resolved, depth))
                    .collect();
                let ret = Self::apply_inner(types, ret, subst, resolved, depth);
                types.intern(&Ty::Fn { params, ret })
            }
            Ty::Primitive(_) | Ty::Unit | Ty::Error | Ty::Var(_) | Ty::Str => id,
        };
        depth.pop();
        out
    }
}
