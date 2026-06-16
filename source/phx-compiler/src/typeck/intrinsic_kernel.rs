//! VM intrinsic lowering sites (`alloc_bytes`, `slice_from_raw_parts`, …).

/// Lowering hint for a call to a compiler intrinsic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicSite {
    /// `alloc_bytes(size)` → `ALLOC` opcode.
    AllocBytes,
    /// `dealloc_bytes(ptr, size)` → `FREE` opcode.
    DeallocBytes,
    /// `slice_from_raw_parts(ptr, len)` → `MAKE_SLICE_FROM_PTR` opcode.
    SliceFromRawParts,
    /// `len(slice)` → `SLICE_LEN` opcode.
    SliceLen,
    /// `size_of::<T>()` → compile-time `u32` constant.
    SizeOf,
}
