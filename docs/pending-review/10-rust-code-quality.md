# Review: Rust Code Quality (all crates)

## Summary

The workspace enforces **high bar defaults**: Edition 2024, `missing_docs` deny, `unwrap`/`expect` deny in production code, std-only dependencies, and clear crate boundaries (`phx-syntax`, `phx-diagnostics`, `phx-compiler`, `phx-bytecode`, `phx-vm`, `phx` CLI). **Needs attention**: a few production `expect` calls remain in VM hot paths, `phx-syntax` has `unreachable!`, intern/clone patterns will not scale, and public APIs sometimes expose compiler internals (`DefId`, full `TypedProgram`) without stability docs.

## Findings

1. **`unwrap`/`expect` policy mostly held**
   Production `expect` in `phx-vm/src/interpreter.rs:184,188` after manual checks; `phx-syntax/src/parser/expr.rs:92` `unreachable!()` violates spirit of deny lints for user-input paths.
   **Severity:** `[medium]`
   **Recommendation:** Add `phx-syntax` to workspace `unwrap_used` deny or replace `unreachable!` with `ParseError`.

2. **Std-only constraint holds**
   Root `Cargo.toml` workspace members use no external deps in compiler crates.
   **Severity:** `[low]` (positive)
   **Recommendation:** Keep policy in CI script grep for `path =` deps outside workspace.

3. **`missing_docs` compliance**
   Public items generally documented; some modules use brief one-liners—acceptable.
   **Severity:** `[low]`
   **Recommendation:** Run `cargo doc --no-deps` in CI; fix warnings.

4. **Clone usage moderate, hotspots in loader/resolve**
   `resolve_crate.rs`, `loader.rs`, `check.rs` clone strings/AST slices for multi-module maps (~10–13 sites in `check.rs`).
   **Severity:** `[medium]`
   **Recommendation:** Store `Arc<str>` per module source in `SourceModule`; pass `&str` into passes.

5. **No inappropriate `Box` in AST**
   Arena policy from rules; AST uses owned `Vec` in nodes—acceptable for MVP.
   **Severity:** `[low]`
   **Recommendation:** Introduce arena when AST node count > ~10k per file (profile first).

6. **Pass boundaries mostly respected**
   `phx-syntax` does not depend on compiler; `phx-diagnostics` is shared error surface.
   **Severity:** `[low]` (positive)
   **Recommendation:** Do not add `phx-compiler` → `phx-syntax` reverse dependency when adding LSP.

7. **Public API leakage**
   `phx_compiler::compile_source`, `TypedProgram`, `ResolvedProgram` expose full internal graphs to CLI/tests.
   **Severity:** `[medium]`
   **Recommendation:** Add stable `CompilationOutput { diagnostics, bytecode }` facade for external tools; mark internal types `#[doc(hidden)]`.

8. **Match exhaustiveness on compiler enums**
   Generally no `_` on `TokenKind`/AST in production paths; `lower/expr.rs` has `_ => {}` wildcards for unhandled exprs.
   **Severity:** `[medium]`
   **Recommendation:** Replace with explicit `Expr` variants or `todo!("lower …")` behind compile_error for missing cases.

9. **Compilation time**
   No heavy macros or generic monoliths yet; split crates help incremental builds.
   **Severity:** `[low]`
   **Recommendation:** When adding generics codegen, isolate monomorphization in its own module/crate to contain compile time.

10. **Test-only `unwrap` in `project/config.rs` tests**
    `create_dir_all`...`unwrap()` in `#[cfg(test)]`—fine.
    **Severity:** `[low]` (positive)

## What's working well

- **Workspace lints** (`Cargo.toml:13–25`): pedantic + deny panics/prints.
- **Crate split** matches pipeline stages (lex/parse vs typeck vs bytecode vs vm).
- **Newtypes** (`DefId`, `Symbol`, `LocalSlot`, `Span`) reduce mix-ups.
- **`#[non_exhaustive]`** on `DefKind` and language-facing enums.
- **Integration tests** as separate crate—keeps lib test time down.

## Recommended next actions

1. Remove `unreachable!` from `phx-syntax` parser.
2. Introduce `Arc<str>` module sources in multi-file driver.
3. Add facade type for CLI/SDK consumers.
4. Audit `lower/expr.rs` and `check.rs` `_` arms—replace with exhaustive matches.
5. Add CI `cargo doc` + forbidden dependency check.

**Cross-references:** Diagnostic bags—[`02-diagnostics.md`](02-diagnostics.md). VM `expect`—[`07-vm.md`](07-vm.md).
