# Ownership and borrowing

Phoenix manages memory through explicit value movement, borrows, pointers, and allocation primitives.

MVP focuses on single-process execution. Actor-boundary ownership rules and implicit scheduler contexts are post-MVP design notes.

---

## Ownership

Every value has exactly one owner. Assignment and passing by value **move** ownership unless the type is **Copyable** (see below).

```
const a1: [u8; 4] = [1u, 2u, 3u, 4u];
const a2 = a1;      // move unless Copyable semantics apply to this type

process :: (v: [u8; 4]) => { /* takes ownership */ };
process(a2);
```

After a move, the previous binding is invalid (use-after-move is a compile error).

---

## Copyable vs Clone

Phoenix splits implicit cheap duplication from explicit expensive duplication. This is **not** a copy of Rust’s surface API — the language uses the name **Copyable** for the implicit case; **Clone** lives in the standard library as a trait.

### Copyable (MVP bootstrap)

**MVP:** The compiler treats Copyable as a known property for primitives and eligible user types — no std import required. **Target:** `Copyable` is a std empty marker trait; the compiler still special-cases implicit bitwise copy on assign and pass-by-value (Rust `Copy` model). See [Phased: Copyable (language → std)](#phased-copyable-language--std) and [traits.md](traits.md#phased-copyable-language--std).

**Copyable** types are duplicated implicitly on assignment, pass-by-value, and `@send` when the message type is Copyable. Duplication is always a **bitwise copy** of the value representation (no user-defined logic, no heap duplication of owned buffers).

A type is Copyable only when:

- Every field is Copyable
- The type has no owning heap payload requiring custom drop behavior
- The type has no interior mutability that would make a bitwise copy unsafe

**Typically Copyable:** integer and float scalars, `bool`, `()`, tuples of Copyable fields, and enums with only Copyable payloads.

**Not Copyable:** owning heap handles, mutable shared handles, and resource wrappers with cleanup requirements.

Copyability can be compiler-known for primitives and inferred/derived for eligible user types.

```
const a: s32 = 1;
const b = a;        // both valid: bitwise copy
const id: u64 = 42u;
// example runtime send semantics are post-MVP
```

There is no `.copy()` method on Copyable types — copying is implicit.

### Clone (standard library trait)

**Clone** is defined in `std`, not as a language primitive. It provides **explicit** duplication that may allocate or run custom logic:

```
Clone :: trait
{
  clone :: (self: &Self) => Self;
}
```

Use `Clone` when you need a second owned value after the first was moved or when generic code must duplicate heap data:

```
const payload: [u8; 3] = [1u, 2u, 3u];
const again = payload.clone();
use(payload);
use(again);
```

**When to use which:**

| Need | Mechanism |
|------|-----------|
| Small scalar, enum of scalars | Copyable — no ceremony |
| Read without taking ownership | `&T` / `&mut T` borrow |
| Transfer ownership | Pass by value (move) |
| Keep binding and also send a duplicate | `value.clone()` with `T: Clone` |
| Large immutable payload, many passes | Prefer shared-handle library types |

Generic bounds: prefer **`T: Copyable`** when the algorithm only needs cheap duplicates; use **`T: Clone`** when the body calls `.clone()` or must duplicate non-Copyable types.

`Clone` is **not** required for `Option`, `Result`, or `@send` at the language level — only where your code or std explicitly needs duplication.

---

## Phased: Copyable (language → std)

**Target architecture (same path as [Option / Result](type-system.md#phased-option-and-result-language--std)):**

| Layer | What belongs there |
|---|---|
| **Language** | Move vs copy analysis, use-after-move, borrows; generic bounds written as `T: Copyable` |
| **Std** | `Copyable` empty marker trait (parallel to `Clone`); opt-in / derive for eligible user types |
| **Compiler** | Special-case: implicit bitwise copy when `T: Copyable` — no trait method dispatch |

**What stays implicit:** assignment and pass-by-value copy eligible values without calling a method (contrast with explicit `.clone()` via `Clone`).

**MVP exception (bootstrap):** With no std crate yet, Copyable is compiler-known so move/copy diagnostics and bounds like `T: Copyable` work without `#import`.

**Migration when std exists:**

1. Define `Copyable` in std as a public empty marker trait.
2. Optional prelude re-export alongside `Clone`, `Option`, `Result` ([type-system.md](type-system.md#phased-option-and-result-language--std)).
3. Keep implicit copy semantics in the compiler — analysis keyed off `T: Copyable`, not user-defined copy hooks.
4. Remove ad hoc language-only Copyable flags from the type checker in favor of ordinary trait bound checking plus compiler eligibility rules for derived/opt-in types.
5. User-facing diagnostics remain **Copyable**, not Rust’s `Copy` name.

**Already distinct:** `Clone` stays std-only for explicit duplication — see [Clone and Copyable](traits.md#clone-and-copyable).

---

## Passing parameters

**Do not pass everything by value.** Phoenix uses three modes intentionally:

| Mode | Syntax | Use when |
|------|--------|----------|
| Move | `fn(x: T)` | Caller transfers ownership; callee owns `x` |
| Shared borrow | `fn(x: &T)` | Read-only access; caller keeps ownership |
| Mutable borrow | `fn(x: &mut T)` | Exclusive mutation; caller keeps ownership |

Method receivers follow the same model: `self` moves (unless Copyable), `self: &Self` borrows, `self: &mut Self` mutably borrows.

Large owned values should use borrow for read, move for transfer, and `.clone()` only when duplication is required.

---

## Borrowing

Borrowing allows temporary access without transferring ownership.

```
len4 :: (s: &[u8; 4]) => u32 { 4u };

const msg: [u8; 4] = [10u, 11u, 12u, 13u];
len4(&msg);

update :: (s: &mut [u8; 4]) =>
{
  s[0] = 42u;
};
```

---

## Rules (compile time)

Within a single function, the borrow checker enforces:

- Either one `&mut` borrow **or** any number of `&` borrows — never both at once on the same value
- Borrows cannot outlive the owner
- No use-after-move

### Implicit scheduler context (post-MVP)

When the VM parks a context at a **typed schedulable-I/O call site** (not during pure sequential computation):

- local bindings remain owned by that context's stack/frame
- suspension does not expose cross-context borrows
- no mailbox or message move is involved

This is distinct from explicit `@send` ownership transfer between actors. See [runtime-transparency.md](runtime-transparency.md).

### Actor boundary rules (post-MVP, explicit path)

These extend the single-threaded rules for **global linear ownership** across actors:

- No cross-actor borrows
- Runtime send/reply move message ownership
- Mailbox receives produce owned values

These rules apply in safe code and are not weakened by `#unsafe`.

---

## Safe vs unsafe regions

Phoenix has two `#unsafe` forms:

| Form | Syntax | Scope |
|------|--------|-------|
| Unsafe function | `#unsafe name :: (…) => T { … }` | Entire body |
| Unsafe block | `#unsafe { … }` | Block inside a safe function |

### What stays enforced

In both unsafe forms:

- Type matching, generics, and trait bounds on calls
- `Option` / `Result` and enum typing where the checker requires it
- Function arity and return types
- Actor isolation (post-MVP) still enforced when actor runtime exists

### What relaxes

- Ownership and borrow rules on **raw pointers** and manual memory (aliasing, some use-after-move of locals the author takes responsibility for)
- Conflicts between shared and mutable access on **raw** paths only
- FFI coercions (e.g. `*u8` from buffers)

Unsafe is for intra-context low-level work — not for bypassing explicit actor isolation. Raw blocking syscalls via `#unsafe`/FFI can stall worker threads and bypass cooperative scheduling.

---

## Related documents

| Topic | Document |
|-------|----------|
| Message moves | [messages.md](messages.md) |
| Actor runtime design | [concurrency.md](concurrency.md) |
| `#unsafe` forms | [compiler-directives.md](compiler-directives.md) |
| VM move/borrow opcodes | [vm-linear.md](vm-linear.md) |
| std `Clone` / phased `Copyable` | [traits.md](traits.md#clone-and-copyable), [Phased: Copyable](traits.md#phased-copyable-language--std) |
| `Option` / `Result` bootstrap | [type-system.md](type-system.md#phased-option-and-result-language--std) |
