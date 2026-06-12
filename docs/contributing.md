# Contributing to Phoenix

Phoenix is an experimental systems language: parse → resolve → type-check → lower → `PHX0` bytecode → verifier → stack VM. This guide gets you building and testing without reading the compiler source.

## Quick start

```bash
git clone <repo-url>
cd phoenix
cargo build -p phx
just pre-commit
```

`just pre-commit` runs format check, Clippy, doc check, dependency check, and language integration tests (`just test-lang`). Run `just test` for the full workspace test suite before large merges.

## Common commands

| Command | Purpose |
|---------|---------|
| `just phx <args>` | Run the local `phx` CLI (e.g. `just phx check file.phx`) |
| `just pre-commit` | Pre-PR gate: fmt, lint, docs, deps, `test-lang` |
| `just test-lang` | CLI E2E, run smoke, and diagnostic golden tests |
| `just test` | `cargo test --workspace` |
| `just build-std` | Build the bundled `std` library package |

## Repository layout

```
phoenix/
├── source/
│   ├── phx-syntax/      # lexer, parser, AST
│   ├── phx-compiler/    # resolve, typeck, lower, codegen, projects
│   ├── phx-bytecode/    # PHX0 format and verifier
│   ├── phx-vm/          # stack interpreter
│   ├── phx-diagnostics/ # error codes and cargo-style rendering
│   ├── phx-cli/         # CLI library
│   └── phx/             # `phx` binary entry
├── std/                 # bundled standard library (lib package)
├── examples/            # demonstration programs (V0-052)
├── tests/
│   ├── cli/fixtures/    # regression fixtures (not user-facing tutorials)
│   ├── integration/     # Rust integration tests (`cli_e2e`, `diagnostics`, …)
│   └── phx-test/        # shared test helpers
└── docs/design/         # language authority (read before changing semantics)
```

Compiler pipeline order for language features: **lexer → parser → AST → resolver → typeck → lower → codegen → verifier → VM → tests**.

## Design authority

| Document | Use when |
|----------|----------|
| [docs/design/mvp.md](design/mvp.md) | MVP in/out of scope |
| [docs/design/language-v0.md](design/language-v0.md) | Language v0 checklist and phase status |
| [docs/design/grammar.ebnf](design/grammar.ebnf) | Formal grammar |
| [docs/design/features/type-system.md](design/features/type-system.md) | Types, literals, casts |
| [docs/design/features/ownership.md](design/features/ownership.md) | Moves, Copyable, borrows |
| [docs/design/features/vm-linear.md](design/features/vm-linear.md) | Bytecode and VM contract |
| [docs/design/features/debug.md](design/features/debug.md) | Debug metadata, dev/release, DAP roadmap |
| [docs/design/features/modules.md](design/features/modules.md) | `#import`, `phoenix.toml`, projects |
| [docs/design/features/traits.md](design/features/traits.md) | Traits and static dispatch |
| [docs/design/features/error-handling.md](design/features/error-handling.md) | `Result`, `?`, std errors |

Do not invent language behavior in code or docs — update the design doc first.

## Language v0 checklist

Track shipped work and remaining items in [docs/design/language-v0.md](design/language-v0.md).

## Tutorial: first program (binary)

1. Create a directory with `phoenix.toml`:

```toml
[project]
name = "my_app"
type = "bin"
module_src = "src"

[build]
dir = "build"
```

2. Add `src/main.phx`:

```phoenix
main :: () => {
    const answer: s32 = 42;
    const _ = answer;
};
```

3. Build and run from the project root:

```bash
just phx build
just phx run
```

Every executable package must define zero-argument `main :: () => { … }`.

## Tutorial: first library + app

**Library** (`my_lib/phoenix.toml`):

```toml
[project]
name = "my_lib"
type = "lib"
module_src = "src"
bundle_std = false

[build]
dir = "build"
```

`src/lib.phx` exports with `pub`:

```phoenix
pub double :: (n: s32) => s32 { n + n };
```

**App** (`my_app/phoenix.toml`) — dependency key must match the library `project.name`:

```toml
[project]
name = "my_app"
type = "bin"
module_src = "src"
bundle_std = false

[dependencies]
my_lib = { path = "../my_lib" }

[build]
dir = "build"
```

`src/main.phx`:

```phoenix
#import my_lib::double;

main :: () => {
    const _ = double(21);
};
```

```bash
cd my_app
just phx build
just phx run
```

Set `bundle_std = false` only when the example does not need bundled `std` (most std-using bins omit this field so `bundle_std` defaults to `true`).

## Demonstration programs

See [examples/README.md](../examples/README.md) for `hello`, `modules`, `generics`, and `errors` with copy-paste commands.

MVP has no standard I/O. Inspect computed values with:

```bash
just phx run path/to/main.phx --dump-main
```

## Rust contributors

- Coding standards: workspace `.cursor/rules/rust-standards.mdc` (plus `rust-unsafe.mdc`, `testing.mdc`) and [docs/contributing/rust-docs.md](contributing/rust-docs.md)
- Commit messages: `[stage]: description` (e.g. `[typeck]: enforce use-after-move`)
- PRs: `just pre-commit` must be green

## CLI and diagnostics

| Command | Purpose |
|---------|---------|
| `phx check <file>` | Type-check |
| `phx build` | Build a `phoenix.toml` project |
| `phx run` | Compile and execute on the VM |
| `phx explain E####` | Short explanation for a diagnostic code |

Integration tests live in `tests/integration/tests/cli_e2e.rs` and `diagnostics.rs` (not shell scripts).
