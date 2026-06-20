//! Generic type nesting depth for monomorphization guardrails.
//!
//! Rejects excessively nested or cyclic generic instantiations before monomorphization expands
//! templates. The checker uses these limits to fail fast with
//! [`TypeCheckError::GenericNestingTooDeep`](phx_diagnostics::TypeCheckError::GenericNestingTooDeep)
//! instead of blowing up compile time or memory during specialization.
//!
//! # Depth model
//!
//! [`type_nesting_depth`] counts nesting layers in the type graph:
//!
//! - Primitives, unit, `str`, inference variables, and error types contribute `0`.
//! - A [`Ty::Named`] with type arguments contributes `1 + max(child depths)`.
//! - Tuples, arrays, slices, references, pointers, and function types use the maximum depth
//!   among their component types (function types include both parameters and return).
//!
//! [`check_named_instantiation_depth`] adds one layer for the named head being instantiated,
//! on top of the maximum depth among explicit type arguments.
//!
//! # Cycles
//!
//! A revisiting [`TypeId`] during traversal is treated as a cycle and reported as exceeding
//! the limit (same error path as depth overflow).

use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::DefId;

/// Maximum allowed nesting depth for monomorphized generic types.
pub const MAX_GENERIC_TYPE_NESTING: usize = 64;

/// Returns the generic nesting depth of `id`.
///
/// # Errors
///
/// Returns `Err(())` when the type graph contains a cycle reachable from `id`.
pub fn type_nesting_depth(types: &TypeInterner, id: TypeId) -> Result<usize, ()> {
    depth_inner(types, id, &mut Vec::new())
}

/// Returns `Ok(())` when `id` is within [`MAX_GENERIC_TYPE_NESTING`].
///
/// # Errors
///
/// Returns `Err(depth)` when `depth` exceeds the limit or a cycle is detected (reported as
/// `MAX_GENERIC_TYPE_NESTING + 1` from underlying cycle detection).
pub fn check_type_nesting_depth(types: &TypeInterner, id: TypeId) -> Result<(), usize> {
    let depth = type_nesting_depth(types, id).map_err(|()| MAX_GENERIC_TYPE_NESTING + 1)?;
    if depth > MAX_GENERIC_TYPE_NESTING {
        Err(depth)
    } else {
        Ok(())
    }
}

/// Returns the maximum nesting depth among `args`.
///
/// # Errors
///
/// Returns `Err(depth)` when any argument exceeds [`MAX_GENERIC_TYPE_NESTING`] or a cycle
/// is detected.
pub fn max_depth_of_args(types: &TypeInterner, args: &[TypeId]) -> Result<usize, usize> {
    let mut max = 0usize;
    for arg in args {
        let depth = type_nesting_depth(types, *arg).map_err(|()| MAX_GENERIC_TYPE_NESTING + 1)?;
        if depth > MAX_GENERIC_TYPE_NESTING {
            return Err(depth);
        }
        max = max.max(depth);
    }
    Ok(max)
}

/// Checks nesting depth of a named type `def` instantiated at `args`.
///
/// Computes `1 + max_depth_of_args(args)` and compares against [`MAX_GENERIC_TYPE_NESTING`].
///
/// # Errors
///
/// Returns `Err(depth)` when the instantiated type would exceed the limit or a cycle is
/// detected among the arguments.
pub fn check_named_instantiation_depth(
    types: &TypeInterner,
    _def: DefId,
    args: &[TypeId],
) -> Result<(), usize> {
    let child = max_depth_of_args(types, args)?;
    let depth = 1 + child;
    if depth > MAX_GENERIC_TYPE_NESTING {
        Err(depth)
    } else {
        Ok(())
    }
}

fn depth_inner(types: &TypeInterner, id: TypeId, visiting: &mut Vec<TypeId>) -> Result<usize, ()> {
    if visiting.contains(&id) {
        return Err(());
    }
    visiting.push(id);
    let depth = match types.get(id).clone() {
        Ty::Named { args, .. } => {
            let child = args
                .iter()
                .map(|arg| depth_inner(types, *arg, visiting))
                .try_fold(0usize, |acc, d| d.map(|v| acc.max(v)))?;
            1 + child
        }
        Ty::Tuple(elems) => max_child_depth(types, &elems, visiting)?,
        Ty::Array { elem, .. } => depth_inner(types, elem, visiting)?,
        Ty::Slice(inner) => depth_inner(types, inner, visiting)?,
        Ty::Ref { inner, .. } | Ty::Ptr { inner, .. } => depth_inner(types, inner, visiting)?,
        Ty::Fn { params, ret } => {
            let param_max = max_child_depth(types, &params, visiting)?;
            let ret_depth = depth_inner(types, ret, visiting)?;
            param_max.max(ret_depth)
        }
        Ty::Primitive(_) | Ty::Unit | Ty::Str | Ty::Var(_) | Ty::Error => 0,
    };
    visiting.pop();
    Ok(depth)
}

fn max_child_depth(
    types: &TypeInterner,
    children: &[TypeId],
    visiting: &mut Vec<TypeId>,
) -> Result<usize, ()> {
    children
        .iter()
        .map(|child| depth_inner(types, *child, visiting))
        .try_fold(0usize, |acc, d| d.map(|v| acc.max(v)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::resolver::DefId;

    fn nest_def(id: u32) -> DefId {
        DefId::from_raw(id)
    }

    fn s32_type(types: &mut TypeInterner) -> TypeId {
        types.intern(&Ty::Primitive(phx_syntax::token::Keyword::S32))
    }

    fn nest_type(types: &mut TypeInterner, def: DefId, inner: TypeId) -> TypeId {
        types.intern(&Ty::Named {
            def,
            args: vec![inner],
        })
    }

    #[test]
    fn primitive_depth_is_zero() {
        let mut types = TypeInterner::new();
        let s32 = s32_type(&mut types);
        assert_eq!(type_nesting_depth(&types, s32).unwrap(), 0);
    }

    #[test]
    fn single_named_layer_depth_one() {
        let mut types = TypeInterner::new();
        let s32 = s32_type(&mut types);
        let nested = nest_type(&mut types, nest_def(1), s32);
        assert_eq!(type_nesting_depth(&types, nested).unwrap(), 1);
    }

    #[test]
    fn nested_sixty_four_is_ok() {
        let mut types = TypeInterner::new();
        let mut ty = s32_type(&mut types);
        for i in 0..64 {
            ty = nest_type(&mut types, nest_def(i + 1), ty);
        }
        assert_eq!(type_nesting_depth(&types, ty).unwrap(), 64);
        assert!(check_type_nesting_depth(&types, ty).is_ok());
    }

    #[test]
    fn nested_sixty_five_errors() {
        let mut types = TypeInterner::new();
        let mut ty = s32_type(&mut types);
        for i in 0..65 {
            ty = nest_type(&mut types, nest_def(i + 1), ty);
        }
        assert_eq!(type_nesting_depth(&types, ty).unwrap(), 65);
        assert_eq!(check_type_nesting_depth(&types, ty), Err(65));
    }

    #[test]
    fn named_instantiation_check_matches_full_type() {
        let mut types = TypeInterner::new();
        let mut inner = s32_type(&mut types);
        let def = nest_def(1);
        for _ in 0..64 {
            inner = nest_type(&mut types, def, inner);
        }
        assert_eq!(
            check_named_instantiation_depth(&types, def, &[inner]),
            Err(65)
        );
    }
}
