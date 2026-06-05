# Phoenix Language Design

Phoenix is a statically typed language that compiles to bytecode and runs on a VM.

Current work is split into:

- MVP compiler and runtime contract (implemented first)
- post-MVP concurrency and advanced language/runtime features

See [mvp.md](mvp.md) for the canonical MVP boundary.

---

## MVP focus

The first milestone is intentionally narrow:

- bytecode compiler pipeline (lex -> parse -> type-check -> lower)
- single-process stack VM
- structs, enums, pattern matching
- user `struct` / `enum` (std `Option` / `Result` are post-MVP)
- typed variables and functions
- arithmetic/comparison/logical operators on primitive types
- required executable entrypoint: `main :: () => { ... }`

---

## Core type policy

Phoenix separates core language types from library-provided behavior.

- Core types are compiler-known and available in every module.
- Standard library APIs/traits are not globally auto-imported.
- A small future prelude may expose common traits only.

Important MVP decision:

- There is no primitive **owned** `string` in MVP; core text is the **`str`** UTF-8 view (`"…"` literals). Owned growable text is std **`String`** (deferred).
- MVP text/data handling is byte-first (`u8`, arrays, slices/views, pointers, allocation primitives).

---

## Directive model

Forward language model:

- `#` prefix = compile-time directives
- `@` prefix = runtime directives

---

## Design principles

- **[Runtime transparency through the type system](features/runtime-transparency.md)** — VM effects (schedulable I/O, actor boundaries, failure) are visible at call sites via types or `@…` syntax; pure sequential code does not implicitly suspend.

---

## Post-MVP direction

These are design targets, not MVP requirements:

- implicit scheduler context for all user code (including `main`)
- type-visible schedulable I/O (cooperative VM parking; no `async`/`await`)
- opt-in explicit actors and supervision
- ownership/borrow checker strengthening
- hot reload and JIT
- richer std ecosystem (including `File.read` and networking)
- derive/codegen helpers

**MVP has no std I/O.** Scheduler and schedulable-I/O runtime ship before std I/O APIs.

Post-MVP concurrency model:

- all user code runs in VM-managed execution contexts; worker-thread placement is not pinned in the type system
- schedulable I/O is distinguishable from pure computation by type at the call site
- simple I/O does not require `@spawn`; explicit actors are opt-in for isolation and message protocols
- scheduler = fixed worker pool with M:N context scheduling

See [features/runtime-transparency.md](features/runtime-transparency.md) for the full principle and call-site taxonomy.

## VM role in the full system

Phoenix ownership rules handle memory semantics in language/compiler space. The VM exists for runtime orchestration and portability:

- executes portable Phoenix bytecode across supported OS targets
- schedules all user code through an M:N worker-pool runtime
- intercepts safe I/O and parks contexts instead of blocking worker threads
- coordinates I/O readiness wakeups with the scheduler
- owns explicit actor lifecycle, mailboxes, and supervision/restart orchestration
- provides crash isolation boundaries between explicit actors
- enables post-MVP operational features like hot reload and JIT

MVP includes the bytecode interpreter/runtime contract only; scheduler, std I/O, explicit actor runtime, supervision, hot reload, and JIT remain post-MVP.

---

## Documentation map


| Document                                                           | Purpose                                                      |
| ------------------------------------------------------------------ | ------------------------------------------------------------ |
| [mvp.md](mvp.md)                                                   | MVP in/out scope and entrypoint contract                     |
| [grammer.md](grammer.md)                                           | Human-readable syntax overview                               |
| [grammar.ebnf](grammar.ebnf)                                       | Formal EBNF and lexical grammar                              |
| [features/grammar-deferred.md](features/grammar-deferred.md)       | Parse-only and not-yet-designed grammar items                |
| [features/type-system.md](features/type-system.md)                 | Core vs std, deterministic MVP typing rules                  |
| [features/traits.md](features/traits.md)                           | Trait model and baseline roadmap                             |
| [features/vm-linear.md](features/vm-linear.md)                     | VM invariants and bytecode format contract                   |
| [features/compiler-directives.md](features/compiler-directives.md) | Compile-time vs runtime directive taxonomy                   |
| [features/runtime-transparency.md](features/runtime-transparency.md) | Runtime transparency principle and call-site taxonomy |
| [features/concurrency.md](features/concurrency.md)                 | Scheduler, schedulable I/O, explicit actors |


