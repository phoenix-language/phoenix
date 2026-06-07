# Phoenix MVP Specification

Status: Draft (MVP lock)

This document defines the first compiler milestone for Phoenix. It is intentionally strict about scope so implementation can proceed without feature creep.

## Goal

Build a minimal Phoenix compiler pipeline:

1. Parse source files.
2. Type-check the MVP language subset.
3. Lower to bytecode.
4. Execute on a single-process stack VM.

## Required Entrypoint

Every executable Phoenix program must define:

```phoenix
main :: () => { /* ... */ }
```

- `main` does not need to be `pub`.
- File name is not important.
- Compilation fails if no `main` function is found.

Post-MVP runtime note: `main` is syntactically a normal function but bootstraps into an implicit root scheduler context. MVP runs `main` on a single stack interpreter without scheduler or I/O.

## MVP In Scope

| Area | Included in MVP |
|---|---|
| Core compilation | lexer, parser, AST, type checker, bytecode lowering |
| Runtime model | single-process stack VM interpreter |
| Declarations | `const`, `var`, function declarations |
| Types | numeric primitives, `bool`, tuples, unit `()`, raw pointers, borrow types (`&T`, `&mut T`), fixed arrays, slices/views, **`str` UTF-8 text view** |
| User types | `Name :: struct`, `Name :: enum`, type aliases |
| Traits | `Name :: trait`, `Type :: impl`, `Type :: impl :: Trait` (parse + static method resolution) |
| Control flow | `if`, `match`, `while`, `loop`, `break`, `continue`, `return`, `given` |
| Expressions | arithmetic, comparison, logical operators; explicit casts (`expr as Type`) |
| Modules (with Phase 2) | Multi-file `#import`, `pub`, whole-program load — see [language-v0.md](language-v0.md) Phase 2; **block-scoped `#import`** ([V0-014](language-v0.md#v0-014--block-scoped-import-mvp-modules)) |

## MVP Out of Scope

| Area | Deferred |
|---|---|
| Concurrency runtime | scheduler, schedulable I/O, actors, mailboxes, supervision |
| Actor language contracts | actor directives, actor trait enforcement, actor handles |
| Ownership safety model | full borrow checker and ownership verifier |
| Runtime sophistication | JIT, hot reload |
| Standard library breadth | collections, formatting, rich text/string APIs, **std I/O** (`File.read`, networking), **`Option` / `Result` / error propagation (`?`)** |
| Compile-time generation | `@derive(...)` or `#derive(...)` semantic codegen |
| Deferred grammar/semantics | default trait body codegen, associated-type bounds, heap alloc surface syntax, full borrow checker — see [features/grammar-deferred.md](features/grammar-deferred.md) |

## Lexer and identifiers (MVP)

- **ASCII identifiers only** — `snake_case` value names and `PascalCase` type names use ASCII rules; Unicode identifiers are post-MVP ([features/ast-roadmap.md](features/ast-roadmap.md)).
- **Deferred syntax is parsed, not lowered** — `for-in`, ranges, lambdas, `@` directives, and `#derive` build AST nodes; typeck reports `UnsupportedFeature` until std/runtime work lands ([features/grammar-deferred.md](features/grammar-deferred.md)). Bracket item attributes (`#[cfg]`, `#[deprecated]`, `#[allow]`, `#[must_use]`) are implemented per [compiler-directives.md](features/compiler-directives.md).

## Core Primitive Policy (Text View, Not Owned String)

MVP does not include a primitive **owned** `string` type (no GC string, no growable string builtin).

The core language provides:

- **`str`** — UTF-8 text view `(ptr, len)`; `"…"` literals; Copyable fat pointer; rodata-backed for literals
- **`u8`**, fixed arrays `[T; N]`, byte slices `[T]`, and `b"…"` for binary data
- raw pointer types and borrow types (`&T`, `&mut T`)
- explicit heap allocation primitive (runtime intrinsic; surface syntax deferred)
- unsafe pointer operations at VM-level boundaries

Owned growable text (`String`), formatting, and rich text APIs are **std library** design (post-MVP std pipeline).

## Deterministic MVP Type Rules

1. Numeric literals default to `s32`, `u32` (with suffix), and `f32`.
2. No implicit numeric widening or narrowing; casts must be explicit (`expr as Type`).
3. Local inference is allowed only when initializer type is unambiguous.
4. Function parameters must always be explicitly typed.
5. Omitted function return type defaults to `()`.
6. `if` and `match` expression branches must unify to one type.
7. Operator support in MVP is compiler-defined for primitive numerics and booleans only.

## Directive Model (Phased)

MVP compiler functionality does not depend on directive-heavy features.

Forward direction for readability:

- `#` for compile-time keyword directives (imports, unsafe regions, optimization hints)
- `#[...]` for item metadata (conditional compilation, deprecation, lint warnings — see [compiler-directives.md](features/compiler-directives.md))
- `@` for runtime directives (runtime actions)

Older drafts may use `@` for both categories; this is transitional and should be normalized in docs.

## Post-MVP Architecture Targets

These are design commitments, not MVP implementation requirements:

- **Implicit scheduler context:** all user code (including `main`) runs under VM scheduler management; there is no bare synchronous OS-thread execution for Phoenix programs.
- **Schedulable I/O:** safe std I/O parks the current context cooperatively; call sites encode schedulability in types (syntax TBD). Pure sequential code does not implicitly suspend.
- **Explicit actors (opt-in):** `@spawn` / message passing for isolation and supervision, not required for simple file reads.
- Actor = heap-resident state + mailbox when explicitly spawned.
- Scheduler = fixed worker pool with M:N context scheduling.
- Actor contract = actor-marked type must satisfy an `Actor` trait contract.
- VM runtime owns scheduling, I/O readiness, mailbox lifecycle, supervision orchestration, and crash isolation boundaries.
- VM runtime is the layer for hot reload and JIT strategy, while ownership semantics remain language/type-system responsibilities.

**Shipping order:** scheduler + schedulable-I/O runtime must exist before std I/O APIs (`File.read`, etc.) ship. MVP implements neither.

Full runtime model (principle and taxonomy): [features/runtime-transparency.md](features/runtime-transparency.md).

## Acceptance Checklist

- `main` requirement is defined and enforced in grammar/spec docs.
- Primitive inventory includes **`str` view**; excludes built-in owned `string`.
- Type rules are explicit and non-contradictory.
- Bytecode format is detailed enough to implement loader + VM without guessing.
- Deferred features are labeled post-MVP in all related docs.

**Implementation order:** The executable checklist for finishing MVP and reaching contributor-ready **Language v0** (std bootstrap) lives in [language-v0.md](language-v0.md).
