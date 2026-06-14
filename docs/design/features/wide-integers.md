# Wide integers (deferred from v1)

## Decision

**`s256`, `s512`, `u256`, `u512`, `f128`, `f256`, and `f512` are not v1 language primitives.**

v1 numeric primitives are:

- Signed: `s8`, `s16`, `s32`, `s64`, `s128`
- Unsigned: `u8`, `u16`, `u32`, `u64`, `u128`
- Float: `f32`, `f64`

Literals default to `s32`, `u32`, and `f32` as documented in [grammer.md](../grammer.md).

## Rationale

Backend and actor services rarely need 512-bit integers in the language core. Including them in v1 would expand literal parsing, operator tables, and std before the VM, ownership, and actor model are stable.

## Future path

When needed (crypto, hashing, fixed-width SIMD):

- Add **`std::wide`** (or `core::wide`) implemented in Phoenix on top of v1 primitives and `#unsafe` intrinsics if required
- Or introduce new primitives in a **breaking language edition** with explicit migration notes

Until then, use `u128` / `s128` in application code or FFI to native libraries for wide arithmetic.

## v1 execution contract (MVP VM)

Portable `PHX0` bytecode and the MVP interpreter execute **width-faithful** primitives through `s128` / `u128`:

- **Storage:** each primitive occupies its declared width on the stack and in locals (`u128` is a 16-byte cell, not a widened `i64` lane).
- **Signed integers (`s8`…`s128`):** arithmetic uses two's-complement **wrapping** (`wrapping_add`, `wrapping_sub`, `wrapping_mul`); `/` and `%` use truncating division toward zero; division by zero is a VM trap (`DivisionByZero`).
- **Unsigned integers (`u8`…`u128`):** arithmetic and comparison use native **unsigned** semantics at the operand width (zero-extended to `u128` internally for `u128` ops). `/` and `%` are truncating; division by zero traps.
- **Floats (`f32`, `f64`):** `+`, `-`, `*`, `/` follow IEEE 754. Float `%` is the **IEEE truncated remainder** (same sign as the dividend; Rust/C `fmod` behavior). Division by zero traps.
- **Integer power (`**`):** **not in v0** — the parser accepts the syntax for forward compatibility, but the MVP type checker rejects it and the VM returns `UnsupportedArithOp` if the opcode appears in hostile bytecode.

Shift masking and NaN comparison rules are specified separately in [type-system.md](type-system.md) (PHX-052).
