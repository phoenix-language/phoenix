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
