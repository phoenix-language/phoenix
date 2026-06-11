# Traits and impl

`trait` and `impl` are core language features used for shared behavior, static dispatch, and generic constraints. Trait libraries are expected to live in `std`/core libraries; the syntax and type-checking are compiler responsibilities.

Declarations use the same `Name :: kind` form as structs and enums: `PartialEq :: trait { … }`, `Point :: impl :: PartialEq { … }`, `Point :: impl { … }`.

Surface syntax lives in [grammer.md](../grammer.md#trait-and-impl-syntax). This document covers semantics.

---

## Trait baseline roadmap (Rust-inspired, Phoenix-specific)

Phoenix adopts a Rust-like trait decomposition as the long-term direction, without copying Rust wholesale.

Primary baseline traits:

| Capability | Trait |
|---|---|
| Explicit duplication | `Clone` |
| Implicit copy semantics | `Copy` / `Copyable` mapping |
| Debug formatting | `Debug` |
| Equality | `PartialEq`, `Eq` |
| Ordering | `PartialOrd`, `Ord` |
| User-facing formatting | `Display` |
| Conversions | `From`, `Into` |
| Iteration | `Iterator` |
| Cleanup/drop hooks | `Drop` |
| Error values (`Result<T, E>`) | `Error` |
| Hashing (later) | `Hash` |

Float caveat:

- `f32`/`f64` should generally implement `PartialEq` and `PartialOrd`.
- They should not imply total-order `Eq`/`Ord` by default unless a separate total-order wrapper is used.

`#derive(...)` is a future feature and not required for MVP code generation.

Conversion traits (`From`, `Into`, `TryFrom`, `TryInto`) are required for ergonomic std error handling and for `?` with mismatched error types — see [error-handling.md](error-handling.md#error-conversion-from--into--v0-058).

---

## Conversion traits (`From` / `Into`)

Std-defined traits in `std::core::convert` ([V0-058](../language-v0.md#v0-058--conversion-traits-from--into-in-std)). **Not** compiler builtins; the type checker resolves impls at monomorphization sites like any other trait bound.

### Signatures (target)

```phoenix
pub From :: <Source> trait {
  from :: (value: Source) => Self;
}

pub Into :: <Target> trait {
  into :: (self) => Target;
}

pub TryFrom :: <Source> trait {
  try_from :: (value: Source) => Result<Self, Self::Error>;
  type Error;
}

pub TryInto :: <Target> trait {
  try_into :: (self) => Result<Target, Self::Error>;
  type Error;
}
```

### Semantics

| Rule | Behavior |
|---|---|
| Canonical direction | Implement **`From<Source>` for `Target`**; callers use `Target::from(x)` or `x.into()` when `Into` exists |
| Dispatch | Static only — monomorphized `Call` at each site; no vtables in Language v0 |
| vs `as` | `as` is for primitives and views ([type-system.md](type-system.md#explicit-cast-tiers)); **`as` never converts errors or user structs/enums** |
| vs `?` | When `E_in ≠ E_out`, `?` desugars to `From::from` on the `Err` payload ([error-handling.md](error-handling.md#the--operator-v0-042-error-conversion-v0-059)) |
| Fallible | `TryFrom` / `TryInto` return `Result`; use for parsing, bounds checks, runtime UTF-8 validation |
| Default bodies | Blanket `Into` from `From` is implemented in `std::core::convert` (V0-063); impl both or call `From::from` explicitly when overriding |

### Error-type impl guidance

- `Error` is a marker trait in `std::core::error` ([error-handling.md](error-handling.md#std-error-vocabulary--v0-060)); concrete error types are crate-local (or future subsystem modules).
- Implement `Type :: impl :: Error { }` on each error struct/enum used as `E` in `Result<T, E>`.
- Provide `From<LeafError> for AppError` in the crate that defines both types.
- Downstream crates must not add conflicting `From` impls for std types they do not own — orphan rule applies.

### Trait supertraits (deferred)

Rust's `core::error::Error` requires `Debug + Display` as supertraits. Phoenix v0 uses an **empty marker** `Error` trait plus documented convention until the compiler supports trait supertrait bounds (`Error: Debug + Display`). Same gap blocks `Error::source()` returning `dyn Error` — see [deferred `dyn Trait`](type-system.md#deferred-dyn-trait).

---


## What a trait is

A trait is a named set of required signatures (and optional defaults). It describes behavior, not data.

```
Debug :: trait
{
  fmt :: (self: &Self) => [u8];
}

PartialEq :: trait
{
  eq :: (self: &Self, other: &Self) => bool;
}
```

Traits may declare **associated types** — type slots filled in by each `impl`:

```
Iterator :: trait
{
  type Item;
  next :: (mut self: &mut Self) => Option<Self::Item>;
}
```

Rich associated-type bounds (`type IntoIter: Iterator<Item = Self::Item>`) are deferred — see [grammar-deferred.md](grammar-deferred.md).

The compiler treats a trait as a **constraint**: "`T` must provide these functions (and associated types) with these signatures."

---

## What `impl` does

An `impl` block attaches behavior to a concrete type. There are two forms.

**1. Trait impl** — "`Type` implements `Trait`"

```
Point :: impl :: PartialEq
{
  eq :: (self: &Self, other: &Self) => bool
  {
    self.x == other.x && self.y == other.y
  };
}
```

**2. Inherent impl** — methods that belong to the type itself, with no trait name

```
Point :: impl
{
  length_squared :: (self: &Self) => s32
  {
    self.x * self.x + self.y * self.y
  };
}
```

Both forms use the same `::` function syntax. The first parameter is the **receiver**:

| Receiver | Meaning |
|---|---|
| `self` | by value (move unless the type is Copyable) |
| `self: &Self` | shared borrow |
| `self: &mut Self` | exclusive mutable borrow |

`Self` inside an `impl` block aliases the concrete implementer type.

---

## Method call syntax

Functions defined in an `impl` block are called with **dot syntax** on a value of that type:

```
const p = Point { x: 3, y: 4 };
const d = p.length_squared(); // inherent method
const same = p.eq(&p);        // trait method
```

The compiler resolves methods statically in MVP.

Free functions still use the `name(args)` form:

```
sum(p.x, p.y);
```

---

## Default trait bodies

A trait may provide a default implementation. Types get it for free unless they override:

```
Zero :: trait
{
  zero :: () => Self
  {
    0
  };
}

s32 :: impl :: Zero { };
```

Default trait bodies use static dispatch — empty `Type :: impl :: Trait { }` inherits methods that have defaults; explicit impl methods override them entirely (no `super` in Language v0).

---

## Generics and trait bounds

Types, functions, structs, enums, and traits may be parameterized over type variables. **Trait bounds** on a type parameter tell the compiler what operations are allowed on that type inside the generic body.

```
max :: <T: PartialOrd + Copyable>(a: T, b: T) => T
{
  if a > b { a } else { b }
}
```

Trait bounds are compile-time contracts. If `T` does not satisfy the bound, generic instantiation fails at compile time.

**MVP:** bounds are parsed and **enforced at instantiation** (function and type monomorphization sites). `Copyable` is compiler-bootstrapped ([Phased: Copyable](#phased-copyable-language--std)); user traits require a matching `Type :: impl :: Trait` in the same crate. Monomorphization uses **static dispatch** only. `dyn Trait` is reserved for explicit runtime polymorphism ([type-system.md](type-system.md#deferred-dyn-trait)).

Multiple bounds use `+`, for example `<T: PartialOrd + Copyable>` or `<T: PartialEq + Clone>`.

---

## Clone and Copyable

**Copyable** is the Phoenix name for implicit bitwise copy on assign and pass-by-value (not Rust’s `Copy` in diagnostics). **MVP:** compiler-known for primitives and eligible types. **Target:** std empty marker trait with compiler special-casing — see [Phased: Copyable (language → std)](#phased-copyable-language--std) and [ownership.md](ownership.md#phased-copyable-language--std).

**Clone** is a **standard library** trait for explicit duplication (may allocate):

```
Clone :: trait
{
  clone :: (self: &Self) => Self;
}
```

Use `T: Clone` when explicit duplication is required. Use `T: Copyable` for implicit by-value copies.

---

## Drop (resource cleanup)

**Drop** is a **standard library** trait for custom cleanup when an owned value leaves scope. The compiler inserts **drop glue** at scope exit for locals whose type implements `Drop` (static dispatch to the type's `drop` method — no dedicated bytecode opcode).

```
Drop :: trait
{
  drop :: (self) => ();
}
```

| Rule | Behavior |
|---|---|
| Scope exit | Owned locals with a `Drop` impl are dropped in **reverse definition order** at block end, before `return`, and before `break` that exits enclosing scopes |
| Moved locals | Bindings already **moved** are not dropped again |
| Manual call | `x.drop()` is an ordinary trait method call; `self` is consumed — subsequent use of `x` is **use-after-move** |
| vs Copyable | Types with a `Drop` impl are **not Copyable**; explicit `Copyable` + `Drop` on the same type is a compile error |
| Dispatch | Static `Call` to the resolved `Drop::drop` impl at each drop site |

### Heap allocation (V0-030 / V0-065)

**Surface:**

- `#import std::core::alloc::alloc_bytes` — `(size: u32) => *mut u8`, callable only inside `unsafe`. Compiler lowers to VM `ALLOC` (pops runtime `size`, pushes heap offset as `*mut u8`).
- `#import std::core::alloc::dealloc_bytes` — `(ptr: *mut u8, size: u32) => ()`, callable only inside `unsafe`. Compiler lowers to VM `FREE` (pops `ptr` and `size`; VM verifies exact `(ptr, size)` ledger entry).

**Ownership:**

- `alloc_bytes` returns an **owned raw address**; there is no GC.
- Every `alloc_bytes(n)` must be paired with exactly one `dealloc_bytes(ptr, n)` on all paths (or the block leaks).
- Raw pointers (`*T`, `*mut T`) are **Copyable** (bitwise copy of the address); only one owner should call `dealloc_bytes`.
- Wrapper types around heap blocks **must** implement `Drop` that calls `dealloc_bytes` with the same `size` passed to `alloc_bytes`.
- VM rejects double-free and size mismatch at runtime; the compiler does not prove alloc/dealloc pairing.

### Flow-insensitive limitation (MVP)

Move tracking is flow-insensitive within a function: if a binding is moved on one branch, it is treated as moved at scope exit on all paths (drop may be skipped on a path where the move did not occur). Full CFG liveness for drop glue is post-V0-054.

---

## Phased: Copyable (language → std)

**Target architecture (aligned with [Option / Result](type-system.md#phased-option-and-result-language--std)):**

| Layer | What belongs there |
|---|---|
| **Language** | Syntax for trait bounds (`T: Copyable`); move/copy and use-after-move analysis |
| **Std** | `Copyable :: trait { }` — empty marker, like `Clone` but for implicit eligibility |
| **Compiler** | Recognizes `Copyable` impls and applies implicit bitwise copy without invoking trait methods |

**MVP exception (bootstrap):** `Copyable` behaves like today’s compiler-known marker so MVP move/copy rules and generic bounds work before std ships.

**Migration when std exists:**

1. Add `Copyable` to std (empty trait); document opt-in / derive for eligible structs and enums.
2. Prelude or `#import` exposes `Copyable` next to `Clone`.
3. Compiler retains intrinsic copy-on-assign/pass-by-value for `T: Copyable` (Rust `Copy` model).
4. Drop language-only Copyable type flags; use ordinary trait bound checking plus eligibility validation.
5. Keep **Copyable** as the user-facing name in errors and docs ([ownership.md](ownership.md#copyable-vs-clone)).

**Pairing with Clone:** `Clone` remains the only path for explicit `.clone()` duplication; types may implement both when bitwise copy is safe and explicit clone is still useful for generic APIs.

---

## How the compiler uses traits

Traits enable **checked generic code**:

1. Trait bounds on generics — `sort :: <T: PartialOrd>(items: &mut [T]) => ...` only type-checks when `T` satisfies bounds.
2. **Method resolution** — `x.method()` finds an inherent impl or a trait impl in scope.
3. **Exhaustive checking** — missing required trait items are compile errors, not runtime failures.

The MVP operator set is compiler-defined for primitive types. Trait-based operator overloading can expand later.

---

## Iteration

Iteration is not a primitive special-case. The language lowers `for` loops through iterator traits ([V0-055](../language-v0.md#v0-055--iterator-protocol-and-for-lowering)).

Std definitions (`std::core::iter`):

```
Iterator :: trait
{
  type Item;
  next :: (mut self: &mut Self) => Option<Self::Item>;
}

IntoIter :: trait
{
  type Item;
  type IntoIter;   // bounds/defaults deferred — see grammar-deferred.md
  into_iter :: (self) => Self::IntoIter;
}
```

`next` uses `mut self: &mut Self` so the iterator state can advance across calls without moving the owned iterator value each time ([ownership.md](ownership.md#passing-parameters)).

Desugaring (implemented):

```
for item in expr { body }
```

lowers to:

```
{
  var __iter = expr.into_iter();
  loop
  {
    if const Some(item) = (&mut __iter).next()
    {
      body
    } else { break; }
  };
}
```

A type participates in `for` when it implements `IntoIter` and the resulting `IntoIter` type implements `Iterator` with matching `Item` associated types.

**Reference std iterator (V0-055):** `Range { start, end }` with `RangeIter` — construct manually (`Range { start: 0, end: 3 }`); range *literals* (`0..n`) remain deferred ([grammar-deferred.md](grammar-deferred.md)).

---

## Primitive implementation guidance (future table)

Suggested baseline (design target, not MVP implementation guarantee):

| Type family | Expected baseline impls |
|---|---|
| Integers | `Copyable`, `Clone`, `Debug`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Display`, `From`/`Into` conversions |
| Floats | `Copyable`, `Clone`, `Debug`, `PartialEq`, `PartialOrd`, `Display` |
| `bool` | `Copyable`, `Clone`, `Debug`, `PartialEq`, `Eq`, `Ord` |
| Pointers | `Copyable`, `Clone`, `Debug`, `PartialEq`, `Eq` |
| Arrays `[T; N]` | trait impls when `T` satisfies corresponding bounds |
| Slices `[T]` | comparison/iteration traits via borrowed views |
| `DynamicArray<T>` (std) | growable buffer; `Clone`/`Drop` when element type and allocator support it |
| `Option<T>` / `Result<T, E>` | lifted impls when inner types satisfy bounds |

---

## Derive (V0-056)

`#derive(...)` and `#[derive(...)]` expand to trait impls at compile time (before name resolution). Supported traits: **`Copyable`**, **`PartialEq`**, **`Debug`** on record structs, **tuple structs** ([V0-057](../language-v0.md#v0-057--opaque--newtype-wrappers)), and enums. Traits must be in scope via prelude or `#import` (e.g. `std::core::cmp::PartialEq`).

```phoenix
#derive(PartialEq, Copyable)
Point :: struct {
  x: s32,
  y: s32,
}
```

Tuple structs ([V0-057](../language-v0.md#v0-057--opaque--newtype-wrappers)) use the same derive allowlist:

```phoenix
#[derive(PartialEq)]
Millimeters :: struct(s32);
```

| Trait | Rule |
|---|---|
| `Copyable` | Empty impl; type must have only Copyable fields and no `Drop` impl |
| `PartialEq` | Record struct: pairwise `==` on named fields; **tuple struct:** `self.0 == other.0 && …`; enum: variant tag + payload comparison |
| `Debug` | Placeholder `fmt` returning a 32-byte type-name buffer |

Generic types, `Clone`/`Eq`, and custom derives are out of scope for V0-056.

---

## Trait impl scope and orphans

A `Type :: impl :: Trait` block should live where either the trait or the type is defined (orphan-rule family constraint) to prevent conflicting downstream implementations.

Std example: `Allocator` and `Global :: impl :: Allocator` both live in `std::core::memory::allocator` — see [allocator.md](allocator.md).
