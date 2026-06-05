# Review: Testing Strategy (`tests/cli/`, `tests/integration/`, crate tests)

## Summary

This review closed the five recommended testing gaps: **slot-index semantics assertions** via [`VmRunCapture`](../../source/phx-vm/src/interpreter.rs), **extended `run_semantics` coverage** for primitive-width/pointer/compare fixtures, **diagnostic golden tests** with `.stderr` sidecars, a **verifier byte-mutation harness**, and **Justfile/CI alignment** (`just test-lang` includes `build.sh`; `just test-cli` matches the CI `cli` job). **Deferred:** property-based lexer round-trip, parse-recovery multi-error corpus, `tests/phoenix/` sidecar E2E format, and a full `format_typecheck_error` snapshot suite beyond the five golden diagnostics.

## Findings

1. **`run_semantics` fragile `main_locals` scanning** — **Addressed**
   [`VmRunCapture::main_local`](../../source/phx-vm/src/interpreter.rs) and slot-index helpers in [`run_semantics.rs`](../../tests/integration/tests/run_semantics.rs). `return_value` captures the stack top on top-level `Return`; callee returns still pass values to callers.

2. **CLI `run.sh` vs integration semantics split** — **Addressed**
   High-risk fixtures ported to [`run_semantics.rs`](../../tests/integration/tests/run_semantics.rs): `primitives_*`, `compare_unary`, `mod_bitwise`, pointer/ref/slice/byte-string paths, and struct/enum/match variants (28 value tests).

3. **Negative tests lack golden snapshots** — **Addressed**
   [`tests/integration/tests/diagnostics.rs`](../../tests/integration/tests/diagnostics.rs) with [`tests/integration/diagnostics/*.stderr`](../../tests/integration/diagnostics/) for type mismatch, use-after-move, missing `main`, import cycle, and multi-resolve duplicates. Regenerate with `UPDATE_GOLDEN=1`.

4. **Verifier mutation/fuzz absent** — **Addressed**
   [`source/phx-bytecode/tests/verify_mutation.rs`](../../source/phx-bytecode/tests/verify_mutation.rs) mutates valid modules (unknown opcode, invalid call, stack underflow, truncated metadata) and asserts `verify` rejects; runtime paths assert `phx_vm::run` returns `Err` or does not panic where the MVP VM does not re-check verifier-only invariants.

5. **VM invalid-bytecode panic defense** — **Addressed**
   Hand-crafted modules in [`phx-vm/src/lib.rs`](../../source/phx-vm/src/lib.rs); mutation tests link verify rejection to VM `Err` on overlapping cases.

6. **Typeck edge cases partially covered** — **Addressed** (pre-existing)
   [`source/phx-compiler/tests/typeck.rs`](../../source/phx-compiler/tests/typeck.rs): shadowing, recursive alias cycle, `break` outside loop, parse-recovery formatting, return-escapes-local.

7. **Parse recovery untested** — **Documented (deferred)**
   Multi-error parse corpus waits until recovery semantics are stable per [`01-syntax-and-ast.md`](01-syntax-and-ast.md).

8. **Property-based layer absent** — **Documented (future)**
   Std-only hand-rolled lexer round-trip generator remains a follow-up.

9. **Diagnostic quality automation** — **Partially addressed**
   Five golden `.stderr` files cover formatted multi-module output; full `format_typecheck_error` snapshot lint (10 canonical errors) deferred.

10. **CI scope / `build.sh`** — **Addressed**
    CI `cli` job already ran all five scripts; [`Justfile`](../../Justfile) `test-lang` now includes `build.sh`; `just test-cli` alias added. [`tests/cli/README.md`](../../tests/cli/README.md) updated.

## What's working well

- **Lexer table tests** ([`phx-syntax/tests/lexer.rs`](../../source/phx-syntax/tests/lexer.rs)) — exemplary breadth.
- **Codegen round-trip + verify** ([`phx-compiler/tests/codegen.rs`](../../source/phx-compiler/tests/codegen.rs)).
- **Integration modules/build** (`run_modules.rs`, `run_dep_build.rs`, `run_build.rs`, `incremental_build.rs`).
- **Slot-index semantics** — deterministic integration value checks without `any()` local scans.
- **Diagnostic goldens** — stable formatted output with path normalization.
- **Clippy `unwrap` deny** on production code; test crates explicitly allow.

## Recommended next actions

1. Property-based lexer round-trip (hand-rolled generator, std-only).
2. Parse-recovery multi-error corpus when recovery lands (`tests/phx-syntax/recovery.phx`).
3. `format_typecheck_error` snapshot suite (10 canonical errors) beyond the five golden CLI diagnostics.
4. Shared test utilities crate if helper duplication across integration files grows.
5. Optional: consolidate overlapping smoke tests (`run_control_flow.rs` vs `run_semantics.rs`).

**Cross-references:** VM capture API — [`07-vm.md`](07-vm.md). Diagnostics — [`02-diagnostics.md`](02-diagnostics.md). Modules/build — [`08-modules-and-build.md`](08-modules-and-build.md). Checklist — [`mvp-implementation-checklist.md`](../mvp-implementation-checklist.md).
