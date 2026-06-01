# Type system

How Phoenix divides compiler-known types from library-defined behavior, and the deterministic type rules for MVP.

---

## Layers and defaults

| Layer | Purpose | Examples |
|---|---|---|
| Core language types | Compiler-known value/type forms | numeric primitives, `bool`, pointers, arrays, slices, tuples, `()`, `Option`, `Result`, `Name :: struct`, `Name :: enum` |
| VM runtime primitives | Scheduler-owned execution machinery (not user-declared types) | execution context, schedulable-I/O markers in std signatures, mailbox registration (post-MVP) |
| Compile-time directives | Compiler behavior controls | `#import`, `#inline`, `#cold`, `#unsafe` |
| Runtime directives | Opt-in explicit actor/message operations | `@spawn`, `@send`, `@receive`, `@reply` (post-MVP) |
| Standard library | APIs built on language + runtime primitives | `File.read`, collections, formatting, traits |
| Sugar | Surface syntax lowered by compiler | `given`, `?`, ranges, `for` |

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
- Built-in generics: `Option<T>`, `Result<T, E>`

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
9. `?` is valid only when the enclosing function returns compatible `Result<_, _>` or `Option<_>`.
10. Operators in MVP are compiler-defined on primitive numeric/boolean types only.
11. Generic inference is local to call-site constraints and does not perform global search.

---

## Built-in generic types

Phoenix has no `null`.

```phoenix
Option<T>      // Some(T) | None
Result<T, E>   // Ok(T) | Err(E)
```

```phoenix
const id: Option<u32> = Some(7u);
const miss: Option<u32> = None;

const ok: Result<s32, ParseError> = Ok(1);
const err: Result<s32, ParseError> = Err(ParseError::BadInput);
```

---

## Unit and tuples

`()` is the unit type. Functions with omitted return type return `()`.

```phoenix
main :: () => { };

const pair: (s32, u8) = (1, 2u);
```

---

## Core vs std policy

- Core types are available in every module by default.
- Std traits and functions are not all auto-imported.
- A small future prelude may include only common traits.
- Most high-level behavior should live in std traits and `impl`s, not new compiler primitives.

---

## Syntactic sugar (MVP and beyond)

| Sugar | Lowering direction | MVP status |
|---|---|---|
| `given Pat = expr { ... }` | `match`-style single-pattern branch | Included |
| `expr?` | early return from `Result`/`Option` context | Included |
| `for x in y` | iterator-protocol lowering | Parseable; richer iterator semantics post-MVP |
| `0..n`, `0..=n` | range values | Parseable; std range behavior post-MVP |
