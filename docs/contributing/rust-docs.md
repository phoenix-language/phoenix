# Rust documentation standards (Phoenix workspace)

This document is the contributor reference for rustdoc in `source/`. Cursor rules in `.cursor/rules/rust-standards.mdc` summarize the same template; this file defines **tiers** and **crate order** for doc passes.

Language semantics belong in `docs/design/` — do not invent behavior in doc comments.

---

## Doc comment template

Every **public** item needs a doc comment (`missing_docs` is **deny** in the workspace `Cargo.toml`).

```rust
/// Short summary of what this does.
///
/// Optional detail: inputs, outputs, or invariants.
///
/// # Errors
///
/// Returns [`CompileError::Parse`] when the source fails to parse.
///
/// # Panics
///
/// Never panics on malformed user input.
pub fn compile_source(...) -> Result<..., CompileError>
```


| Section      | When                                                            |
| ------------ | --------------------------------------------------------------- |
| Summary line | Always                                                          |
| `# Errors`   | Any `Result` return (`missing_errors_doc` is **deny**)          |
| `# Panics`   | Any panic path, or state that user/malformed input never panics |
| `# Safety`   | Public `unsafe` functions                                       |
| Examples     | Non-obvious safe APIs                                           |


**Enum variants:** document behavior and stack/preconditions where relevant (see `source/phx-bytecode/src/opcode.rs`).

---

## Tiers

### Tier A — Public API

**Goal:** Docs are useful to crate consumers, not one-line stubs.

- All `pub` items in crate roots and re-exported modules.
- `# Errors` on every public `Result` API.
- Stability: mark intentional internals `#[doc(hidden)]` or note “not stable for external tools” on exposed compiler graphs (`DefId`, full `TypedProgram`, etc.).

**Crate order** (pipeline order):

1. `phx-diagnostics`
2. `phx-syntax`
3. `phx-bytecode`
4. `phx-compiler`
5. `phx-vm`
6. `phx` (CLI)

### Tier B — Internal passes

**Goal:** Onboard contributors per compiler stage without documenting every helper.


| Item                        | Required doc                                                              |
| --------------------------- | ------------------------------------------------------------------------- |
| Every `source/**/*.rs` file | `//!` module header: purpose, inputs/outputs, owning pass                 |
| `pub(crate)` functions      | `///` — contract, errors, invariants                                      |
| Private functions           | `///` only when non-obvious (ownership, stack layout, multi-module edges) |
| Wildcard `match` arms       | Brief comment *why* the arm exists                                        |


**Batch order** (one PR per batch is fine):

1. Syntax: `lexer.rs`, `parser/`, `ast/`
2. Resolver: `resolver/` (`walk.rs`, `scopes.rs`)
3. Typeck: `typeck/` (`check.rs`, `unify.rs`, `ownership.rs`)
4. Lower + IR: `lower/`, `ir/`
5. Codegen + verify: `codegen/`, `phx-bytecode/src/verify.rs`
6. Modules/build: `modules/`, `build/`, `project/`

Do **not** require docs on every private one-line helper.

### Tier C — Tooling

- `just doc-check` — `cargo doc --workspace --no-deps`
- `just pre-commit` — fmt, clippy, doc-check, dep-check, CLI lang tests (see `.cursor/rules/project-layout.mdc`)
- Full CI parity before merge: `cargo test --workspace`

---

## Verification

```bash
just pre-commit          # fmt-check, clippy, doc-check, test-lang
just doc-check           # rustdoc only (also run via pre-commit)
cargo test --workspace   # unit + integration (not in pre-commit)
```

---

## Agent / pre-commit gate

Before finishing work in this repo, agents run `**just pre-commit**` from the repo root. See `.cursor/rules/project-layout.mdc` (Completion gate).