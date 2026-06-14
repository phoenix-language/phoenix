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
            (a_ty, b_ty) => self.unify_concrete(types, defs, value_types, &a_ty, &b_ty),
        }
    }

    fn unify_concrete(
        &mut self,
        types: &mut TypeInterner,
        defs: &[Def],
        value_types: &HashMap<DefId, TypeId>,
        a: &Ty,
        b: &Ty,
    ) -> bool {
        match (a, b) {
            (
                Ty::Named {
                    def: def_a,
                    args: args_a,
                },
                Ty::Named {
                    def: def_b,
                    args: args_b,
                },
            ) => {
                if def_a != def_b || args_a.len() != args_b.len() {
                    return false;
                }
                for (arg_a, arg_b) in args_a.iter().zip(args_b) {
                    if !self.unify(types, defs, value_types, *arg_a, *arg_b) {
                        return false;
                    }
                }
                true
            }
            (Ty::Tuple(elems_a), Ty::Tuple(elems_b)) => {
                if elems_a.len() != elems_b.len() {
                    return false;
                }
                for (elem_a, elem_b) in elems_a.iter().zip(elems_b) {
                    if !self.unify(types, defs, value_types, *elem_a, *elem_b) {
                        return false;
                    }
                }
                true
            }
            (
                Ty::Array {
                    elem: elem_a,
                    len: len_a,
                },
                Ty::Array {
                    elem: elem_b,
                    len: len_b,
                },
            ) => len_a == len_b && self.unify(types, defs, value_types, *elem_a, *elem_b),
            (Ty::Slice(inner_a), Ty::Slice(inner_b)) => {
                self.unify(types, defs, value_types, *inner_a, *inner_b)
            }
            (
                Ty::Ref {
                    mut_: mut_a,
                    inner: inner_a,
                },
                Ty::Ref {
                    mut_: mut_b,
                    inner: inner_b,
                },
            ) => mut_a == mut_b && self.unify(types, defs, value_types, *inner_a, *inner_b),
            (
                Ty::Ptr {
                    mut_: mut_a,
                    inner: inner_a,
                },
                Ty::Ptr {
                    mut_: mut_b,
                    inner: inner_b,
                },
            ) => mut_a == mut_b && self.unify(types, defs, value_types, *inner_a, *inner_b),
            (
                Ty::Fn {
                    params: params_a,
                    ret: ret_a,
                },
                Ty::Fn {
                    params: params_b,
                    ret: ret_b,
                },
            ) => {
                if params_a.len() != params_b.len() {
                    return false;
                }
                for (param_a, param_b) in params_a.iter().zip(params_b) {
                    if !self.unify(types, defs, value_types, *param_a, *param_b) {
                        return false;
                    }
                }
                self.unify(types, defs, value_types, *ret_a, *ret_b)
            }
            (Ty::Primitive(_), Ty::Primitive(_))
            | (Ty::Unit, Ty::Unit)
            | (Ty::Error, Ty::Error)
            | (Ty::Str, Ty::Str) => false,
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::typeck::builtins::{bool_type, int_literal_type};

    fn empty_env() -> (Vec<Def>, HashMap<DefId, TypeId>) {
        (Vec::new(), HashMap::new())
    }

    #[test]
    fn unify_named_binds_inner_var() {
        let mut types = TypeInterner::new();
        let (defs, value_types) = empty_env();
        let mut infer = InferenceCtx::new();
        let def = DefId::from_raw(0);
        let var = infer.fresh_var(&mut types);
        let s32 = int_literal_type(&mut types, false);
        let a = types.intern(&Ty::Named {
            def,
            args: vec![var],
        });
        let b = types.intern(&Ty::Named {
            def,
            args: vec![s32],
        });
        assert!(infer.unify(&mut types, &defs, &value_types, a, b));
        assert_eq!(infer.resolve(&mut types, var), s32);
    }

    #[test]
    fn unify_named_mismatch_def_or_args_fails() {
        let mut types = TypeInterner::new();
        let (defs, value_types) = empty_env();
        let mut infer = InferenceCtx::new();
        let def_a = DefId::from_raw(0);
        let def_b = DefId::from_raw(1);
        let s32 = int_literal_type(&mut types, false);
        let a = types.intern(&Ty::Named {
            def: def_a,
            args: vec![s32],
        });
        let b = types.intern(&Ty::Named {
            def: def_b,
            args: vec![s32],
        });
        assert!(!infer.unify(&mut types, &defs, &value_types, a, b));

        let c = types.intern(&Ty::Named {
            def: def_a,
            args: vec![s32, s32],
        });
        assert!(!infer.unify(&mut types, &defs, &value_types, a, c));
    }

    #[test]
    fn unify_tuple_binds_inner_var() {
        let mut types = TypeInterner::new();
        let (defs, value_types) = empty_env();
        let mut infer = InferenceCtx::new();
        let var = infer.fresh_var(&mut types);
        let s32 = int_literal_type(&mut types, false);
        let a = types.intern(&Ty::Tuple(vec![var, s32]));
        let b = types.intern(&Ty::Tuple(vec![s32, s32]));
        assert!(infer.unify(&mut types, &defs, &value_types, a, b));
        assert_eq!(infer.resolve(&mut types, var), s32);
    }

    #[test]
    fn unify_ref_recurses() {
        let mut types = TypeInterner::new();
        let (defs, value_types) = empty_env();
        let mut infer = InferenceCtx::new();
        let var = infer.fresh_var(&mut types);
        let s32 = int_literal_type(&mut types, false);
        let a = types.intern(&Ty::Ref {
            mut_: false,
            inner: var,
        });
        let b = types.intern(&Ty::Ref {
            mut_: false,
            inner: s32,
        });
        assert!(infer.unify(&mut types, &defs, &value_types, a, b));
        assert_eq!(infer.resolve(&mut types, var), s32);

        let mut_a = types.intern(&Ty::Ref {
            mut_: true,
            inner: s32,
        });
        assert!(!infer.unify(&mut types, &defs, &value_types, a, mut_a));
    }

    #[test]
    fn unify_conflict_after_bind_fails() {
        let mut types = TypeInterner::new();
        let (defs, value_types) = empty_env();
        let mut infer = InferenceCtx::new();
        let def = DefId::from_raw(0);
        let var = infer.fresh_var(&mut types);
        let s32 = int_literal_type(&mut types, false);
        let bool_ty = bool_type(&mut types);
        infer.bindings.insert(0, s32);
        let a = types.intern(&Ty::Named {
            def,
            args: vec![var],
        });
        let b = types.intern(&Ty::Named {
            def,
            args: vec![bool_ty],
        });
        assert!(!infer.unify(&mut types, &defs, &value_types, a, b));
    }

    #[test]
    fn unify_primitive_mismatch_fails() {
        let mut types = TypeInterner::new();
        let (defs, value_types) = empty_env();
        let mut infer = InferenceCtx::new();
        let s32 = int_literal_type(&mut types, false);
        let bool_ty = bool_type(&mut types);
        assert!(!infer.unify(&mut types, &defs, &value_types, s32, bool_ty));
    }
}
