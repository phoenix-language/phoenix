//! Type equality and branch unification.
//!
//! [`same_type`] compares types under alias expansion ([`normalize_type`]). Used for `if`/`match`
//! branch joins, pattern compatibility, and generic constraint checks.

use std::collections::{HashMap, HashSet};

use crate::resolver::{Def, DefId, DefKind};

use super::types::{Ty, TypeId, TypeInterner};

/// Context for resolving type aliases to their underlying types.
pub struct AliasEnv<'a> {
    /// Interned type pool.
    pub types: &'a TypeInterner,
    /// All definitions in the compilation unit.
    pub defs: &'a [Def],
    /// Type of each definition (`TypeAlias` → underlying type).
    pub value_types: &'a HashMap<DefId, TypeId>,
}

/// Expands `TypeAlias` definitions to their underlying type (with cycle protection).
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

/// Returns `true` when `a` and `b` are the same type after alias normalization.
#[must_use]
pub fn same_type(env: &AliasEnv<'_>, a: TypeId, b: TypeId) -> bool {
    normalize_type(env, a) == normalize_type(env, b)
}

/// Picks a common type for two branches when equal, else `None`.
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
