# Phoenix

**Experimental alpha** — statically typed language that compiles to portable bytecode and runs on a stack VM.

**Contributing:** [docs/contributing.md](docs/contributing.md) · **Language v0 checklist:** [docs/design/language-v0.md](docs/design/language-v0.md)

## What is Phoenix?

Phoenix targets systems-style programs without a garbage collector. Memory safety is moving toward ownership, moves, and (post-MVP) borrow checking; the MVP compiler already rejects use-after-move.

Programs compile to **`PHX0` bytecode** (portable on disk) and execute on a **Phoenix VM** (per platform). The long-term model keeps runtime effects visible—schedulable I/O and explicit actors are designed into the type system rather than hidden behind opaque OS-thread or GC abstractions. MVP runs `main` on a single-process stack interpreter with no scheduler and no standard I/O.

Errors are **values** (`Result`, `Option` in the standard library, `?` propagation)—not exceptions. The bundled `std` defines those types as ordinary Phoenix enums.

Authoritative scope: [docs/design/mvp.md](docs/design/mvp.md).

## Example

Every executable program needs a zero-argument `main`:

```phoenix
add :: (a: s32, b: s32) => s32 {
    a + b
};

main :: () => {
    const sum: s32 = add(10, 2);
    const ok: bool = { if sum > 0 { true } else { false } };
    const _ = ok;
};
```

More programs: [examples/](examples/) (demos) and [tests/cli/fixtures/](tests/cli/fixtures/) (regression fixtures).

## Basic syntax


| Topic        | Notes                                                                                                                                                                                        |
| ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Declarations | Uniform `::` style: `name :: (…) => T { … }`, `Name :: struct { … }`, `Name :: enum { … }`, `Name :: trait`, `Type :: impl`, `Type :: impl :: Trait`                                         |
| Bindings     | `const` and `var`; function parameters are always typed                                                                                                                                      |
| Types        | Numeric primitives (`s32`, `u32`, …), `bool`, `()`, tuples, raw pointers (`*T`), borrows in signatures (`&T`, `&mut T`), **Arrays** `[T; N]`, slices `[T]`; `str` views (no owned string); growable **`DynamicArray`** in std (post–Language v0) |
| Literals     | Integers default to `s32`; `42u` → `u32`; floats default to `f32`; byte strings `b"hi"` → `[u8; N]`; UTF-8 `"hi"` → `str`                                                                  |
| Casts        | No implicit numeric widening—use `expr as Type`                                                                                                                                              |
| Control flow | `if`, `if const` / `if var`, `match`, `while`, `loop`, `break`, `continue`, `return`                                                                                                          |
| Modules      | Files are modules; paths use `::`; `#import path::to::item`; `pub` exports. Single-file `phx check` needs `--module-src` when using `#import`—see [tests/cli/README.md](tests/cli/README.md) |
| Directives   | `#` compile-time (e.g. `#import`); `@` runtime (post-MVP, e.g. actors)                                                                                                                       |


Grammar overview: [docs/design/grammer.md](docs/design/grammer.md). Formal grammar: [docs/design/grammar.ebnf](docs/design/grammar.ebnf).

## Trying it

Build the `phx` compiler from the repo root:

```bash
cargo build -p phx
```

Single file (no `phoenix.toml`):

```bash
cargo run -p phx -- check path/to/file.phx
cargo run -p phx -- run path/to/file.phx
cargo run -p phx -- run path/to/file.phx --dump-main   # print main locals (MVP debug channel)
```

Multi-file modules:

```bash
cargo run -p phx -- check --module-src src path/to/main.phx
```

Project with `phoenix.toml` at the root (`module_src`, `[build].dir`, etc.):

```bash
cargo run -p phx -- build
cargo run -p phx -- run
```

**Just recipes** (requires [just](https://github.com/casey/just)):

```bash
git clone --recurse-submodules https://github.com/phoenix-language/phoenix.git

just phx run examples/hello/src/main.phx --dump-main
just pre-commit    # fmt, clippy, doc-check, dep-check, test, test-lang
just test-lang     # CLI E2E + diagnostics goldens
just test          # full workspace tests
just website dev   # marketing site (git submodule at website/)
```

Demonstration programs: [examples/README.md](examples/README.md). MVP smoke project: [tests/cli/fixtures/mvp_acceptance/](tests/cli/fixtures/mvp_acceptance/). Fixture details: [tests/cli/README.md](tests/cli/README.md).

## Language status

**Shipped (Language v0 substrate):** lex → parse → resolve → type-check → lower → `PHX0` → verifier → stack VM; `main` required; structs, enums, generics, traits with static dispatch and monomorphization; control flow including `if const` / `if var`; explicit casts; `#import` / `pub` / `phoenix.toml` projects; use-after-move checking; bundled `std` with `Option` / `Result`, core traits, conversion traits, and layered error types.

**Partial or narrow:** borrow types in signatures without a full borrow checker; **Arrays** `[T; N]` and stack-backed slices; no std I/O (use `phx run --dump-main` to inspect `main` locals).

**Post-MVP (not implemented):** M:N scheduler, schedulable I/O, actors and mailboxes, std I/O and networking, full ownership verifier, JIT, hot reload.

Implementation tracker: [docs/mvp-implementation-checklist.md](docs/mvp-implementation-checklist.md). Phase checklist: [docs/design/language-v0.md](docs/design/language-v0.md).

## Documentation


| Document                                                                                     | Purpose                                 |
| -------------------------------------------------------------------------------------------- | --------------------------------------- |
| [docs/contributing.md](docs/contributing.md)                                                 | Build, test, layout, tutorials          |
| [docs/design/README.md](docs/design/README.md)                                               | Design doc index                        |
| [docs/design/language-v0.md](docs/design/language-v0.md)                                     | Language v0 phased checklist            |
| [docs/design/mvp.md](docs/design/mvp.md)                                                     | MVP in/out of scope                     |
| [docs/design/features/type-system.md](docs/design/features/type-system.md)                   | Types, literals, casts                  |
| [docs/design/features/ownership.md](docs/design/features/ownership.md)                       | Moves, Copyable, borrows (phased)       |
| [docs/design/features/vm-linear.md](docs/design/features/vm-linear.md)                       | Bytecode format and verifier            |
| [docs/design/features/debug.md](docs/design/features/debug.md)                             | Debug layers, dev/release, tooling roadmap |
| [docs/design/features/modules.md](docs/design/features/modules.md)                           | `#import`, `pub`, `phoenix.toml`        |
| [docs/design/features/traits.md](docs/design/features/traits.md)                             | Traits and static dispatch              |
| [docs/design/features/error-handling.md](docs/design/features/error-handling.md)             | `Result`, `?`, std errors               |
| [docs/design/features/runtime-transparency.md](docs/design/features/runtime-transparency.md) | Runtime visibility (post-MVP direction) |
| [docs/mvp-implementation-checklist.md](docs/mvp-implementation-checklist.md)                 | What is implemented in `source/`        |


## License

Apache License 2.0 — see [LICENSE](LICENSE).
