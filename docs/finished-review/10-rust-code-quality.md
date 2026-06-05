# Review: Rust Code Quality (all crates)

## Summary

This review closed the five recommended next actions: **`phx-vm` workspace lint alignment** (no production `expect` on the `Call` path), **`Arc<str>` module sources** (`SourceText` in `LoadedModule` / `SourceModule`), a **stable compile facade** (`phx_compiler::facade`), **explicit lowering matches** in `lower/expr.rs` (with documented `#[non_exhaustive]` fallbacks), and a **CI dependency policy script** (`tests/ci/check-deps.sh`). **Stale at review time:** `phx-syntax` `unreachable!` was already removed; `cargo doc` already ran in CI and `just pre-commit`.

## Findings

1. **`unwrap`/`expect` policy mostly held** — **Addressed (VM)**
   `phx-vm` now inherits workspace `unwrap_used` / `expect_used` deny. `Opcode::Call` in [`interpreter.rs`](../../source/phx-vm/src/interpreter.rs) uses `VmError` returns instead of `expect`. Parser `unreachable!` claim was stale — [`parser/expr.rs`](../../source/phx-syntax/src/parser/expr.rs) uses proper range parsing.

2. **Std-only constraint holds** — **Addressed (automated)**
   [`tests/ci/check-deps.sh`](../../tests/ci/check-deps.sh) fails on crates.io `version =` deps in member `Cargo.toml` files; wired into [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) and `just dep-check` / `pre-commit`.

3. **`missing_docs` compliance** — **Addressed (pre-existing)**
   `cargo doc --workspace --no-deps` in CI and `just doc-check`.

4. **Clone usage moderate, hotspots in loader/resolve** — **Addressed (sources)**
   [`SourceText`](../../source/phx-compiler/src/modules/source_text.rs) (`Arc<str>`) in [`LoadedModule`](../../source/phx-compiler/src/modules/loader.rs) and [`SourceModule`](../../source/phx-compiler/src/resolver/mod.rs); `resolve_crate` and [`DiagnosticContext::from_loaded`](../../source/phx-compiler/src/compile.rs) clone `Arc` handles. Program/AST `clone()` in `resolve_crate` deferred.

5. **No inappropriate `Box` in AST** — **Documented (deferred)**
   Arena allocation remains future work when profiling warrants.

6. **Pass boundaries mostly respected** — **Unchanged (positive)**
   `phx-syntax` does not depend on the compiler; `phx-diagnostics` is the shared error surface.

7. **Public API leakage** — **Addressed**
   [`facade.rs`](../../source/phx-compiler/src/facade.rs): `CompileOutput`, `CheckOutput`, stable `check_file` / `compile_to_module` wrappers. [`TypedProgram`](../../source/phx-compiler/src/typeck/mod.rs), [`ResolvedProgram`](../../source/phx-compiler/src/resolver/mod.rs), [`CompilationUnit`](../../source/phx-compiler/src/unit.rs) re-exported with `#[doc(hidden)]`; crate-root stability docs in [`lib.rs`](../../source/phx-compiler/src/lib.rs).

8. **Match exhaustiveness on compiler enums** — **Partially addressed**
   [`lower/expr.rs`](../../source/phx-compiler/src/lower/expr.rs): explicit `UnaryOp` / `BinOp` / `Pattern` arms; single `#[allow(unreachable_patterns)]` fallback per `#[non_exhaustive]` enum. Full `check.rs` wildcard sweep deferred.

9. **Compilation time** — **Documented (deferred)**
   Isolate generic codegen monomorphization when generics land.

10. **Test-only `unwrap` in config tests** — **Unchanged (positive)**
    `#[cfg(test)]` `unwrap` remains acceptable.

## What's working well

- **Workspace lints** (`Cargo.toml`): pedantic + deny panics/prints on production crates.
- **Crate split** matches pipeline stages (lex/parse vs typeck vs bytecode vs vm).
- **Newtypes** (`DefId`, `Symbol`, `LocalSlot`, `Span`) reduce mix-ups.
- **`#[non_exhaustive]`** on language-facing enums.
- **Integration tests** as separate crate—keeps lib test time down.
- **Std-only policy** enforced in CI without manual review.

## Recommended next actions

1. Arena allocation when AST node count per file exceeds ~10k (profile first).
2. Exhaustive `check.rs` `Expr` / `Stmt` wildcard audit (large file; separate pass).
3. Generic codegen compile-time isolation when generics codegen ships.
4. LSP / SDK consumers should use `phx_compiler::facade` only.
5. Optional: property-test or fuzz the dependency checker against synthetic bad `Cargo.toml` snippets.

**Cross-references:** VM `Call` path — [`07-vm.md`](07-vm.md). Diagnostics — [`02-diagnostics.md`](02-diagnostics.md). Testing — [`09-testing-strategy.md`](09-testing-strategy.md).
