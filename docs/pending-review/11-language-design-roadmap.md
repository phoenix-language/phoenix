# Review: Language Design Roadmap (strategic)

## Summary

Phoenix’s **implicit philosophy** is consistent with the design docs: bytecode-first, no GC, ownership-flavored semantics, runtime transparency post-MVP, and errors-as-values in std—not exceptions. The implementation is faithfully **MVP-narrow** (no strings, no std I/O, no scheduler, move-only ownership). The largest strategic risks are deferring strings/std too long, half-built generics, and building runtime features before the type system can express schedulable I/O and actors.

---

## What is the language trying to be?

Based on `docs/design/mvp.md`, `runtime-transparency.md`, and the compiler’s choices, Phoenix aims to be a **systems language with a portable bytecode contract and an honest VM**, not a thin Rust clone. It prioritizes:

- **Predictable machine model** (stack VM, explicit memory, no hidden GC).
- **Static front end** with post-MVP **cooperative runtime** (scheduler, schedulable I/O, actors).
- **Transparency** — effects visible in types at call sites eventually.

The implementation matches that story for MVP (single-threaded interpreter, byte-first text, use-after-move). **Inconsistency:** design says deferred syntax should parse; compiler often rejects at parse ([`01-syntax-and-ast.md`](01-syntax-and-ast.md)).

---

## Top 5 architectural decisions (before scaling)

### 1. String / text representation

| Tradeoff | Wrong choice cost |
|----------|-------------------|
| `&[u8]` only vs language `str` vs interned atoms | Blocks std, ergonomics, and I/O APIs for months |

**Recommendation:** MVP std library uses **`[u8]` / `&[u8]` views** over owned buffers; add a distinct **`str` slice type** in the type system before user-facing string APIs (length-prefixed UTF-8, no null termination). Do not add a hidden GC string.

### 2. Generics strategy: monomorphization vs IR generics

| Tradeoff | Wrong choice cost |
|----------|-------------------|
| Rust-style mono vs boxed dyn vs Go-style erasure | Rework codegen, `.pxi`, and linker |

**Recommendation:** **Monomorphization at compile time** per instantiation site (Zig/Rust-like), keeping runtime vtables only for explicit `dyn Trait` later. Parser/generic AST already exist—next step is a monomorphization pass before IR, not full inference.

### 3. Separate compilation contract (`.pxi` / stable ids)

| Tradeoff | Wrong choice cost |
|----------|-------------------|
| Source-parse deps vs binary interfaces | Slow builds; broken incremental |

**Recommendation:** **`.pxi` v2` with structured types + stable symbol ids**; typecheck imports without parsing dependency bodies ([`03-resolver.md`](03-resolver.md), [`08-modules-and-build.md`](08-modules-and-build.md)).

### 4. Ownership roadmap: move-only vs full borrow checker

| Tradeoff | Wrong choice cost |
|----------|-------------------|
| Rust-like lifetimes vs Swift-style struct exclusivity vs leave unsafe | Either unsoundness or paralysis |

**Recommendation:** **Phase 1 (now):** fix shadowing moves ([`04-type-system.md`](04-type-system.md)). **Phase 2:** stack-only borrow rules for `&T`/`&mut T` without full region inference. **Phase 3:** actor boundary ownership per `ownership.md`. Do not jump to full rustc-style lifetimes before schedulable I/O types exist.

### 5. Runtime transparency in types (schedulable I/O)

| Tradeoff | Wrong choice cost |
|----------|-------------------|
| Marker types vs effect rows vs attributes | Post-MVP std I/O ships opaque “magic” calls |

**Recommendation:** Reserve a **`Schedulable` effect marker** (or `io` capability param) in the type system before implementing scheduler; even a stub enum prevents retrofit pain.

---

## Load-bearing features for “programs that do something observable”

Minimum set **after MVP checklist** (order matters):

1. **Heap `Alloc` + slices** wired end-to-end (compiler → VM)—already partially in VM ([`07-vm.md`](07-vm.md)).
2. **Byte buffer + fmt** in std (no primitive string required).
3. **Std `Option` / `Result` + `?`** as generic enums in library with typeck support ([`docs/design/features/type-system.md`](../../design/features/type-system.md)).
4. **One schedulable I/O primitive** (e.g. read file to buffer) with type marking schedulability.
5. **Process exit / print hook** for observability in tests (host syscall behind intrinsic).

Without (1)–(3), error propagation and text are blocked; without (4)–(5), runtime transparency remains theoretical.

---

## Generics: current state and cheapest path

**State:** Parser and AST support generic params on structs, enums, functions, aliases (`phx-syntax/tests/parser.rs`); typeck registers `GenericParam` defs but **no `Ty::Var` solving** ([`04-type-system.md`](04-type-system.md)).

**Cheapest usable path:**

1. **Explicit generic args only** at call sites (`foo::<s32>()`).
2. **Monomorphization pass** duplicates functions/types per concrete args before `lower`.
3. **Mangle export names** in `.pxi` for link (`foo$s32`).

**Prior art fit:** **Zig comptime + Rust mono** fit bytecode VM goals better than Go erasure or Java-style runtime generics. Swift’s reification is closest but Phoenix lacks ARC—mono is simpler.

---

## Ownership and borrowing: realistic safe memory model

**Current:** use-after-move on identifiers; no borrow exclusivity; references in signatures only.

**Path:**

1. Fix lexical shadowing and move paths (immediate).
2. **Intra-function borrow checking** for `&mut` exclusivity (LLVM-less, like early Rust alpha).
3. **Struct field borrows** with limited lifetime (caller must not destroy owner while borrow live).
4. **Actor messages** as owned transfers only (align with `messages.md`).

A **full borrow checker** is the right long-term goal for a no-GC language, but a **simpler “exclusive `&mut` + no escape”** model delivers most safety earlier. Phoenix should not adopt GC as the shortcut.

---

## String handling options

| Option | Fit for Phoenix |
|--------|-----------------|
| Rust `str`/`String` | Familiar; `String` implies alloc API—OK with explicit `Alloc` |
| Null-terminated C strings | Poor fit (unsafe, encoding ambiguity) |
| Length-prefixed UTF-8 | **Best default** for VM + FFI |
| Immutable interned atoms | Good for identifiers/keywords, not general text |

**Recommendation:** Language **`str` = UTF-8 slice** (`ptr + len`); **`String` = owned buffer** in std using `Alloc`. Source literals stay **`b"..."`** until `str` literals are designed.

---

## What could make Phoenix genuinely different?

### Defining characteristics (from design, not hype)

1. **Runtime transparency** — pure vs schedulable I/O vs actors visible at call sites (`runtime-transparency.md`). Few languages commit to this without `async` syntax.
2. **Bytecode-first product** — PHX0 portable, VM per platform, post-MVP scheduler—not LLVM-as-distribution.

### Implementation alignment

| Characteristic | Building toward? |
|----------------|------------------|
| Transparency | **Partially** — types do not yet mark schedulable I/O; MVP has no scheduler |
| Bytecode-first | **Yes** — pipeline centers PHX0 + verifier + VM |
| No GC ownership | **Yes** — move checking started; borrows not enforced |

**Risk of drifting away:** shipping std I/O as plain functions without schedulable types would **undermine** the main differentiator. **Recommendation:** Do not merge std I/O until effect markers exist, even if stubs.

---

## Cross-cutting issues (explicit links)

| Issue | Files |
|-------|-------|
| Multi-file diagnostics / module spans | [`02-diagnostics.md`](02-diagnostics.md), [`08-modules-and-build.md`](08-modules-and-build.md) |
| `.pxi` not on resolve path | [`03-resolver.md`](03-resolver.md), [`08-modules-and-build.md`](08-modules-and-build.md) |
| Ownership shadowing + IR erasure | [`04-type-system.md`](04-type-system.md), [`05-ir-and-lowering.md`](05-ir-and-lowering.md) |
| `compile_source` vs imports | [`03-resolver.md`](03-resolver.md), [`08-modules-and-build.md`](08-modules-and-build.md), [`09-testing-strategy.md`](09-testing-strategy.md) |

---

## Recommended next actions (strategic, ordered)

1. Lock **string/`str` design doc** update (length-prefixed UTF-8).
2. Ship **monomorphization-only generics** before inference.
3. Implement **`.pxi` v2** and import typecheck without parsing deps.
4. Fix **ownership shadowing**; publish phased borrow roadmap in `ownership.md`.
5. Add **schedulable I/O marker** type to design docs before any std I/O code.
