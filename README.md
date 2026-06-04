# Phoenix

**Experimental alpha** — statically typed language that compiles to portable bytecode and runs on a stack VM.

## What is Phoenix?

Phoenix targets systems-style programs without a garbage collector. Memory safety is moving toward ownership, moves, and (post-MVP) borrow checking; the MVP compiler already rejects use-after-move.

Programs compile to `**PHX0` bytecode** (portable on disk) and execute on a **Phoenix VM** (per platform). The long-term model keeps runtime effects visible—schedulable I/O and explicit actors are designed into the type system rather than hidden behind opaque OS-thread or GC abstractions. MVP runs `main` on a single-process stack interpreter with no scheduler and no standard I/O.

Post-MVP, errors are **values** (`Result`, `Option` in the standard library, `?` propagation)—not exceptions. Those types are not built into the MVP compiler.

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

More fixtures: [tests/cli/fixtures/](tests/cli/fixtures/) (e.g. [sample.phx](tests/cli/fixtures/sample.phx)).

## Basic syntax


| Topic        | Notes                                                                                                                                                                                        |
| ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Declarations | Uniform `::` style: `name :: (…) => T { … }`, `Name :: struct { … }`, `Name :: enum { … }`, `Name :: trait`, `Type :: impl`, `Type :: impl :: Trait`                                         |
| Bindings     | `const` and `var`; function parameters are always typed                                                                                                                                      |
| Types        | Numeric primitives (`s32`, `u32`, …), `bool`, `()`, tuples, raw pointers (`*T`), borrows in signatures (`&T`, `&mut T`), fixed arrays `[T; N]`, slices `[T]`; no primitive `string` in MVP   |
| Literals     | Integers default to `s32`; `42u` → `u32`; floats default to `f32`; byte strings `b"hi"` → `[u8; N]`                                                                                          |
| Casts        | No implicit numeric widening—use `expr as Type`                                                                                                                                              |
| Control flow | `if`, `match`, `while`, `loop`, `break`, `continue`, `return`, `given`                                                                                                                       |
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

Workspace checks: `cargo test --workspace`, `just test-lang` (CLI fixtures). MVP smoke project: [tests/cli/fixtures/mvp_acceptance/](tests/cli/fixtures/mvp_acceptance/). Details: [tests/cli/README.md](tests/cli/README.md).

## Language status

**MVP pipeline (in scope today):** lex → parse → resolve → type-check → lower → `PHX0` → verifier → stack VM. `main :: () => { … }` is required. Structs, enums, traits with static dispatch, control flow including `given`, explicit casts, `#import` / `pub` projects, and use-after-move checking are in place.

**Partial or narrow:** borrow types in signatures without a full borrow checker; fixed arrays and stack-backed slices; generics scaffold only.

**Post-MVP (not implemented):** M:N scheduler, schedulable I/O, actors and mailboxes, std I/O and networking, primitive `string`, std `Option` / `Result` / `?`, full ownership verifier, JIT, hot reload.

Implementation tracker: [docs/mvp-implementation-checklist.md](docs/mvp-implementation-checklist.md).

## Documentation


| Document                                                                                     | Purpose                                 |
| -------------------------------------------------------------------------------------------- | --------------------------------------- |
| [docs/design/README.md](docs/design/README.md)                                               | Design doc index                        |
| [docs/design/mvp.md](docs/design/mvp.md)                                                     | MVP in/out of scope                     |
| [docs/design/features/type-system.md](docs/design/features/type-system.md)                   | Types, literals, casts                  |
| [docs/design/features/ownership.md](docs/design/features/ownership.md)                       | Moves, Copyable, borrows (phased)       |
| [docs/design/features/vm-linear.md](docs/design/features/vm-linear.md)                       | Bytecode format and verifier            |
| [docs/design/features/modules.md](docs/design/features/modules.md)                           | `#import`, `pub`, file modules          |
| [docs/design/features/runtime-transparency.md](docs/design/features/runtime-transparency.md) | Runtime visibility (post-MVP direction) |
| [docs/mvp-implementation-checklist.md](docs/mvp-implementation-checklist.md)                 | What is implemented in `source/`        |


## License

Apache License 2.0 — see [LICENSE](LICENSE).