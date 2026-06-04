# Review: Codegen and Bytecode (`phx-compiler/codegen/`, `phx-bytecode`)

## Summary

PHX0 encoding, 43 opcodes, stack-flow verification, and codegen from `IrInst` form a **coherent MVP bytecode pipeline**. Review 06 closed **correctness gaps**: const pool deduplication, verifier checks for const payload width vs `prim_kind` and `local_layouts`, `Call` operand shape and callee arity in stack simulation, encode/codegen/link overflow errors, post-link `verify` in the build driver, stable **global function ids** for path dependencies (`ENTRY_NONE` for libraries), and **`.pxi` v2** structured exports with import type hydration. **Deferred:** compiler lowering for `Alloc`/`PtrStore`, `Pop` emission, and PHX0 symbols section (kind 5).

## Findings

1. **Const pool: no deduplication** — **Addressed**
   [`ConstPoolBuilder`](../../source/phx-compiler/src/codegen/const_pool.rs) dedupes by `(tag, payload)`; IR literal index → pool index map preserved for emit. Test: `const_pool_dedupes_identical_literals` in [`codegen.rs`](../../source/phx-compiler/tests/codegen.rs).

2. **Verifier: const width vs `prim_kind` not checked** — **Addressed**
   `verify_const_operands` validates `PrimitiveKind`, payload length, and `ConstTag` vs signed/unsigned for integers (`verify.rs`). Test: `reject_const_payload_width_mismatch`.

3. **`local_layouts` section encoded, never verified** — **Addressed**
   Per-function layout required when section non-empty; slot count vs `local_count`; `LoadLocal`/`StoreLocal`/`AddressOfLocal` slot and `prim_kind` vs `LocalSlotKind` (aggregate `0xFF`).

4. **`Call` arity not validated against callee** — **Addressed**
   `Call` requires exactly one operand (function id); stack flow errors on unknown callee instead of arity 0 (`stack_flow.rs`). Callee arity applied via `stack_effect` + `build_fn_arity_map` including `.pxi` imports.

5. **`Alloc` / `PtrStore` opcodes unreachable from compiler** — **Documented (deferred)**
   Opcodes remain stable; [`vm-linear.md`](../design/features/vm-linear.md) and [`opcode.rs`](../../source/phx-bytecode/src/opcode.rs) rustdoc note VM-ready, compiler emission after intrinsic kernel spelling per [`grammar-deferred.md`](../design/features/grammar-deferred.md).

6. **`Pop` never emitted** — **Documented (MVP)**
   Phoenix codegen discards via `StoreLocal`/control flow; opcode retained for discriminant stability (`opcode.rs`, `vm-linear.md`).

7. **PHX0 versioning** — **Documented (positive)**
   `VERSION_MAJOR=0`, `VERSION_MINOR=1`; decode accepts `minor <= VERSION_MINOR`. Bump minor when adding mandatory verifier sections.

8. **Symbols section (kind 5) not written** — **Documented (reserved)**
   Section kind `5` reserved in `vm-linear.md` for future debug names; encoder still emits five MVP sections.

9. **Link step patches indices; no mandatory re-verify in driver** — **Addressed**
   `phx_bytecode::verify(&linked)` after `link_modules` in [`build/driver.rs`](../../source/phx-compiler/src/build/driver.rs); `BuildError::Verify`.

10. **Size overflow: `u32::try_from(...).unwrap_or(0)`** — **Addressed (bytecode boundaries)**
    [`EncodeError`](../../source/phx-bytecode/src/encode.rs), [`CodegenError`](../../source/phx-compiler/src/codegen/error.rs), `LinkError::SectionTooLarge`; `BytecodeModule::encode() -> Result`.

11. **`.pxi` exports: name + kind + signature string only** — **Addressed (v2)**
    Design [`pxi-format.md`](../design/features/pxi-format.md); emit/parse [`PxiType`](../../source/phx-compiler/src/pxi/type_ast.rs); [`import_types.rs`](../../source/phx-compiler/src/pxi/import_types.rs) seeds typeck from v2; v1 readers keep signature-only fallback. Tests: [`pxi.rs`](../../source/phx-compiler/tests/pxi.rs).

**Also shipped:** `LoadLocal` uses binding type for `prim_kind` (fixes `&T` locals vs layout); `build_global_fn_map` assigns imported fn ids before body fns for link; libraries use [`ENTRY_NONE`](../../source/phx-bytecode/src/header.rs) (`0xFFFF_FFFF`) so `entry_function_id` does not point at exported functions.

## What's working well

- **Opcode stability test** (`phx-bytecode/src/lib.rs`).
- **Stack-flow CFG analysis** catches depth underflow/overflow (`stack_flow.rs`, `verify.rs`).
- **Variable-length instructions** with operand indices only (`instr.rs`).
- **Codegen maps most `IrInst` variants** including short-circuit via `JumpIf` (`emit.rs`).
- **Verifier tests** for bad jumps, locals, stack, const width, invalid call targets (`verify.rs`).

## Recommended next actions

1. Lower `Alloc` / `PtrStore` when alloc/intrinsic kernel syntax is specified.
2. Wire `bindings_from_pxi` to skip dependency body parse when only types are needed ([`08-modules-and-build.md`](../review/08-modules-and-build.md)).
3. Remap dependency function ids at link if global map and per-crate objects diverge (follow-up if duplicate-id link errors appear in larger graphs).

**Cross-references:** VM executes `Alloc`/pointers—[`07-vm.md`](07-vm.md). Incremental build—[`08-modules-and-build.md`](../pending-review/08-modules-and-build.md). IR lowering—[`05-ir-and-lowering.md`](05-ir-and-lowering.md).
