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

**Typically Copyable:** integer and float scalars, `bool`, `()`, tuples of Copyable fields, **tuple structs** whose fields are all Copyable ([V0-057](language-v0.md#v0-057--opaque--newtype-wrappers)), enums with only Copyable payloads, **raw pointers** (`*T`, `*mut T` — bitwise copy of the address), and function pointer values.

**Not Copyable:** borrow references (`&T`, `&mut T`), owning heap handles (wrapper types with `Drop`), mutable shared handles, and resource wrappers with cleanup requirements.

Copyability can be compiler-known for primitives and inferred/derived for eligible user types.

```
const a: s32 = 1;
const b = a;        // both valid: bitwise copy
const id: u64 = 42u;
// example runtime send semantics are post-MVP
```

There is no `.copy()` method on Copyable types — copying is implicit.

### Tuple struct moves (V0-057)

Tuple structs move and copy **field-wise** through the wrapper type — the struct name is nominal, but ownership of each anonymous field follows the same rules as a record struct with those field types. In MVP, field-wise rules determine **Copyable eligibility** and whole-value move behavior; per-field invalidation of a binding is not tracked until post-MVP (see [MVP: no partial moves](#mvp-no-partial-moves)).

```phoenix
Buffer :: struct([u8; 4]);

store :: (b: Buffer) => () { const _ = b; };

main :: () => {
  const a: Buffer = Buffer([1u, 2u, 3u, 4u]);
  const b = a;       // move: `a` invalid afterward (Buffer is not Copyable if a field is not)
  store(b);
};
```

When all fields are Copyable, `#[derive(Copyable)]` or compiler-known Copyable applies to the tuple struct as a whole. Use `.0`, `.1`, … or inherent impl methods to access inner values; implicit unwrap to inner types is rejected ([type-system.md](type-system.md#type-aliases-vs-opaque-newtypes-phased)).

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

## Drop (scope-end cleanup)

Types that own resources (heap buffers, handles, callbacks) implement **`Drop`** in std. The compiler calls `drop` automatically when an owned binding leaves scope.

```
Drop :: trait
{
  drop :: (self) => ();
}
```

| Rule | Behavior |
|---|---|
| Automatic drop | At block end, before `return`, and before `break` — for locals still **valid** (not moved) whose type implements `Drop` |
| Manual `.drop()` | Consumes `self` like any by-value method; later use of the binding is **use-after-move** (same diagnostic as a move) |
| Copyable | A type with a `Drop` impl cannot be Copyable — custom cleanup and bitwise copy conflict |
| Heap wrappers | Types wrapping `ALLOC` heap blocks must implement `Drop`; see [traits.md — Drop](traits.md#drop-resource-cleanup) and V0-030 |

**MVP limitation:** move/drop planning is flow-insensitive (same as use-after-move on conditional branches). If a binding is moved on one branch, drop glue is skipped at scope exit even on paths where the move did not run.

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

### MVP: move detection scope

The MVP compiler records a move only when ownership transfers through a **bare identifier** on the right-hand side of an assignment or `var` initializer, for example `var q = p;`. Expressions such as `var q = foo();` or `var q = make();` do **not** move out of a local binding yet, even when `foo` takes its parameter by value.

Block-scoped shadowing is respected: an inner `var x` does not affect move state for an outer `x` after the inner block ends.

**Conditional branches (`if` / `match`):** Each arm is checked starting from a snapshot of ownership at the branch entry. At the merge point, a binding is treated as **Moved** if it was moved on **any** arm (flow-insensitive join, consistent with drop planning below). Uses of a binding in one arm therefore do not see moves performed only in sibling arms; uses after the whole `if` or `match` expression see the joined state.

**Loops (`while` / `loop` / `for-in`):** The body is checked once from a pre-loop snapshot; loop-carried bindings (defined before the loop) that are moved anywhere in the body are treated as **Moved** at the loop head for back-edge checking. Any **read** use of such a binding anywhere in the loop body is a **use-after-move** (flow-insensitive, same family as branch join). Move sources (`var q = p`, by-value call arguments) are not counted as reads. Uses after the loop see the joined post-body state. This may reject some break-guarded single-iteration patterns until post-MVP path-sensitive analysis — see the drop-planning limitation below.

### MVP: no partial moves

Move tracking applies to **whole bindings**, not individual fields within a struct or enum payload.

- **Field access** (`.field`) is a read of the receiver; it does not move or partially invalidate the parent binding.
- **Pattern destructuring** (`Point { r }`, match struct/tuple patterns) introduces field bindings but does **not** move or partially invalidate the scrutinee local.
- **Field assignment** (`p.field = v`) assigns through the receiver; MVP does not model partial mutability after a field was moved out.
- Only **bare-identifier** whole-value transfer (above) marks a binding **Moved**; extracting a non-Copyable field via `.field` or pattern bind does not invalidate the parent until post-MVP path-sensitive analysis (same deferred family as branch/loop flow-insensitivity — see drop-planning limitation below).

  **PHX-025 / ROADMAP note:** rejecting field-extraction moves (`var x = s.non_copyable_field`) is **deferred** — v0 does not implement partial-move rejection or per-field invalidation; see [ROADMAP.md](../ROADMAP.md) Resolved Design Decisions #2.

Post-MVP ownership analysis will use path-sensitive last-use and move-through-call rules aligned with the full design above.

### MVP: returning borrows of locals

The MVP compiler rejects **returning** a `&T`, `&mut T`, slice view (`[T]`), or **`str`** view whose value is formed from a function-local binding (`var`, `const`, or `match` scrutinee temp). Examples that fail type-check: `return &x` when `x` is a local, `return arr as [u8]` when `arr` is a local **Array**, or `return arr as str` when `arr` is a local byte Array. Storing a borrow into another local (`const p = &x;`) or using it only inside the function body remains allowed. Full borrow checking (lifetime parameters, borrow exclusivity across branches) is post-MVP.

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

These rules apply in safe code and are not weakened by `unsafe`.

---

## Safe vs unsafe regions

Phoenix has two **`unsafe` keyword** forms:

| Form | Syntax | Scope |
|------|--------|-------|
| Unsafe function | `unsafe name :: (…) => T { … }` | Entire body |
| Unsafe block | `unsafe { … }` | Block inside a safe function |

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

### Extern calls

**`extern "C"` symbol calls require `unsafe`.** The compiler cannot verify C ABI or foreign behavior at compile time. Taking an extern fn as a fn pointer value is safe; **invoking** it is not.

Unsafe is for intra-context low-level work — not for bypassing explicit actor isolation. Raw blocking syscalls via `unsafe`/FFI can stall worker threads and bypass cooperative scheduling.

---

## Heap ownership (V0-065)

Phoenix has no GC. Heap bytes come from `#import std::core::alloc::alloc_bytes` (VM `ALLOC`) and are reclaimed with `dealloc_bytes` (VM `FREE`).

| Rule | Behavior |
|---|---|
| Pairing | Every `alloc_bytes(n)` must have exactly one matching `dealloc_bytes(ptr, n)` on all paths, or the block leaks until the VM run ends |
| `Drop` | Std owning wrappers (`Box`, buffers, growable collections) implement `Drop` to deallocate via `Allocator` (see [allocator.md](allocator.md)) |
| Raw pointers | Copyable address values; copying the pointer does not transfer deallocation responsibility |
| Compiler | No proof of pairing in V0-065; VM ledger catches double-free, size mismatch, and use-after-free at runtime |
| Compaction | `FREE` marks bytes dead (zeroed) but does not shrink the bump heap |

### Std allocator layering (V0-066)

Heap intrinsics remain in `std::core::alloc`. The `Allocator` trait in `std::core::memory::allocator` wraps them; **only** `VmHeapAllocator` calls `alloc_bytes` / `dealloc_bytes` directly. Application code and collections use `Global` (or a custom `Allocator`) instead of the intrinsics. Owning types such as `Box` and `DynamicArray` call `Allocator::dealloc` from `Drop`, not the intrinsics.

Full trait shape, `Layout`, orphan rules, and `Allocator` → `Drop` on `Box`: [allocator.md](allocator.md).

---

## Related documents

| Topic | Document |
|-------|----------|
| Message moves | [messages.md](messages.md) |
| Actor runtime design | [concurrency.md](concurrency.md) |
| `#unsafe` forms | [compiler-directives.md](compiler-directives.md) |
| VM move/borrow opcodes | [vm-linear.md](vm-linear.md) |
| Heap allocator trait | [allocator.md](allocator.md) |
| std `Clone` / phased `Copyable` | [traits.md](traits.md#clone-and-copyable), [Phased: Copyable](traits.md#phased-copyable-language--std) |
| `Option` / `Result` (std, post-MVP) | [type-system.md](type-system.md#phased-option-and-result-language--std) |
