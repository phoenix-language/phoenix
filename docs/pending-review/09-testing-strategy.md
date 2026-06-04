# Review: Testing Strategy (`tests/cli/`, `tests/integration/`, crate tests)

## Summary

Testing is **strong on pipeline smoke and selected semantics**: CLI `check.sh`/`run.sh` cover many fixtures, integration `run_semantics.rs` asserts values via `run_captured`, and compiler crates have focused unit tests (lexer, resolve, typeck, codegen, verify). **Gaps**: many CLI fixtures only assert exit 0, negative diagnostics are thin, verifier/VM malformed-bytecode fuzzing is absent, multi-error diagnostic quality is untested, and pointer/slice/alloc paths lack semantic assertions.

## Findings

1. **`run_semantics` relies on scanning `main_locals`**
   Helpers use `any(|v| scalar_* matches)` (`run_semantics.rs:73–118`)—fragile if locals reorder or spill.
   **Severity:** `[medium]`
   **Recommendation:** Return `main`’s return value in `VmRunCapture`; assert exact slot or stack top after `run`.

2. **CLI `run.sh` vs integration semantics split**
   Fixtures like `deref_ptr.phx`, `ref_local.phx`, `byte_string.phx`, `primitives_*`, `compare_unary.phx` run in CLI but not in `run_semantics.rs`.
   **Severity:** `[high]`
   **Recommendation:** Port high-risk fixtures to integration value assertions within one sprint.

3. **Negative tests: few expected diagnostic snapshots**
   `bad_type.phx`, `use_after_move.phx`, `missing_main.phx` exist; no stable `.stderr` golden files for multi-error or import cycles.
   **Severity:** `[high]`
   **Recommendation:** Add `tests/integration/diagnostics/` with `// expect-error` comments or sidecar `.txt` compared to `format_with_modules` output.

4. **Verifier rejection of malicious bytecode**
   `phx-bytecode/src/verify.rs` has unit tests; no fuzz/property tests generating random operand streams.
   **Severity:** `[medium]`
   **Recommendation:** Add `cargo test` harness that mutates valid module bytes and asserts verify fails without VM panic.

5. **VM: no tests that invalid bytecode never panics**
   VM assumes verified input; defense-in-depth untested ([`07-vm.md`](../finished-review/07-vm.md) — addressed with `phx-vm` unit tests).
   **Severity:** `[medium]`
   **Recommendation:** Handcraft 5 invalid modules (bad stack, bad local) and assert `VmError` not Rust panic.

6. **Typeck edge cases partially covered**
   `phx-compiler/tests/typeck.rs` has use-after-move, non-exhaustive match; missing shadowing, alias cycle, `break` outside loop.
   **Severity:** `[medium]`
   **Recommendation:** Add one test per finding in [`04-type-system.md`](04-type-system.md) prioritized list.

7. **Parse recovery untested**
   No multi-error parse tests ([`01-syntax-and-ast.md`](01-syntax-and-ast.md)).
   **Severity:** `[low]` until recovery exists
   **Recommendation:** After recovery lands, add `tests/phx-syntax/recovery.phx` with two errors.

8. **Property-based layer absent**
   No quickcheck-style generators (std-only policy allows hand-rolled).
   **Severity:** `[future]`
   **Recommendation:** Start with lexer round-trip: random ASCII → lex → stringify tokens → compare.

9. **Diagnostic quality automation**
   Only `format.rs` unit tests for caret line; no lint on message text.
   **Severity:** `[low]`
   **Recommendation:** Snapshot `format_typecheck_error` for 10 canonical errors; fail CI on unintended message drift.

10. **CI scope**
    `tests/cli/README.md`: CI runs `check.sh` + `run.sh` only; `build.sh` local.
    **Severity:** `[medium]`
    **Recommendation:** Add `build.sh` to CI for `mvp_acceptance` and `app_dep` projects.

## What's working well

- **Lexer table tests** (`phx-syntax/tests/lexer.rs`)—exemplary breadth.
- **Codegen round-trip + verify** (`phx-compiler/tests/codegen.rs`).
- **Integration modules/build** (`run_modules.rs`, `run_dep_build.rs`, `run_build.rs`).
- **`mvp_acceptance` project** exercises multi-file build in semantics test.
- **Clippy `unwrap` deny** on production code; test crates explicitly allow.

## Recommended next actions

1. Extend `run_semantics.rs` with pointer/slice/primitive-width cases.
2. Add diagnostic golden tests for 5 error kinds.
3. Add verifier mutation tests.
4. Enable `build.sh` in CI.
5. Implement `VmRunCapture.return_value` and migrate semantics tests.

**Cross-references:** `run_captured` design—[`07-vm.md`](../finished-review/07-vm.md). `compile_source` limitation—[`08-modules-and-build.md`](08-modules-and-build.md).
