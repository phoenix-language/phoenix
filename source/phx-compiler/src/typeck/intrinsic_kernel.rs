//! VM intrinsic lowering sites (`alloc_bytes`, `slice_from_raw_parts`, …).
//!
//! [`IntrinsicSite`] tags recognized std/compiler intrinsics at call sites so lowering emits
//! dedicated bytecode opcodes instead of ordinary indirect function calls.
//!
//! ## Pipeline placement
//!
//! 1. **Type check** — [`super::check::intrinsic`] matches callee names/signatures and records
//!    [`IntrinsicSite`] on the call expression.
//! 2. **Lower** — [`crate::lower::expr::call`] maps each variant to a single opcode (see table below).
//!
//! ## Opcode mapping
//!
//! | [`IntrinsicSite`] | Bytecode opcode |
//! |-------------------|-----------------|
//! | [`IntrinsicSite::AllocBytes`] | `ALLOC` |
//! | [`IntrinsicSite::DeallocBytes`] | `FREE` |
//! | [`IntrinsicSite::SliceFromRawParts`] | `MAKE_SLICE_FROM_PTR` |
//! | [`IntrinsicSite::SliceLen`] | `SLICE_LEN` |
//! | [`IntrinsicSite::SizeOf`] | compile-time `u32` constant (no runtime call) |

/// Lowering hint for a call to a compiler-recognized intrinsic.
///
/// Recorded during type checking when the callee matches a known std/VM builtin; consumed by
/// [`crate::lower::expr::call`] to select a dedicated opcode instead of `Call`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicSite {
    /// `alloc_bytes(size: u32) -> *mut u8` lowers to [`phx_bytecode::Opcode::Alloc`].
    AllocBytes,
    /// `dealloc_bytes(ptr: *mut u8, size: u32)` lowers to [`phx_bytecode::Opcode::Free`].
    DeallocBytes,
    /// `slice_from_raw_parts(ptr, len)` lowers to [`phx_bytecode::Opcode::MakeSliceFromPtr`].
    SliceFromRawParts,
    /// `len(slice: &[T]) -> u32` lowers to [`phx_bytecode::Opcode::SliceLen`].
    SliceLen,
    /// `size_of::<T>()` folds to a compile-time `u32` constant (no runtime opcode).
    SizeOf,
}
