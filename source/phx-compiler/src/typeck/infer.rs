//! Local call-site type inference for generic instantiations.

use std::collections::HashMap;

use crate::resolver::{Def, DefId};

use super::types::{Ty, TypeId, TypeInterner};
use super::unify::{AliasEnv, same_type};

/// Fresh type variables and bindings for one call-site inference scope.
#[derive(Debug, Default)]
pub struct InferenceCtx {
    next_var: u32,
    bindings: HashMap<u32, TypeId>,
}

impl InferenceCtx {
    /// Creates an empty inference context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates a fresh inference variable.
    pub fn fresh_var(&mut self, types: &mut TypeInterner) -> TypeId {
        let id = self.next_var;
        self.next_var = self.next_var.saturating_add(1);
        types.intern(&Ty::Var(id))
    }

    /// Unifies two types under local inference rules.
    ///
    /// Returns `false` on conflict.
    pub fn unify(
        &mut self,
        types: &mut TypeInterner,
        defs: &[Def],
        value_types: &HashMap<DefId, TypeId>,
        a: TypeId,
        b: TypeId,
    ) -> bool {
        let a = self.resolve(types, a);
        let b = self.resolve(types, b);
        if Self::same_type_readonly(types, defs, value_types, a, b) {
            return true;
        }
        match (types.get(a).clone(), types.get(b).clone()) {
            (Ty::Var(va), Ty::Var(vb)) => {
                if va == vb {
                    true
                } else if let Some(&bound) = self.bindings.get(&vb) {
                    self.bindings.insert(va, bound);
                    true
                } else {
                    self.bindings.insert(va, b);
                    true
                }
            }
            (Ty::Var(v), _) => self.bind_var(types, defs, value_types, v, b),
            (_, Ty::Var(v)) => self.bind_var(types, defs, value_types, v, a),
            _ => false,
        }
    }

    fn same_type_readonly(
        types: &TypeInterner,
        defs: &[Def],
        value_types: &HashMap<DefId, TypeId>,
        a: TypeId,
        b: TypeId,
    ) -> bool {
        let env = AliasEnv {
            types,
            defs,
            value_types,
        };
        same_type(&env, a, b)
    }

    fn bind_var(
        &mut self,
        types: &mut TypeInterner,
        defs: &[Def],
        value_types: &HashMap<DefId, TypeId>,
        var_id: u32,
        concrete: TypeId,
    ) -> bool {
        if matches!(types.get(concrete), Ty::Var(_)) {
            let var_ty = types.intern(&Ty::Var(var_id));
            return self.unify(types, defs, value_types, var_ty, concrete);
        }
        if let Some(&existing) = self.bindings.get(&var_id) {
            return self.unify(types, defs, value_types, existing, concrete);
        }
        self.bindings.insert(var_id, concrete);
        true
    }

    /// Resolves inference variables to a concrete type.
    #[must_use]
    pub fn resolve(&mut self, types: &mut TypeInterner, id: TypeId) -> TypeId {
        match types.get(id).clone() {
            Ty::Var(var_id) => {
                if let Some(&bound) = self.bindings.get(&var_id) {
                    let resolved = self.resolve(types, bound);
                    self.bindings.insert(var_id, resolved);
                    resolved
                } else {
                    id
                }
            }
            _ => id,
        }
    }

    /// Returns `true` when `id` is a fully resolved concrete type (no free vars).
    #[must_use]
    pub fn is_resolved(&mut self, types: &mut TypeInterner, id: TypeId) -> bool {
        let resolved = self.resolve(types, id);
        !matches!(types.get(resolved), Ty::Var(_))
    }
}
