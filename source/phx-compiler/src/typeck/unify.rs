//! Type equality and branch unification.
//!
//! [`same_type`] compares types after expanding [`DefKind::TypeAlias`] chains via
//! [`normalize_type`]. Used for `if`/`match` branch joins, pattern compatibility checks,
//! and verifying that explicit generic arguments match inferred types.
//!
//! # Alias normalization
//!
//! [`normalize_type`] walks [`Ty::Named`] heads that resolve to type aliases, substituting
//! each alias with its underlying type from [`AliasEnv::value_types`]. A visited-set prevents
//! infinite loops on cyclic alias definitions; when a cycle is detected the current id is
//! returned unchanged and downstream equality may report a mismatch.
//!
//! # Branch unification
//!
//! [`unify_branch`] is the join operator for conditional expressions: when both arms have
//! the same normalized type, that type is the result; otherwise the join fails (`None`) and
//! the checker emits a branch type mismatch diagnostic.
//!
//! This module does **not** introduce inference variables — call-site inference lives in
//! [`super::infer::InferenceCtx`]. Substitution of explicit generic arguments lives in
//! [`super::subst::Substitution`].

use std::collections::{HashMap, HashSet};

use crate::resolver::{Def, DefId, DefKind};

use super::types::{Ty, TypeId, TypeInterner};

/// Context for resolving type aliases to their underlying types.
///
/// Bundles the intern pool, definition table, and per-definition type map needed to expand
/// [`DefKind::TypeAlias`] during [`normalize_type`] and [`same_type`].
pub struct AliasEnv<'a> {
    /// Interned type pool shared by the type checker.
    pub types: &'a TypeInterner,
    /// All definitions in the compilation unit (used to detect alias heads).
    pub defs: &'a [Def],
    /// Type of each definition; for `TypeAlias` entries this is the underlying type.
    pub value_types: &'a HashMap<DefId, TypeId>,
}

/// Expands [`DefKind::TypeAlias`] definitions to their underlying type.
///
/// Follows alias chains through [`AliasEnv::value_types`] until a non-alias [`Ty::Named`]
/// head or a non-named type is reached. Tracks visited [`TypeId`]s to stop on cycles.
///
/// # Panics
///
/// Never panics on malformed input. Missing definitions or unmapped aliases terminate
/// expansion at the current type.
#[must_use]
pub fn normalize_type(env: &AliasEnv<'_>, id: TypeId) -> TypeId {
    let mut current = id;
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current) {
            return current;
        }
        let Ty::Named { def, .. } = env.types.get(current) else {
            return current;
        };
        let Some(def_record) = env.defs.get(def.index() as usize) else {
            return current;
        };
        if def_record.kind != DefKind::TypeAlias {
            return current;
        }
        let Some(&underlying) = env.value_types.get(def) else {
            return current;
        };
        current = underlying;
    }
}

/// Returns `true` when `a` and `b` denote the same type after alias normalization.
///
/// Compares [`normalize_type`] results for structural identity of [`TypeId`] indices
/// (interned types with equal structure share ids).
#[must_use]
pub fn same_type(env: &AliasEnv<'_>, a: TypeId, b: TypeId) -> bool {
    normalize_type(env, a) == normalize_type(env, b)
}

/// Picks a common type for two conditional branches when they are equal, else `None`.
///
/// Returns `Some(a)` when [`same_type`] holds (either id may be returned since they
/// normalize to the same interned node). Returns `None` when the arms disagree, signaling
/// that the enclosing expression cannot be assigned a single type.
#[must_use]
pub fn unify_branch(env: &AliasEnv<'_>, a: TypeId, b: TypeId) -> Option<TypeId> {
    if same_type(env, a, b) { Some(a) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typeck::builtins::{bool_type, int_literal_type};
    use crate::typeck::types::TypeInterner;

    #[test]
    fn unify_equal_primitives() {
        let mut types = TypeInterner::new();
        let a = int_literal_type(&mut types, false);
        let b = int_literal_type(&mut types, false);
        let value_types = HashMap::new();
        let env = AliasEnv {
            types: &types,
            defs: &[],
            value_types: &value_types,
        };
        assert_eq!(unify_branch(&env, a, b), Some(a));
    }

    #[test]
    fn unify_mismatch_returns_none() {
        let mut types = TypeInterner::new();
        let a = int_literal_type(&mut types, false);
        let b = bool_type(&mut types);
        let value_types = HashMap::new();
        let env = AliasEnv {
            types: &types,
            defs: &[],
            value_types: &value_types,
        };
        assert_eq!(unify_branch(&env, a, b), None);
    }
}
