//! Compile-time byte size of types for `size_of` intrinsic.
//!
//! [`type_byte_size`] evaluates aggregate and primitive sizes from [`ProgramLayout`] during
//! checking so `size_of` calls fold to constants in lowering.

use super::layout::ProgramLayout;
use super::primitive::{keyword_to_primitive_kind, primitive_byte_size};
use super::types::{Ty, TypeId, TypeInterner};
use crate::resolver::{DefId, ResolvedProgram};

/// Returns the in-memory byte size of `ty` for std `size_of` (MVP: primitives and aggregates).
///
/// Wide integers (`s128`/`u128`) use 8 bytes in the MVP VM.
#[must_use]
pub fn type_byte_size(
    types: &TypeInterner,
    layout: &ProgramLayout,
    resolved: &ResolvedProgram,
    ty: TypeId,
) -> Option<u32> {
    match types.get(ty) {
        Ty::Primitive(kw) => {
            keyword_to_primitive_kind(*kw).map(|k| u32::from(primitive_byte_size(k)))
        }
        Ty::Unit => Some(0),
        Ty::Ptr { .. } | Ty::Ref { .. } => Some(8),
        Ty::Fn { .. } => Some(8),
        Ty::Array { elem, len } => {
            type_byte_size(types, layout, resolved, *elem)?.checked_mul(*len)
        }
        Ty::Tuple(elems) => elems.iter().try_fold(0u32, |acc, e| {
            Some(acc + type_byte_size(types, layout, resolved, *e)?)
        }),
        Ty::Named { def, args } => struct_byte_size(types, layout, resolved, *def, args),
        Ty::Slice(_) | Ty::Str | Ty::Error | Ty::Var(_) => None,
    }
}

fn struct_byte_size(
    types: &TypeInterner,
    layout: &ProgramLayout,
    resolved: &ResolvedProgram,
    def: DefId,
    args: &[TypeId],
) -> Option<u32> {
    let sl = layout.struct_layout(def, args)?;
    let mut total = 0u32;
    for (_, fty) in &sl.fields {
        total = total.checked_add(type_byte_size(types, layout, resolved, *fty)?)?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typeck::builtins::int_literal_type;
    use crate::typeck::types::TypeInterner;
    use phx_syntax::token::Keyword;

    #[test]
    #[allow(clippy::expect_used)]
    fn primitive_s32_is_four_bytes() {
        let mut types = TypeInterner::new();
        let s32 = types.intern(&Ty::Primitive(Keyword::S32));
        let layout = ProgramLayout::default();
        let file = phx_syntax::parse("main :: () => {};");
        assert!(!file.has_errors());
        let file = file.value;
        let resolved = crate::resolver::resolve(&file).expect("resolve");
        assert_eq!(type_byte_size(&types, &layout, &resolved, s32), Some(4));
        let _ = int_literal_type(&mut types, true);
    }
}
