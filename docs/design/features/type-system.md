# Type system

How Phoenix divides compiler-known types from library-defined behavior, and the deterministic type rules for MVP.

---

## Layers and defaults

| Layer | Purpose | Examples |
|---|---|---|
| Core language types | Compiler-known value/type forms | numeric primitives, `bool`, pointers, **Arrays** `[T; N]`, slices, tuples, `()`, `Name :: struct`, `Name :: enum` (see [Phased: Option and Result](#phased-option-and-result-language--std)) |
| VM runtime primitives | Scheduler-owned execution machinery (not user-declared types) | execution context, schedulable-I/O markers in std signatures, mailbox registration (post-MVP) |
| Compile-time directives | Compiler behavior controls | `#import`, `#inline`, `#cold`, `#unsafe` |
| Runtime directives | Opt-in explicit actor/message operations | `@spawn`, `@send`, `@receive`, `@reply` (post-MVP) |
| Standard library | APIs built on language + runtime primitives | `File.read`, **DynamicArray**, formatting, traits |
| Sugar | Surface syntax lowered by compiler | `if const` / `if var`, ranges, `for`; `?` when std `Option`/`Result` exist (post-MVP) |

Notes:

- There is no primitive **owned** `string` type in MVP; owned growable text lives in std (`String` over `Alloc`). Core text is the **`str` UTF-8 view** (see below).
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
- **Arrays** (fixed-size contiguous sequences): `[T; N]` — user-facing name **Array**; syntax unchanged
- Slices/views: `[T]` — `(ptr, len)` over Array storage or (post–V0-062) heap blocks
- Text view: `str` (UTF-8 `(ptr, len)`; Copyable fat pointer, same convention as `[T]` slices)
- Tuples: `(T1, T2, ...)`
- Unit: `()`

### Runtime storage (MVP VM)

Each numeric primitive occupies **its declared width** in constants, local slots, and on the operand stack. There is no shared `i64` lane: `s32` values are 4-byte cells, `s128`/`u128` are 16-byte cells, and binary operators require **identical** primitive kinds unless an explicit `expr as Type` cast appears in source.

**Operator availability (MVP VM):**

| Operator | Signed ints | Unsigned ints | Floats |
|---|---|---|---|
| `+` `-` `*` `/` | yes (wrapping / truncating div) | yes (wrapping / truncating div) | yes (IEEE 754) |
| `%` | yes | yes | yes (IEEE truncated remainder) |
| `**` | **no (v0)** | **no (v0)** | **no (v0)** |
| Comparisons | yes | yes (unsigned order) | yes (IEEE; NaN rules in PHX-052) |

Width-accurate execution through `u128` is normative; see [wide-integers.md](wide-integers.md).

`bool` is a 1-byte cell, not an integer alias. Raw pointers and borrow references are `u64` addresses at runtime; MVP does not enforce borrow exclusivity (see [ownership.md](ownership.md)).

UTF-8 string literals `"…"` have type `str` and lower to a `(ptr, len)` view over module constant-pool rodata (no heap allocation).

Byte string literals `b"…"` have type `[u8; N]` and lower to an **Array** of `u8` elements.

**Arrays** `[T; N]` may be explicitly cast to slices `[T]`; slice values are `(ptr, len)` views over existing Array storage (no heap allocation in MVP).

**Heap slices (V0-062):** Slices may also view VM heap bytes when constructed inside `unsafe` via `#import std::core::slice::slice_from_raw_parts`. Signature: `slice_from_raw_parts :: <T>(ptr: *mut T, len: u32) => [T]`. Element type `T` must be a numeric primitive or `bool` in V0-062. Caller contract: `ptr` must address at least `len * size_of(T)` valid bytes (typically from `alloc_bytes`); the compiler does not track allocation extent. There is no Tier A cast `*mut T as [T]` — length is not expressible in a unary cast.

### Sequence naming (language vs std)

| Name | Form | Role |
|---|---|---|
| **Array** | `[T; N]` | Language primitive — fixed-size contiguous sequence |
| **Slice** | `[T]` | View `(ptr, len)` over Array, rodata, or heap storage |
| **DynamicArray** | `std::collections::DynamicArray<T>` (Std v0) | Growable contiguous sequence over heap allocation — **not** called `Vec` or `vector` in Phoenix |

Do not use Rust’s `Vec` naming in user-facing docs or std APIs. Implementation may still use growable buffers internally; the public std type is **`DynamicArray`**.

**Text vs binary casts (MVP):**

- `expr as [u8]` when `expr` has type `str` — safe projection to a byte slice view.
- `expr as str` when `expr` has type `[u8; N]` — allowed only when the array bytes are valid UTF-8 at compile time (e.g. a `b"…"` literal whose contents are UTF-8). Runtime validation belongs in std once `Result` exists.

`str` values are **Copyable** (bitwise copy of the fat pointer). Indexing and equality on `str` are std/trait concerns, not compiler builtins.

---

## Explicit cast tiers

Phoenix uses a single conversion operator: postfix **`expr as Type`**. Casts are always explicit in source; there is no implicit widening at call sites, assignments, returns, or in binary operators.

### Tier A — MVP (implemented)

| Conversion | Semantics |
|---|---|
| Numeric primitive ↔ numeric primitive (ints, uints, floats; cross-width and signed/unsigned) | Truncating/wrapping; **`bool` excluded** |
| `[T; N] as [T]` | Slice view over Array storage (no copy) |
| `str as [u8]` | Byte slice view over the same rodata / storage |
| `[u8; N] as str` | Allowed when UTF-8 is provable at compile time: inline `b"…"` literal **or** a **`const`** binding initialized directly from such a literal; lowers to rodata (`MakeStr`) |
| Single-field tuple struct ↔ inner field type | **V0-057** — repr-identical no-op: `500 as Millimeters`, `m as s32` when `Millimeters :: struct(s32)` ([opaque newtypes](#type-aliases-vs-opaque-newtypes-phased)) |

Examples:

```phoenix
const wide: s64 = 100 as s64;
const f: f32 = n as f32;
const sl: [u8] = arr as [u8];
const s: str = b"hi" as str;
const s2: str = arr as str;  // when `const arr = b"hi";`
const bytes: [u8] = msg as [u8];  // when `msg: str`
```

### Tier B — post-MVP / std

| Conversion | Status | Mechanism |
|---|---|---|
| `[u8; N] as str` with **runtime** UTF-8 validation | not implemented | std `TryFrom` / `Result`-returning API ([V0-058](../language-v0.md#v0-058--conversion-traits-from--into-in-std)) |
| `#unsafe` pointer / reinterpret casts | not implemented | `#unsafe` + documented ABI |
| User-defined infallible conversions | planned ([V0-058](../language-v0.md#v0-058--conversion-traits-from--into-in-std)) | `From` / `Into` traits ([traits.md](traits.md#conversion-traits-from--into)) |
| Fallible conversions | planned ([V0-058](../language-v0.md#v0-058--conversion-traits-from--into-in-std)) | `TryFrom` / `TryInto` → `Result` |
| Error type conversion at `?` sites | planned ([V0-059](../language-v0.md#v0-059--with-from-error-conversion)) | `From<E_in>` for `E_out` — not `as` ([error-handling.md](error-handling.md#error-conversion-from--into--v0-058)) |
| Contextual literal typing (e.g. `const x: f32 = 1` without `as`) | not implemented | optional DX only; not implicit call coercion |

### Tier C — forbidden via `as`

- `bool` ↔ numeric
- Struct / enum / layout punning (except identity `as SameType` and single-field tuple struct ↔ inner type — Tier A, V0-057)
- **Error or domain enum conversion** — use `From` / `TryFrom`, never `as`
- Casts that silently allocate (e.g. `str` → owned std `String`)
- Implicit numeric widening anywhere (calls, assignment, operators)

See also [grammer.md](../grammer.md#explicit-casts) for surface syntax and precedence.

---

## Type aliases vs opaque newtypes (phased)

| Form | Status | Semantics |
|---|---|---|
| `type Alias = T` | **MVP (shipped)** | **Transparent** alias — `Alias` and `T` unify for assignability, operators, and pattern matching after alias expansion. |
| Tuple struct `Name :: struct(T, …)` | **V0-057 (shipped)** | **Distinct nominal** wrapper — Rust-style `struct Millimeters(u32)`; not interchangeable with field types without ctor, `.N`, method, or explicit `as` (single-field). |
| Opaque / newtype wrapper | **V0-057** | Same as **tuple struct** (no separate `newtype` keyword). |

**Transparent aliases today:** recursive aliases are rejected; generic aliases (`type Pair<t> = (t, t);`) monomorphize like other generic declarations.

**Tuple struct as opaque wrapper (V0-057):**

- Declaration: `Millimeters :: struct(u32);` or multi-field `Point :: struct(s32, s32);` — not `type Alias = T`.
- Construction: tuple struct **constructor call** `Millimeters(500)` (not struct literal `{ … }`).
- Access: tuple fields `value.0`, `value.1`, … or inherent impl methods; single-field may also use `expr as Inner` / `inner as Wrapper`.
- Assignability: `Millimeters` and `u32` do **not** unify; `const x: u32 = length` is an error without `.0` / method / cast.
- Runtime repr: same as a struct with anonymous fields (zero-cost for primitive fields).
- `Copyable` / move semantics follow field types; `#derive(Copyable, PartialEq, Debug)` supported on tuple structs (non-generic).
- Generics: `Pair :: struct<t>(t, t);` monomorphizes like record structs.

**Accepted example (V0-057):**

```phoenix
#[derive(PartialEq)]
Millimeters :: struct(s32);

Millimeters :: impl {
  as_s32 :: (self: &Self) => s32 { self.0 }
};

main :: () => {
  const length: Millimeters = Millimeters(500);
  accept(length);
  const raw: s32 = length.as_s32();
  const _ = raw;
};

accept :: (m: Millimeters) => () { const _ = m; };
```

**Rejected:** `const x: s32 = length` (no `.0` / method / cast); `f(length)` when `f :: (x: s32) => ()`.

**Use cases:** newtyped IDs (`UserId`, `SessionId`), units (`Meters`, `Seconds`), small product types without field names. Record structs `{ x: T }` remain named-field types.

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

## Generics strategy (monomorphization)

Phoenix uses **compile-time monomorphization** as the **permanent** generics strategy — not an MVP-only stopgap. Each explicit instantiation site produces concrete layouts and specialized definitions before IR lowering. There is no runtime type erasure and no global inference search.

| Rule | Behavior |
|---|---|
| Syntax | Explicit `:: <t1, …>` (or `::<t1, …>`) at instantiation sites; **local inference** may omit args when constraints are unambiguous |
| Function calls | `id(1)` infers `s32`, or `id :: <s32> (1)`; `:: <…>` before `(` in postfix chains |
| Struct literals | `Box :: <s32> { v: 1 }` or `Box::<s32> { v: 1 }` when the struct template has generic parameters (field inference from literals is **not** in v1) |
| Enum constructors | `Some(1)` infers payload type, or `Some :: <s32> (1)` |
| Type annotations | `Pair<s32>`, `const x: Box<s32>`, generic type aliases with explicit args |
| Impl methods | `v.get()` and `v.id(2)` on generic impls; explicit `:: <…>` on methods when needed |
| Specialization | Each distinct `(template, args…)` gets mangled symbols such as `id$s32` and concrete [`ProgramLayout`](../../../source/phx-compiler/src/typeck/layout.rs) entries for lowering |
| Trait bounds | Checked when concrete type arguments are known (monomorphization / type instantiation) |
| Trait dispatch | Static (monomorphized) only; `dyn Trait` reserved for explicit runtime polymorphism |

Type parameters in templates are checked once; the monomorphization pass validates trait bounds, re-checks specialized bodies where needed, and emits substituted struct/enum layouts for each collected type instantiation.

**In scope today:** local call-site inference for generic functions and enum ctors; trait-bound enforcement at instantiation (`Copyable` bootstrap + user `Type :: impl :: Trait`); generic inherent impl methods with mono + inference; monomorphized function bodies and struct/enum/alias use sites.

### Local inference (v1)

Call sites may omit `:: <…>` when argument types constrain all generic parameters:

- **In:** generic function calls (`id(1)`), enum constructor calls (`Some(1)`).
- **Out (v1):** struct-literal field inference (`Box { v: 1 }` without `::<s32>`), cross-function constraint propagation, return-only inference with zero arguments.

Inference uses fresh `Ty::Var` nodes and local unification at the call site only (rule 10). Explicit `:: <…>` always takes precedence.

### `.pxi` export mangling (V0-024)

Cross-crate linking uses **stable `export_id`** values in dependency `.pxi` files (session `DefId` ≠ link id). The build driver:

1. Collects cross-crate monomorphization requests after consumer type-check.
2. Rebuilds affected path dependencies with an injected worklist when mangled exports are missing.
3. Writes mangled fn exports (e.g. `sort$s32`) with concrete signatures and `function_id` for link.

Importers keep using unmangled template names in `#import`; explicit `:: <T>` at the call site selects the mangled export at link time. See [pxi-format.md](pxi-format.md#implemented-pxi-mangling-for-generics-v0-024).

### Deferred: `dyn Trait`

Runtime trait objects need fat-pointer layout, vtables, object-safety rules, and `IndirectCall` through vtable slots — a large VM + typeck surface. Phoenix prioritizes **static mono** + **fn pointers for C FFI** ([Callable values: four layers](#callable-values-four-layers)); `dyn Trait` is in-language dynamism, not FFI.

**Trigger to implement:** plugin registries, `DynamicArray<dyn Draw>`, or trait-returning factories without monomorphization explosion.

When a generic function accepts a comparator or callback, monomorphization specializes the callee (`sort :: <s32> (…)`) at compile time; the callback argument is a **concrete function pointer type** (`:: (s32, s32) => bool`), not an erased generic fn value. See [Callable values: four layers](#callable-values-four-layers).

---

## Callable values: four layers

Phoenix is **no-GC**. Callable **values** must not behave like struct-sized owned payloads that move on every pass. The design splits **static calls**, **function pointers**, **closures**, and **`dyn` trait** dynamism into separate layers. Only the first is MVP-complete today; the rest are documented now so FFI and generics work toward the same target.

### Core principle

Do **not** pass functions “by value” like structs. Callable values should be **pointer-sized** and **Copyable** (bitwise copy of an address), not big owned or move-only values.

### Layer 1 — Static dispatch (default; MVP+ today)

Top-level and monomorphized functions use direct call lowering:

```phoenix
add :: (a: s32, b: s32) => s32 { a + b };
add(1, 2);
```

| Property | Behavior |
|---|---|
| Resolution | Callee name → compile-time `DefId` |
| Bytecode | PHX0 **`Call`** opcode with `function_id` operand (**implemented**) |
| Cost | No pointer, no move, no indirect dispatch |
| Status | Default for ordinary calls; remains the preferred path |

### Layer 2 — Function pointers (post-MVP / FFI phase; design now)

Callable **values** (as opposed to callee names in call syntax) are pointer-sized and Copyable.

| Property | Target behavior |
|---|---|
| Size | Fixed for C ABI — `usize` or platform code-pointer width |
| Copyability | Copyable (bitwise copy of address) |
| Syntax (conceptual) | `type Comparator = :: (s32, s32) => bool` as a **value type** — fn pointer — distinct from using `Ty::Fn` only in signatures today |
| Bytecode | Planned PHX0 **`IndirectCall`** opcode (not implemented; see [vm-linear.md](vm-linear.md) optional `CALL_INDIRECT`) |
| C interop | `extern "C"` maps cleanly to C function pointers |
| Generics tie-in | `sort::<s32>` monomorphizes statically; comparator passed as concrete `:: (s32, s32) => bool` fn pointer |

Function pointer types describe **code addresses**, not captured environments.

### Layer 3 — Closures (separate; deferred)

Lambda syntax `(params) => expr | block` is scaffolded in the parser and resolver ([resolver.md](resolver.md), [grammar-deferred.md](grammar-deferred.md)) but is **not** a function pointer.

| Property | Target behavior |
|---|---|
| Representation | Fat pointer (code pointer + captures) |
| Distinction | Not interchangeable with C-style fn pointers |
| Status | Post-MVP; typeck reports `UnsupportedFeature` today |

### Layer 4 — `dyn Trait` (alternative dynamism)

In-language polymorphism via vtables is a separate path from fn pointers.

| Use case | Preferred mechanism |
|---|---|
| C callbacks / `extern "C"` | Function pointers (Layer 2) |
| In-language “any `Ord` comparator” | `dyn Trait` may suffice without fn pointers |
| Status | Post-MVP; reserved alongside explicit fn pointers |

### Current implementation status

| Item | Today |
|---|---|
| `Ty::Fn` in type checker / parser | Function types in signatures and **fn pointer values** |
| First-class fn **values** | **Implemented (V0-053)** — Copyable, pointer-sized; static `Call` when callee known |
| `Ty::Fn` Copyability | **Copyable** (bitwise fn address) |
| `IndirectCall` / `CALL_INDIRECT` | **Implemented (V0-053)** |
| `extern "C"` | **Implemented (V0-053)** — calls require `unsafe`; types via `std::ffi` |

Do not treat a function name as a move-only non-copyable value blob; that shape is a bug relative to this design.

### Layered recommendations

| Layer | Now (MVP+) | Next (FFI) | Later |
|---|---|---|---|
| Static calls | `Call` opcode, static `DefId` | same | same |
| Fn pointer values | Copyable, `MakeFnPtr`, `CallIndirect` | same | — |
| C ABI | `extern "C"` + `std::ffi` ([ffi.md](ffi.md)) | dynamic link | native export |
| Avoid | fn name as move-only non-copyable value | — | — |
| Closures / `dyn Trait` | deferred | — | fat pointers / vtables |

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

**Already aligned:** `Clone` and phased `Copyable` live in std ([traits.md](traits.md), [ownership.md](ownership.md)); no primitive owned `string` (std `String` only); core `str` view is a language primitive; collections and I/O are post-MVP ([mvp.md](../mvp.md)).

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
| `if const Pat = expr { ... }` / `if var Pat = expr { ... }` | single-pattern branch (`if let` style) | Included |
| `expr?` | early return from std `Result`/`Option`; `From` on mismatched `E` ([V0-059](../language-v0.md#v0-059--with-from-error-conversion)) | V0-042 shipped (identical types); V0-059 planned |
| `for x in y` | iterator-protocol lowering | Parseable; richer iterator semantics post-MVP |
| `0..n`, `0..=n` | range values | Parseable; std range behavior post-MVP |
