# Deferred grammar and semantics

This document tracks Phoenix syntax that is **parsed in [grammar.ebnf](../grammar.ebnf)** but not fully implemented in MVP, and syntax that is **not yet in the grammar** because the design is incomplete.

Use this when implementing the compiler: parse vs type-check vs codegen boundaries.

---

## Parsed now; semantics or codegen deferred

| Feature | Grammar | MVP compiler behavior | Deferred work |
|---------|---------|----------------------|---------------|
| `#[derive(...)]` on fn/trait | On functions, traits | Parse `#[derive]`; reject at typeck | — |
| `#[derive(...)]` on struct/enum | On structs, enums | **Implemented** — expand to trait impls ([V0-056](../language-v0.md#v0-056--derive-minimal)): `Copyable`, `PartialEq`, `Debug` on structs and enums (including generic types with inferred trait bounds on type parameters) | `Clone`/`Eq`, custom derives |
| `#derive(...)` | Any item | **Rejected** at parse — use `#[derive(...)]` | — |
| `@spawn` / `@send` / `@receive` / `@reply` | Expression-level | Parse; reject or warn at typeck | Post-MVP actor runtime |
| `for x in y` | Statement | **Implemented** — `IntoIter` + `Iterator` desugaring ([V0-055](../language-v0.md#v0-055--iterator-protocol-and-for-lowering)) | — |
| `0..n` / `0..=n` | Expression (`range_expr`) | Parse; reject at typeck | Std range literal syntax; use `Range { start, end }` until then |
| `start..end` / `start..=end` | Pattern (`range_pattern`) | Parse; reject at typeck | Range patterns in `match` / `if` bindings; semantics TBD |
| `lambda_expr` | `(params) => expr \| block` | Parse | Closure typing, capture, lowering TBD |
| Trait default bodies | `Name :: trait { fn :: () => T { … }; }` | **Implemented** — inherit defaults in typeck/codegen ([V0-063](../language-v0-completion-roadmap.md#v0-063--trait-default-bodies)) | — |
| `break expr` | `break` , [ expr ] | Parse | Loop-value / labeled break semantics TBD |
| `Option` / `Result` types | Type expressions | **Implemented** with `#import std::core::…` or prelude ([V0-041](../language-v0.md#v0-041--core-std-types-as-ordinary-generic-enums), [V0-044](../language-v0.md#v0-044--prelude-minimal)) | — |
| `Some` / `None` / `Ok` / `Err` | Expr / patterns | **Implemented** with `#import std::core::…` or prelude ([V0-042](../language-v0.md#v0-042--std-constructors-and--sugar), [V0-044](../language-v0.md#v0-044--prelude-minimal)) | — |
| `expr?` | Postfix `?` | **Implemented** — identical `Result`/`Option` ([V0-042](../language-v0.md#v0-042--std-constructors-and--sugar)); `From` error conversion ([V0-059](../language-v0.md#v0-059--with-from-error-conversion)) | — |
| `From` / `Into` / `TryFrom` trait calls | Method / associated fn syntax | **Implemented** — `Target::from(x)`, `t::from(x)` at mono sites ([V0-058](../language-v0.md#v0-058--conversion-traits-from--into-in-std)); `From` at `?` ([V0-059](../language-v0.md#v0-059--with-from-error-conversion)) | — |
| Block-scoped `#import` | `import_directive` in `block_item` | **Implemented** — scoped name intro per [modules.md](modules.md#scoped-imports-mvp) | — |
| `#[cfg(...)]` | `attribute` on items | **Implemented** — strip before resolve ([V0-039](../language-v0.md#v0-039--item-attributes-and-conditional-compilation)) | `all`/`any`, `#![cfg]` |
| `#[deprecated(...)]` | `attribute` on items | **Implemented** — warning at use sites | Cross-crate via `.pxi` |
| `#[allow(...)]` / `#[must_use]` | `attribute` on items | **Implemented** — lint suppression / discard warning | `#[deny]` / `#[forbid]` |
| `#[stable(...)]` / `#[since(...)]` | — | Not in grammar v1 | API versioning metadata |
| Heap `alloc_bytes` / `dealloc_bytes` | `#import std::core::alloc::{alloc_bytes, dealloc_bytes}` | **Implemented** — compiler lowers to `ALLOC` / `FREE` opcodes inside `unsafe` ([V0-030](../language-v0.md#v0-030--heap-allocation-intrinsic), [V0-065](../language-v0-completion-roadmap.md#v0-065--heap-deallocation-dealloc_bytes--free)) | — |

---

## Not in grammar yet (design TBD)

| Feature | Why deferred | Notes |
|---------|--------------|-------|
| Module namespace import value | Needs module ref type + fn pointers | `const math = #import utils::math;` then `math.add` — Tier 2 in [modules.md](modules.md#import-evolution-phased); after [V0-053](../language-v0.md#v0-053--function-pointers-and-indirect-calls) |
| Qualified paths without `#import` | Path resolution in expr/type position | e.g. `utils::math::add(1, 2)` — Tier 3 in [modules.md](modules.md#import-evolution-phased) |
| Associated types with bounds/defaults | Needs richer grammar than `type Item;` | Example target: `type IntoIter: Iterator<Item = Self::Item>;` |
| Schedulable I/O types | Call-site syntax not locked | See [runtime-transparency.md](runtime-transparency.md); no `File.read` in MVP |
| Keyword reservation policy | Lexer implementation detail | Reserve words from [grammer.md](../grammer.md); reject as user identifiers |
| Single-element tuples `(T,)` | No MVP example requires them | Add to grammar if needed |
| Labeled `break` / `continue` | Not in overview docs | Add when loop labels are designed |
| `#actor` / `#supervise` | Post-MVP compile directives | Actor contract metadata |

---

## MVP grammar that is fully specified

These are in [grammar.ebnf](../grammar.ebnf) and intended for full MVP pipeline support:

- `Name :: struct` / `Name :: enum` / `type Alias = T` (tuple struct `Name :: struct(T, …)` — [V0-057](../language-v0.md#v0-057--opaque--newtype-wrappers))
- `Name :: trait` / `Type :: impl` / `Type :: impl :: Trait`
- Functions `name :: (params) => T { }`, top-level and block `const` / `var`
- Borrow types `&T`, `&mut T`; explicit casts `expr as Type`
- Module paths with PascalCase segments; file-level `#import`; block-scoped `#import` ([V0-014](language-v0.md#v0-014--block-scoped-import-mvp-modules))
- `match`, `if const` / `if var`, control flow (MVP); `Option` / `Result` / `?` parse-only until std
- Unit enum patterns (`Eof => …`); struct/tuple enum patterns

See [mvp.md](../mvp.md) for milestone scope.

---

## Related documents

| Topic | Document |
|-------|----------|
| Formal grammar | [grammar.ebnf](../grammar.ebnf) |
| Human-readable syntax | [grammer.md](../grammer.md) |
| Traits and impl | [traits.md](traits.md) |
| MVP boundary | [mvp.md](../mvp.md) |
