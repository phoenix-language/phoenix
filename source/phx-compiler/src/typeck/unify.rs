//! Type equality and branch unification.

use super::types::{TypeId, TypeInterner};

/// Returns `true` when `a` and `b` are the same type.
#[must_use]
pub fn same_type(_types: &TypeInterner, a: TypeId, b: TypeId) -> bool {
    a == b
}

/// Picks a common type for two branches when equal, else `None`.
#[must_use]
pub fn unify_branch(types: &TypeInterner, a: TypeId, b: TypeId) -> Option<TypeId> {
    if same_type(types, a, b) {
        Some(a)
    } else {
        None
    }
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
        assert_eq!(unify_branch(&types, a, b), Some(a));
    }

    #[test]
    fn unify_mismatch_returns_none() {
        let mut types = TypeInterner::new();
        let a = int_literal_type(&mut types, false);
        let b = bool_type(&mut types);
        assert_eq!(unify_branch(&types, a, b), None);
    }
}
