# Type system

How Phoenix divides compiler-known types from library-defined behavior, and the deterministic type rules for MVP.

---

## Layers and defaults

| Layer | Purpose | Examples |
|---|---|---|
| Core language types | Compiler-known value/type forms | numeric primitives, `bool`, pointers, arrays, slices, tuples, `()`, `Name :: struct`, `Name :: enum` (see [Phased: Option and Result](#phased-option-and-result-language--std)) |
| VM runtime primitives | Scheduler-owned execution machinery (not user-declared types) | execution context, schedulable-I/O markers in std signatures, mailbox registration (post-MVP) |
| Compile-time directives | Compiler behavior controls | `#import`, `#inline`, `#cold`, `#unsafe` |
| Runtime directives | Opt-in explicit actor/message operations | `@spawn`, `@send`, `@receive`, `@reply` (post-MVP) |
| Standard library | APIs built on language + runtime primitives | `File.read`, collections, formatting, traits |
| Sugar | Surface syntax lowered by compiler | `given`, ranges, `for`; `?` when std `Option`/`Result` exist (post-MVP) |

Notes:

- `string` is not a core primitive in MVP.
- `ActorRef<T>` is not a primitive type; actor contracts are post-MVP trait-level design.
- **Scheduler context** is a VM runtime primitive: all user code (including `main`) runs under scheduler management in the full runtime vision.
- Safe std I/O is **type-visible schedulable I/O** at call sites (concrete syntax TBD); it lowers to schedulable VM operations. Raw blocking syscalls belong only behind `#unsafe`/FFI with documented worker-stall risk.
- See [runtime-transparency.md](runtime-transparency.md) for the call-site taxonomy.

---

## Language primitives vs runtime primitives

| Category | Owned by | User writes? | MVP |
|---|---|---|---|
| Language primitives | compiler/type checker | yes (types, values, functions) | yes |
| Runtime primitives | VM scheduler/I/O layer | surfaced via std/API types and `@…` directives, not invisible magic | documented; not implemented |
| Std library | Phoenix libraries | yes (calls like `File.read`) | post-MVP for I/O |

Language primitives answer: *what is the shape and meaning of data and control flow?*

Runtime primitives answer: *how does the VM schedule execution and park contexts on schedulable I/O without blocking workers?* Schedulability is reflected in types at call sites.

Explicit `@spawn` actors are an opt-in layer on top of runtime primitives, not a prerequisite for running `main` or doing I/O.

---

## MVP core primitive inventory

- Signed ints: `s8`, `s16`, `s32`, `s64`, `s128`
- Unsigned ints: `u8`, `u16`, `u32`, `u64`, `u128`
- Floats: `f32`, `f64`
- `bool`
- Raw pointers: `*T`, `*mut T`
- Borrow types: `&T`, `&mut T`
- Fixed arrays: `[T; N]`
- Slices/views: `[T]`
- Tuples: `(T1, T2, ...)`
- Unit: `()`
---

## Deterministic MVP type rules

1. Integer literals default to `s32`; with `u` suffix they default to `u32`.
2. Float literals default to `f32` unless explicitly suffixed (`f32`/`f64`) or context-constrained.
3. No implicit numeric casts (no widening or narrowing). Use explicit `expr as Type` (see [grammer.md](../grammer.md#explicit-casts)).
4. `const`/`var` local inference is allowed only when initializer type is unambiguous.
5. Function parameter types are always explicit.
6. Function return type may be omitted; omitted return type is `()`.
7. `if` and `match` are expressions; branch/arm result types must unify.
8. `return;` is valid only for `()` return functions.
9. Operators in MVP are compiler-defined on primitive numeric/boolean types only.
10. Generic inference is local to call-site constraints and does not perform global search.

---

## Phased: Option and Result (language → std)

Phoenix has no `null`. **Absence and failure are std concerns**, not compiler builtins in MVP.

| Layer | What belongs there |
|---|---|
| **Language** | `struct`, `enum`, `trait`, `impl`, generics, `match`, `if`, moves/`Copyable`, primitive types, explicit casts; optional **sugar** (`?`, `Some`/`None`/`Ok`/`Err` patterns) lowered against std-defined types once std ships |
| **Std** | `Option`, `Result`, `Clone`, `Copyable`, collections, text helpers, I/O, formatting — imported via `#import` / prelude |
| **Compiler (MVP)** | Knows primitives and user `enum`/`struct` only; **rejects** `Option`/`Result` types and std ctor/`?` syntax until std exists |

**Std shape (ordinary generic enums, not language primitives):**

```phoenix
pub Option :: enum<Type> {
  None,
  Some(Type),
}

pub Result :: enum<Ok, Err> {
  Ok(Ok),
  Err(Err),
}
```

**When std lands:** add prelude re-exports, type-check `Option`/`Result` like any other enum, wire `?` and ctor/pattern sugar to those definitions ([error-handling.md](error-handling.md)). Do not reintroduce `Ty::Option` / `Ty::Result` in the type checker.

**Grammar note:** [grammar.ebnf](../grammar.ebnf) may still parse `Option`, `Result`, `Some`, `None`, `Ok`, `Err`, and `?` for forward compatibility; MVP type-check reports them as post-MVP std features.

**Already aligned:** `Clone` and phased `Copyable` live in std ([traits.md](traits.md), [ownership.md](ownership.md)); no primitive `string`; collections and I/O are post-MVP ([mvp.md](../mvp.md)).

---

## Unit and tuples

`()` is the unit type. Functions with omitted return type return `()`.

```phoenix
main :: () => { };

const pair: (s32, u8) = (1, 2u);
```

---

## Core vs std policy

- Core types are available in every module by default (primitives, `()`, tuples, pointers, arrays, slices, and user `struct`/`enum` declarations).
- Std traits and functions are not all auto-imported.
- A small future prelude may re-export only common std items (e.g. `Option`, `Result`, core traits) — not the whole library.
- Most high-level behavior should live in std traits and `impl`s, not new compiler primitives.
- Do not add new compiler builtins for features that can be expressed as std `enum`/`trait`/`impl` once the std pipeline exists.

---

## Syntactic sugar (MVP and beyond)

| Sugar | Lowering direction | MVP status |
|---|---|---|
| `given Pat = expr { ... }` | `match`-style single-pattern branch | Included |
| `expr?` | early return from `Result`/`Option` context | Post-MVP (requires std types) |
| `for x in y` | iterator-protocol lowering | Parseable; richer iterator semantics post-MVP |
| `0..n`, `0..=n` | range values | Parseable; std range behavior post-MVP |
