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
| Hashing (later) | `Hash` |

Float caveat:

- `f32`/`f64` should generally implement `PartialEq` and `PartialOrd`.
- They should not imply total-order `Eq`/`Ord` by default unless a separate total-order wrapper is used.

`#derive(...)` is a future feature and not required for MVP code generation.

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
  next :: (mut self) => Option<Self::Item>;
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

Default trait body codegen is deferred in MVP — see [grammar-deferred.md](grammar-deferred.md).

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

Iteration is not a primitive special-case. The language can lower `for` loops through iterator traits.

Conceptual std definition:

```
IntoIter :: trait
{
  type Item;
  type IntoIter;   // bounds/defaults deferred — see grammar-deferred.md
  into_iter :: (self) => Self::IntoIter;
}
```

Desugaring (conceptual):

```
for item in expr { body }
```

becomes something like:

```
{
  var __iter = expr.into_iter();
  loop
  {
    given Some(item) = __iter.next()
    {
      body
    } else { break; }
  };
}
```

A type can participate in `for` when it implements the iterator protocol traits.

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
| `Option<T>` / `Result<T, E>` | lifted impls when inner types satisfy bounds |

---

## Derive direction (future)

Compiler-generated impls are a planned feature:

```phoenix
#derive(Debug, Clone, Eq, PartialEq)
Point :: struct
{
  x: f32,
  y: f32,
}
```

Current status:

- Reserved as future functionality.
- Parser support may exist before semantic/codegen support.

---

## Trait impl scope and orphans

A `Type :: impl :: Trait` block should live where either the trait or the type is defined (orphan-rule family constraint) to prevent conflicting downstream implementations.
