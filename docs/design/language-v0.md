# Language v0 roadmap

Status: Active checklist

This document is **the list** — the ordered checklist that marks the end of MVP, what is required to **show Phoenix to contributors**, and what must land before **writing the standard library in Phoenix source**.

When every item in **Phases 1-6** is checked, the project reaches **Language v0**: a demonstrable, contributor-ready compiler that can host an in-language std bootstrap.

**Authority:** Language semantics come from the design docs. This file only sequences work and defines acceptance criteria. If behavior is ambiguous, update the relevant design doc first — do not invent semantics in implementation.

---

## What Language v0 means


| Milestone                      | Meaning                                                                                                                                            |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| **MVP** (Phase 1)              | Minimal compiler pipeline: parse → type-check → bytecode → single-process VM. See [mvp.md](mvp.md).                                                |
| **Language v0** (Phases 1-6)   | MVP is complete **plus** the language substrate, packaging, and std-bootstrap wiring needed to compile real library code and onboard contributors. |
| **Std v0** (after Language v0) | `Option`, `Result`, core traits, conversion traits, std error types, alloc-backed collections/text — authored as Phoenix `type = lib` packages, not compiler builtins. |
| **Std I/O** (explicitly later) | Requires scheduler + schedulable-I/O runtime first. Not a Language v0 gate. |
| **Runtime v1** (after Std v0) | VM-managed M:N scheduler, cooperatively parked schedulable I/O, optional actors — compile-time safety from Language v0 is the prerequisite. |

---

## Product vision (why Language v0 matters)

Language v0 deliberately ships a **compile-time-safe frontend** and a **minimal deterministic VM** — enough to author std in Phoenix, verify bytecode, and run programs on any host with a Phoenix VM.

That split is intentional. Many runtime features Phoenix targets (cooperative scheduling, schedulable I/O that does not block worker threads, actor isolation, portable PHX0 bytecode, C-ABI interop) are **hard to build safely on an unsafe or dynamically typed core**. Language v0 establishes the contract first:

| Language v0 delivers | Enables later |
|---|---|
| Static types, moves, `Result`/`Option`, trait bounds | Failures and resource ownership visible before runtime |
| Monomorphized traits + `From` error conversion | Ergonomic layered errors without exceptions |
| Portable PHX0 + verifier | Same bytecode on embedded, server, and desktop targets |
| Function pointers + `@extern` path ([V0-053](#v0-053--function-pointers-and-indirect-calls)) | Interop with any C-ABI language at documented boundaries |
| Minimal stack VM | Room to add scheduler, I/O parking, and mailboxes without rewriting the language |

**Post-Language v0 direction** (documented, not v0 scope): a **VM-managed concurrency model** — all Phoenix execution under a scheduler; I/O and blocking work cooperatively parked (Tokio-like transparency, Phoenix syntax and types). See [runtime-transparency.md](features/runtime-transparency.md), [concurrency.md](features/concurrency.md).

Language v0 does **not** implement the scheduler or std I/O. It **does** require the type system and std error story to be strong enough that runtime work composes cleanly on top.

---

## How agents should use this list

1. Work **top to bottom** within a phase. Do not skip ahead unless a later item has no dependency on open items (called out in each step).
2. Complete **one checklist item per task** when possible. Each item has an ID (`V0-NNN`), acceptance criteria, and doc links.
3. Mark an item done only when its acceptance criteria pass and `just pre-commit` is green from the repo root.
4. Add or extend **tests** (unit, integration, or `tests/cli/`) for every item that changes observable behavior.
5. If an item requires a design decision not covered in docs, **stop and update the design doc** before coding.

### Global completion gate

From the repo root, before marking any phase complete:

```bash
just pre-commit
```

For changes that touch compiler passes or VM semantics, also run:

```bash
just test
```

---

## Phase 1 — MVP compiler contract

Finish the minimal pipeline defined in [mvp.md](mvp.md). Nothing in later phases replaces this work.

### V0-001 — End-to-end pipeline

- Lex → parse → resolve → type-check → lower → bytecode serialize → verifier → VM execute works for single-file programs.
- Every executable program requires `main :: () => { … }`; missing `main` is a compile error with a clear diagnostic.

**Acceptance:** At least one integration test runs a `.phx` program end-to-end on the stack VM and exits successfully.

**Refs:** [mvp.md](mvp.md), [vm-linear.md](features/vm-linear.md)

---

### V0-002 — Core types and literals

- Numeric primitives (`s32`, `u32`, `f32`, etc.), `bool`, unit `()`, tuples.
- Raw pointers and borrow types `&T`, `&mut T` in signatures (full borrow checking is **not** required).
- Fixed arrays `[T; N]` and slices `[T]`.
- `**str`** UTF-8 text view: `"…"` literals, `(ptr, len)` representation, Copyable fat pointer over rodata.
- Literal rules: `s32` default integers, `u` suffix → `u32`, `f32` default floats; **no implicit numeric widening**.

**Acceptance:** Type-check and run programs using `str`, slices, arrays, and explicit `expr as Type` casts.

**Refs:** [mvp.md](mvp.md) (Core Primitive Policy), [type-system.md](features/type-system.md)

---

### V0-003 — User types and control flow

- `Name :: struct`, `Name :: enum`, type aliases.
- `if`, `if const` / `if var`, `match`, `while`, `loop`, `break`, `continue`, `return`.
- Arithmetic, comparison, and logical operators on primitive numerics and `bool` only.

**Acceptance:** Integration tests cover struct/enum construction, `match` exhaustiveness errors, and control-flow lowering.

**Refs:** [grammar.ebnf](grammar.ebnf), [mvp.md](mvp.md)

---

### V0-004 — Traits and static dispatch

- `Name :: trait`, `Type :: impl`, `Type :: impl :: Trait` parse, resolve, and type-check.
- Static method resolution and monomorphized `Call` lowering for known callees.

**Acceptance:** A program with an inherent `impl` and a trait `impl` compiles and runs; missing trait methods are compile errors.

**Refs:** [traits.md](features/traits.md)

---

### V0-005 — MVP ownership

- Use-after-move is a compile error; diagnostic cites the original move site.
- `Copyable` bootstrap for primitives and `str` (language marker, not full std trait yet).

**Acceptance:** `tests/integration/diagnostics/use_after_move` (or equivalent) passes; moved non-Copyable values reject further use.

**Refs:** [ownership.md](features/ownership.md)

---

### V0-006 — Deferred syntax boundaries

- Parsed-but-unsupported constructs (`@spawn`, `#derive` semantics, closures, etc.) produce `UnsupportedFeature` (or equivalent) at type-check — not silent miscompilation.
- `Option` / `Result` / `Some` / `None` / `Ok` / `Err` / `?` may parse but **must** be rejected until Phase 5 (std bootstrap).

**Acceptance:** [grammar-deferred.md](features/grammar-deferred.md) table is reflected in compiler behavior and tests.

**Refs:** [grammar-deferred.md](features/grammar-deferred.md)

---

## Phase 2 — Modules and project tooling

Multi-file programs and a real project layout so contributors can build packages, not just single scripts.

### V0-010 — M1 whole-program modules

- Files are modules; `#import`, `pub`, and `::` paths per [modules.md](features/modules.md).
- Whole-program compile: load all reachable `.phx` files, reject import cycles with a cycle trace.
- Single linked PHX0 output for workspace builds.

**Acceptance:** Multi-file program with cross-module `#import` of `pub` items compiles and runs.

**Refs:** [modules.md](features/modules.md) (M1)

---

### V0-011 — `phoenix.toml` and M2 build driver

- `phoenix.toml` project root discovery; `project.type` = `bin` | `lib`.
- `bin` requires `main.phx` at `module_src` root; `lib` requires `lib.phx`; `main` forbidden in lib packages.
- `phx build`, `phx run`, `phx check` wired to `module_src` and dependency graph.
- `build/` artifact layout: `manifest.json`, `build/pxi/`, `build/phx0/`, `build/bin/`, `build/lib/`, `build/deps/`.

**Acceptance:** Sample `bin` and `lib` projects build from `phoenix.toml`; `phx run` executes `build/bin/{name}.phx0`.

**Refs:** [modules.md](features/modules.md) (M2, CLI)

---

### V0-012 — `.pxi` interfaces and incremental rebuild

- Emit `.pxi` v2 (`format_version: 2`) with `logical_module`, `source_hash`, structured `ty`, and `dependencies`.
- Rebuild when source hash or dependency `pxi_hash` changes.
- `phx check` uses project layout when `phoenix.toml` is found (no `build/` required).

**Acceptance:** Touching one module rebuilds only stale modules and transitive importers; `.pxi` drives cross-module type-checking.

**Refs:** [modules.md](features/modules.md), [pxi-format.md](features/pxi-format.md)

---

### V0-013 — PHX0 linker and path dependencies

- Link workspace `build/phx0/*.phx0` and dependency artifacts into `build/bin/` or `build/lib/`.
- Globally unique `function_id` across modules; cross-module `Call` uses pre-assigned ids.
- Path dependencies: `[dependencies]` with `path = "…"`; key must equal depended `project.name`.

**Acceptance:** App package depends on a `lib` package via path; linked binary calls across package boundary.

**Refs:** [modules.md](features/modules.md) (Linker contract)

**Status:** Done

---

### V0-014 — Block-scoped `#import` (MVP modules)

- `#import` allowed inside `{ … }` blocks (function bodies, `if`/`while`/`loop` arms, nested blocks).
- Same import forms as file scope: single item, `{ A, B, … }`, glob `{ * }`.
- Block import names visible only in that block and nested scopes; normal shadowing rules apply.
- Block `#import` participates in whole-program module loading (graph discovery), not only name binding.
- Compile-time only: `pub` exports only; no runtime module loader.

**Acceptance:** A program imports a `pub` fn only inside `main` (no file-top import of that symbol), type-checks, compiles, and runs correctly; the name is unresolved outside the block.

**Refs:** [modules.md](features/modules.md) (Scoped imports)

**Status:** Done

---

## Phase 3 — Generics and trait substrate

Everything std types are built from: generic enums, bounds, associated types, monomorphization.

### V0-020 — Generic declarations

- Generic `struct`, `enum`, `type` alias, `trait`, `impl`, and functions with `:: <T, …>` parameters.
- Explicit instantiation `id :: <s32> (1)` and local call-site inference when arguments constrain type parameters.

**Acceptance:** Generic `enum` and function monomorphize to distinct symbols (e.g. `id$s32`); inference and explicit args both work.

**Refs:** [type-system.md](features/type-system.md) (Generics strategy)

**Status:** Done

---

### V0-021 — Monomorphization pass

- Collect instantiation sites; emit specialized layouts and bodies per `(template, args…)`.
- Trait bounds checked when concrete type arguments are known at instantiation.

**Acceptance:** Two call sites with different type args produce two specialized definitions in bytecode/layout tables.

**Refs:** [type-system.md](features/type-system.md), [traits.md](features/traits.md)

**Status:** Done

---

### V0-022 — Generic enum constructors and patterns

- Enum ctor calls: `Variant(1)` and `Variant :: <s32> (1)` with inference.
- `match` on generic enums with monomorphized patterns.

**Acceptance:** Generic enum ctors and pattern match compile across at least two monomorphized instantiations.

**Refs:** [type-system.md](features/type-system.md)

**Status:** Done

---

### V0-023 — Trait bounds and associated types (basic)

- Generic bounds `T: Copyable`, `T: SomeTrait` enforced at monomorphization.
- Associated types: `type Item;` on traits and concretely specified on `impl`.

**Acceptance:** Program with `Iterator :: trait { type Item; … }` and a concrete `impl` type-checks and resolves `Self::Item`.

**Refs:** [traits.md](features/traits.md)

**Status:** Done

---

### V0-024 — Cross-crate generic exports (`.pxi` mangling)

- Stable mangled `export_id` for monomorphized symbols (e.g. `sort$s32`) in `.pxi` export lists.
- Importers can call specialized generics from a dependency without re-parsing its source.

**Acceptance:** Lib package exports a generic function; bin package calls a concrete instantiation via path dependency and `.pxi` only.

**Refs:** [type-system.md](features/type-system.md) (`.pxi` export mangling), [pxi-format.md](features/pxi-format.md)

**Status:** Done

---

## Phase 4 — Runtime primitives for std

Std collections and owned text need heap allocation — not a GC, not a primitive owned `string`.

### V0-030 — Heap allocation intrinsic

- VM `ALLOC` (or documented equivalent) opcode implemented and verified.
- Compiler-lowering surface for allocation (canonical spelling locked in design — candidate: `core::alloc` module wrapping intrinsic).
- Documented deallocation / ownership contract for std authors (MVP may be manual `free` intrinsic or defer `Drop` to Phase 6).

**Acceptance:** Phoenix program allocates a byte buffer on the heap, writes via pointer, runs on VM without verifier failure.

**Refs:** [grammar-deferred.md](features/grammar-deferred.md) (Heap `ALLOC`), [mvp.md](mvp.md), [vm-linear.md](features/vm-linear.md)

---

### V0-031 — No primitive owned `string`

- Confirm no language builtin growable `string` type exists.
- `str` remains the only core text type; owned growable text is reserved for std `String`.

**Acceptance:** Type checker has no owned-string primitive; design docs and implementation agree.

**Refs:** [mvp.md](mvp.md), [type-system.md](features/type-system.md)

---

## Phase 5 — Std bootstrap wiring

Wire the compiler to std-defined types — **not** new `Ty::Option` / `Ty::Result` builtins.

### V0-040 — Std package layout

- Create `std` (or `phoenix-std`) as `type = lib` with `lib.phx` root module per [modules.md](features/modules.md).
- Path-dependency workflow documented for local development (`phoenix.toml` example in repo).

**Acceptance:** `phx build` produces `build/lib/std.phx0` (or chosen package name) from Phoenix source only.

**Refs:** [modules.md](features/modules.md)

**Status:** Done

---

### V0-039 — Item attributes and conditional compilation

- Bracket item attributes `#[...]` alongside existing `#keyword` directives (`#import`, `#unsafe`, `#derive`, …).
- v1 attributes: `#[cfg(...)]`, `#[deprecated(...)]`, `#[allow(...)]`, `#[must_use]`.
- `#[cfg]` strips inactive items before resolve; host `target_os` / `target_arch` / `debug_assertions` defaults.
- Deprecated and must-use produce warnings (not errors); `#[allow(...)]` suppresses within scope.

**Acceptance:** `#[cfg(target_os = "...")]` removes inactive code; deprecated use warns with note/suggestion and `#[allow(deprecated)]` suppresses; `#[must_use]` warns on discarded returns; existing keyword directives still compile; `just pre-commit` green.

**Refs:** [compiler-directives.md](features/compiler-directives.md)

**Status:** Done

---

### V0-041 — Core std types as ordinary generic enums

- `std::core::option::Option` and `std::core::result::Result` as `pub` generic enums in Phoenix source (`std/src/core/option.phx`, `std/src/core/result.phx`).
- Type-check monomorphized `Option<T>` and `Result<T, E>` like any user generic enum once std is linked.
- Bundled std: `bundle_std = true` by default; opt out in `phoenix.toml`.

**Acceptance:** Program `#import`s `std::core::option::Option` / `std::core::result::Result` and uses them in signatures and `match` without compiler builtins.

**Status:** Done

**Refs:** [type-system.md](features/type-system.md) (Phased: Option and Result), [error-handling.md](features/error-handling.md)

---

### V0-042 — Std constructors and `?` sugar

- Enable `Some`, `None`, `Ok`, `Err` as enum constructors tied to std definitions.
- Enable `expr?` postfix sugar lowered against `Result` / `Option` in compatible function contexts.

**Status:** Done

**Acceptance:** `read_config`-style example from [error-handling.md](features/error-handling.md) type-checks and lowers correctly (`tests/cli/fixtures/std_try`).

**Refs:** [error-handling.md](features/error-handling.md), [grammar-deferred.md](features/grammar-deferred.md)

---

### V0-043 — Core traits in std

- `Clone :: trait` in std with explicit duplication semantics.
- `Copyable` marker trait in std (or documented split: language bound vs std trait) per [ownership.md](features/ownership.md).
- Baseline traits stubbed or implemented: `Debug`, `PartialEq`, `Eq` (minimal fmt/compare sufficient for demos).
- Conversion traits (`From`, `Into`, `TryFrom`, `TryInto`) are **[V0-058](#v0-058--conversion-traits-from--into-in-std)** — separate checklist item.

**Status:** Done

**Acceptance:** Generic function with `T: Copyable` and `T: Clone` bounds type-checks against std trait definitions.

**Refs:** [ownership.md](features/ownership.md), [traits.md](features/traits.md)

---

### V0-044 — Prelude (minimal)

- Optional small prelude re-exports common std items (`Option`, `Result`, core traits) — not the entire library.
- Prelude behavior documented; most std remains explicit `#import`.

**Status:** Done

**Acceptance:** Prelude-enabled module uses `Option` without explicit import; non-prelude modules still require `#import`.

**Refs:** [type-system.md](features/type-system.md) (Core vs std policy)

---

### V0-058 — Conversion traits (`From` / `Into`) in std

**Status: Done**

- `std::core::convert`: `From<Source>`, `Into<Target>`, `TryFrom<Source>`, `TryInto<Target>` as ordinary generic traits in Phoenix source.
- Type-check and monomorphize trait method calls like any other trait impl.
- Document orphan-rule expectations for std error `From` impls.

**Acceptance:** Generic function with bound `T: From<U>` compiles, monomorphizes, and runs; `TryFrom` returns `Result`; no compiler conversion builtins beyond `expr as Type` for Tier A casts.

**Refs:** [traits.md](features/traits.md#conversion-traits-from--into), [type-system.md](features/type-system.md#explicit-cast-tiers)

---

### V0-059 — `?` with `From` error conversion

**Status: Done**

- Extend `?` type-check: `Result<T, E_in>?` inside `Result<T, E_out>` when `From<E_in>` exists for `E_out` (same `T`; Ok types must unify).
- Lower failure path: load `Err` payload → monomorphized `From::from` → `return Err(converted)`.
- Diagnostic when `From` is missing: cite expected impl and link to [error-handling.md](features/error-handling.md).

**Acceptance:** Function returning `Result<Config, AppError>` may `?` a `Result<_, LeafError>` call when `From<LeafError> for AppError` exists; fixture in `tests/cli/fixtures/std_try_from/`.

**Refs:** [error-handling.md](features/error-handling.md#the--operator-v0-042-error-conversion-v0-059)

---

### V0-060 — Std error trait

**Status: Done**

- `std::core::error::Error` marker trait (Rust-inspired; supertrait bounds deferred).
- Concrete error types implement `Error` in user crates; layered `From` in fixtures.

**Acceptance:** `std::core::error::Error` imports and `Result` + `?` smoke (`std_errors`); `E: Error` bound (`std_traits`); layered `From` (`std_try_from`); `examples/errors` builds with bundled std.

**Refs:** [error-handling.md](features/error-handling.md#std-error-vocabulary--v0-060)

---

### V0-061 — Rust-style `mod.phx` module entries

**Status: Done**

- Child module declarations (`mod name`, `pub mod name`) gate module loading for all package kinds (`bin`, `lib`, path deps, bundled `std`).
- `pub reexport :: Item` and `pub reexport :: child::Item` build parent export maps and `.pxi` surfaces.
- Orphan / ambiguous / missing module entry diagnostics; cross-package deep imports require `pub mod`.

**Acceptance:** `std::core::error::Error` (not `std::core::error::error::Error`); `modules/` and `mvp_acceptance/` fixtures build; negative fixtures for missing `mod.phx` and orphan files; `just pre-commit` green.

**Refs:** [modules.md](features/modules.md#mod-and-barrel-syntax)

---

## Phase 6 — Contributor showcase and std authoring enablers

Polish and capabilities that make the project legible to new contributors and unblock real std development.

### V0-050 — CLI and diagnostics UX

**Status: Done**

- `phx check`, `phx build`, `phx run`, `phx explain <code>` (or equivalent) with stable, tested diagnostic output.
- Errors show file, line, column, span snippet, and error code; internal panics never leak to users.
- `phx run --dump-main` documents the MVP VM debug channel for inspecting `main` locals.

**Acceptance:** `tests/integration/tests/cli_e2e.rs` and `diagnostics.rs` pass; diagnostic fixtures match committed `.stderr` golden files; `just test-lang` includes both.

**Refs:** [tests/cli/fixtures/](../../tests/cli/fixtures/), [tests/integration/diagnostics/](../../tests/integration/diagnostics/)

---

### V0-051 — Contributor documentation

**Status: Done**

- [docs/contributing.md](../contributing.md) explains: build, test, project layout, and **this checklist**.
- “First program” and “first library” tutorials using `phoenix.toml`.
- Link to design authority table in [mvp.md](mvp.md) / [README.md](../../README.md).

**Acceptance:** New contributor can clone, run `just pre-commit`, build a `bin` + `lib` example without reading the compiler source.

---

### V0-052 — Demonstration programs

**Status: Done**

- [examples/](../../examples/) with at least:
  - `hello` — `main`, `str` literal; `phx run --dump-main` VM debug channel.
  - `modules` — multi-file `bin` + `lib` dependency.
  - `generics` — generic enum + trait bound monomorphization.
  - `errors` — `Result` + `match` + `?` with `From` error conversion across std error types (V0-058-060).
- Each example has a one-line `//` README comment at the top of `main.phx`.

**Acceptance:** All examples build and run via documented commands in `just test-lang` (`cli_e2e` examples tests).

---

### V0-053 — Function pointers and indirect calls

- Function pointer **value** types: pointer-sized, Copyable.
- `IndirectCall` / `CALL_INDIRECT` in bytecode and VM.
- Comparator/callback parameters in generic functions (e.g. sort hook).

**Acceptance:** Program passes a function pointer to another function and invokes it indirectly; verifier enforces stack and type contract.

**Refs:** [type-system.md](features/type-system.md) (Callable values: Layer 2), [vm-linear.md](features/vm-linear.md)

**Status: Done**

---

### V0-054 — `Drop` trait and resource cleanup

- `Drop :: trait` in std; compiler emits drop glue at scope end for owned values that implement `Drop`.
- Documented interaction with heap alloc from V0-030.

**Acceptance:** Owned wrapper with `Drop` runs cleanup on scope exit; double-drop or use-after-drop rejected.

**Refs:** [traits.md](features/traits.md), [ownership.md](features/ownership.md)

**Status: Done**

---

### V0-055 — Iterator protocol and `for` lowering

- `Iterator :: trait` in std with associated `Item`.
- `for x in y` lowers against iterator protocol (or documented desugaring to `while` + `next`).

**Acceptance:** `for` loop over a std-provided iterator compiles and runs.

**Refs:** [grammar-deferred.md](features/grammar-deferred.md), [traits.md](features/traits.md)

**Status: Done**

---

### V0-056 — `#derive(...)` (minimal)

- `#derive(Copyable, Debug, PartialEq)` (or agreed subset) expands to trait `impl`s at compile time.
- Derive list and supported traits documented.

**Acceptance:** Struct with `#derive(PartialEq)` compares equal for identical fields; unsupported derives error clearly.

**Refs:** [grammar-deferred.md](features/grammar-deferred.md), [traits.md](features/traits.md)

**Status: Done**

---

### V0-057 — Opaque / newtype wrappers

- Rust-style **tuple structs**: `Millimeters :: struct(u32);` — distinct from transparent `type Alias = T`.
- Construct with ctor call `Millimeters(500)`; access via `.0` / impl methods; no implicit assignability with inner field types.
- `#derive` on tuple structs; tuple field postfix `.0` in [grammar.ebnf](grammar.ebnf).

**Acceptance:** `millimeters.phx` — derive + impl + ctor + pass to `fn(Millimeters)`; negative fixture rejects implicit inner use; `just pre-commit` green.

**Refs:** [type-system.md](features/type-system.md#type-aliases-vs-opaque-newtypes-phased), [ownership.md](features/ownership.md#tuple-struct-moves-v0-057), [grammer.md](grammer.md#tuple-structs-opaque-wrappers-v0-057), [traits.md](features/traits.md#derive-v0-056), [grammar.ebnf](grammar.ebnf)

**Status: Done**

---

## Language v0 complete — definition of done

When **all** items in Phases 1-6 are checked:


| Outcome                                                 | Status  |
| ------------------------------------------------------- | ------- |
| MVP compiler contract                                   | Shipped |
| Multi-file projects with `phoenix.toml`                 | Shipped |
| Generics + traits + monomorphization                    | Shipped |
| Heap alloc for std data structures                      | Shipped |
| `Option` / `Result` / core traits as Phoenix std source | Shipped |
| Conversion traits + std error types + `?`/`From` wiring | Shipped |
| Contributor docs + examples                             | Shipped |
| Fn pointers, `Drop`, iterators, basic derive            | Shipped |


**You may start Std v0 authoring in earnest** — growable `String`, `Vec`, formatting, collections — using the lib package model.

Announce **Language v0** when the demonstration programs in V0-052 run and `just pre-commit` is green on `main`.

---

## Explicitly not Language v0 (do not block the checklist)

These are real Phoenix goals but **out of scope** for this list. Do not implement them while Phases 1-6 are open unless a design doc is updated to promote an item.


| Feature                                                     | Why deferred                                     | When                                                                                                                        |
| ----------------------------------------------------------- | ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| Scheduler + M:N runtime                                     | Std I/O parks contexts cooperatively             | Before std `File.read` / networking                                                                                         |
| Schedulable I/O types                                       | Call-site syntax not locked                      | With scheduler                                                                                                              |
| Actors (`@spawn`, mailboxes)                                | Post-MVP isolation model                         | After scheduler                                                                                                             |
| Full borrow checker                                         | MVP is use-after-move only                       | Parallel to std maturation                                                                                                  |
| Closures                                                    | Layer 3 callable values                          | After fn pointers                                                                                                           |
| Module namespace import values (`const m = #import …; m.f`) | Needs module ref type + fn pointer member access | With [V0-053](#v0-053--function-pointers-and-indirect-calls); see [modules.md](features/modules.md#import-evolution-phased) |
| Qualified paths without `#import`                           | Expr-path resolution spec                        | After or with block imports; [modules.md](features/modules.md#import-evolution-phased)                                      |
| `dyn Trait`                                                 | Runtime vtables; static mono first               | When plugin-style APIs needed                                                                                               |
| Std I/O and networking                                      | Requires schedulable runtime                     | After scheduler lands                                                                                                       |
| Layered debug protocol (symbols, trace, breakpoints, DAP)   | Interim `--dump-main` only; full spec in [debug.md](features/debug.md) | Parallel to scheduler; D1+ post-Language v0                                                                      |
| JIT, hot reload                                             | Operational                                      | Post-v0                                                                                                                     |
| Unicode identifiers                                         | ASCII-only for v0                                | [ast-roadmap.md](features/ast-roadmap.md)                                                                                   |
| Primitive owned `string`                                    | Std `String` only                                | Never as language primitive                                                                                                 |


**Refs:** [mvp.md](mvp.md), [runtime-transparency.md](features/runtime-transparency.md), [concurrency.md](features/concurrency.md)

---

## After Language v0 — Std v0 starting point

First std modules to author **in Phoenix** once the checklist is complete (order is flexible; all depend on Phases 4-6):

1. `**core::alloc`** — allocation/deallocation wrappers over VM intrinsics.
2. `**core::option` / `core::result` / `core::convert**` — enums and conversion traits (if not already in std from V0-041 / V0-058).
3. `**error**` — `std::core::error::Error` trait (V0-060); concrete types + layered `From` in user crates.
4. `**core::clone` / `core::copyable` / `core::cmp` / `core::fmt**` — traits and minimal derive support.
5. `**collections::vec**` — growable buffer over `alloc`.
6. `**text::string**` — owned UTF-8 `String` over `alloc` + `Clone`.
7. `**text::fmt**` — basic formatting builders (no OS I/O required).

Std I/O (`fs`, `net`, …) waits for scheduler + schedulable-I/O runtime per [modules.md](features/modules.md) and [mvp.md](mvp.md).

---

## Related documents


| Document                                            | Role                                   |
| --------------------------------------------------- | -------------------------------------- |
| [mvp.md](mvp.md)                                    | Canonical MVP in/out scope             |
| [modules.md](features/modules.md)                   | Packages, `.pxi`, linker, CLI          |
| [type-system.md](features/type-system.md)           | Generics, core vs std, callable layers |
| [ownership.md](features/ownership.md)               | Copyable, Clone, Drop                  |
| [traits.md](features/traits.md)                     | Trait baseline roadmap                 |
| [grammar-deferred.md](features/grammar-deferred.md) | Parse vs implement boundaries          |
| [error-handling.md](features/error-handling.md)     | Result, `?`                            |
| [vm-linear.md](features/vm-linear.md)               | Bytecode and verifier contract         |


