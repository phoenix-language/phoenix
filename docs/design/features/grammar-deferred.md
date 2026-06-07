# Deferred grammar and semantics

This document tracks Phoenix syntax that is **parsed in [grammar.ebnf](../grammar.ebnf)** but not fully implemented in MVP, and syntax that is **not yet in the grammar** because the design is incomplete.

Use this when implementing the compiler: parse vs type-check vs codegen boundaries.

---

## Parsed now; semantics or codegen deferred

| Feature | Grammar | MVP compiler behavior | Deferred work |
|---------|---------|----------------------|---------------|
| `#derive(...)` | On functions, structs, enums, traits | Parse; no codegen | Emit trait impls from derive list |
| `@spawn` / `@send` / `@receive` / `@reply` | Expression-level | Parse; reject or warn at typeck | Post-MVP actor runtime |
| `for x in y` | Statement | Parse; limited or no lowering | Iterator trait desugaring |
| `0..n` / `0..=n` | Expression (`range_expr`) | Parse | Std range types and iteration |
| `lambda_expr` | `(params) => expr \| block` | Parse | Closure typing, capture, lowering TBD |
| Trait default bodies | `Name :: trait { fn :: () => T { … }; }` | Parse | Inherit defaults in typeck/codegen |
| `break expr` | `break` , [ expr ] | Parse | Loop-value / labeled break semantics TBD |
| `Option` / `Result` types | Type expressions | Parse; typeck rejects | Std generic enums + prelude |
| `Some` / `None` / `Ok` / `Err` | Expr / patterns | Parse; typeck rejects | Std enum constructors |
| `expr?` | Postfix `?` | Parse; typeck rejects | Sugar over std `Option`/`Result` |
| Block-scoped `#import` | `import_directive` in `block_item` | **Implemented** — scoped name intro per [modules.md](modules.md#scoped-imports-mvp) | — |
| `#[cfg(...)]` | `attribute` on items | **Implemented** — strip before resolve ([V0-039](../language-v0.md#v0-039--item-attributes-and-conditional-compilation)) | `all`/`any`, `#![cfg]` |
| `#[deprecated(...)]` | `attribute` on items | **Implemented** — warning at use sites | Cross-crate via `.pxi` |
| `#[allow(...)]` / `#[must_use]` | `attribute` on items | **Implemented** — lint suppression / discard warning | `#[deny]` / `#[forbid]` |
| `#[stable(...)]` / `#[since(...)]` | — | Not in grammar v1 | API versioning metadata |

---

## Not in grammar yet (design TBD)

| Feature | Why deferred | Notes |
|---------|--------------|-------|
| Module namespace import value | Needs module ref type + fn pointers | `const math = #import utils::math;` then `math.add` — Tier 2 in [modules.md](modules.md#import-evolution-phased); after [V0-053](../language-v0.md#v0-053--function-pointers-and-indirect-calls) |
| Qualified paths without `#import` | Path resolution in expr/type position | e.g. `utils::math::add(1, 2)` — Tier 3 in [modules.md](modules.md#import-evolution-phased) |
| Heap `ALLOC` surface syntax | MVP lists a runtime intrinsic; no canonical spelling | Candidate when std exists: `core::alloc::alloc_bytes(size: u32) => *mut u8` lowering to `ALLOC` opcode |
| Owned growable `String` | Core ships **`str`** view only; no primitive owned string | Post-`str` milestone: std `String` struct over `Alloc` + `Clone`; see [type-system.md](type-system.md) |
| Associated types with bounds/defaults | Needs richer grammar than `type Item;` | Example target: `type IntoIter: Iterator<Item = Self::Item>;` |
| Schedulable I/O types | Call-site syntax not locked | See [runtime-transparency.md](runtime-transparency.md); no `File.read` in MVP |
| Keyword reservation policy | Lexer implementation detail | Reserve words from [grammer.md](../grammer.md); reject as user identifiers |
| Single-element tuples `(T,)` | No MVP example requires them | Add to grammar if needed |
| Labeled `break` / `continue` | Not in overview docs | Add when loop labels are designed |
| `#actor` / `#supervise` | Post-MVP compile directives | Actor contract metadata |

---

## MVP grammar that is fully specified

These are in [grammar.ebnf](../grammar.ebnf) and intended for full MVP pipeline support:

- `Name :: struct` / `Name :: enum` / `type Alias = T`
- `Name :: trait` / `Type :: impl` / `Type :: impl :: Trait`
- Functions `name :: (params) => T { }`, top-level and block `const` / `var`
- Borrow types `&T`, `&mut T`; explicit casts `expr as Type`
- Module paths with PascalCase segments; file-level `#import`; block-scoped `#import` ([V0-014](language-v0.md#v0-014--block-scoped-import-mvp-modules))
- `match`, `given`, control flow (MVP); `Option` / `Result` / `?` parse-only until std
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
