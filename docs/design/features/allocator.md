# Allocator trait (Std v0)

Status: Active — **V0-066**

**Authority:** Pluggable heap policy lives in the standard library, not in the compiler. The language exposes VM heap intrinsics (`alloc_bytes`, `dealloc_bytes`); std wraps them behind an `Allocator` trait. There is **no** `Ty::Allocator` or allocator opcode family in the compiler.

**Related:** [ownership.md](ownership.md) (heap pairing, `Drop`), [traits.md](traits.md) (orphan rule), [vm-linear.md](vm-linear.md) (`ALLOC` / `FREE`), [type-system.md](type-system.md) (`DynamicArray` naming).

---

## Goals

| Goal | Rationale |
|------|-----------|
| Single intrinsic boundary | Only `VmHeapAllocator` in std calls `alloc_bytes` / `dealloc_bytes` |
| Pluggable policy in std | Arenas, pools, and test fakes implement `Allocator` in Phoenix source |
| Default for collections | `Global` is the default backing allocator for `Box`, `DynamicArray`, and similar owning types |
| No compiler special cases | Trait dispatch and monomorphization only — same as `Iterator` or `Drop` |

---

## Layering

```text
Application / collections
        │
        ▼
  Box<T, A>  ──Drop──►  A.dealloc(ptr, layout)
  DynamicArray<T, A>     (Std v0 — not Language v0)
        │
        ▼
  Allocator trait  (std::core::memory::allocator)
        │
        ▼
  Global  ──wraps──►  VmHeapAllocator
        │
        ▼
  alloc_bytes / dealloc_bytes  (std::core::alloc — compiler intrinsics → ALLOC / FREE)
        │
        ▼
  VM bump heap + allocation ledger
```

### `Allocator` → `Drop` on `Box`

`Box<T, A>` (Std v0) stores:

- `ptr: *mut T` (or raw bytes + typed view),
- `layout: Layout`,
- `alloc: A` (or `A` held by reference / ZST handle).

Construction calls `A.alloc(layout)` (inside `unsafe` at the std boundary). On scope exit, `Drop for Box<T, A>` calls `A.dealloc(ptr, layout)` — **not** the intrinsics directly. Custom allocators therefore control both allocation and reclamation without changing VM opcodes.

`DynamicArray<T, A: Allocator = Global>` follows the same pattern: grow/reallocate via `Allocator`; `Drop` frees the backing store through the trait.

**Language v0 scope:** document this layering; implement `Allocator`, `Layout`, `Global`, and `VmHeapAllocator` only. `Box` and `DynamicArray` are subsequent Std v0 items.

---

## `Layout`

`Layout` describes a byte span to allocate or free. Std v0 shape:

```phoenix
Layout :: struct {
    size: u32,
    align: u32,
};
```

| Field | Meaning |
|-------|---------|
| `size` | Byte count passed to the VM (`ALLOC` / `FREE` operand) |
| `align` | Required alignment for the allocation (recorded for future VM / platform rules) |

**V0 VM contract:** the MVP heap honors `size` only; `align` must be `1` for VM-backed allocations until a stricter platform contract is specified. Std helpers may assert or default `align` to `1` when calling `VmHeapAllocator`.

---

## `Allocator` trait

Defined in `std::core::memory::allocator`:

```phoenix
pub Allocator :: unsafe trait {
    alloc :: (self: &mut Self, layout: Layout) => *mut u8;
    dealloc :: (self: &mut Self, ptr: *mut u8, layout: Layout) => ();
};
```

| Method | Contract |
|--------|----------|
| `alloc` | Returns a pointer to at least `layout.size` bytes, or a null pointer on failure (Std v0: VM bump heap does not report failure — document as future `Result`) |
| `dealloc` | Releases the block previously obtained with the **same** `layout.size`; must not be called on pointers not allocated through this allocator instance |

Methods take `&mut Self` so stateful allocators (arenas, pools) can update bookkeeping. `Global` / `VmHeapAllocator` are zero-sized; mutation is a no-op but keeps a uniform trait surface for generic collections.

**V0-066:** `Allocator` is an `unsafe trait` (Option B). `VmHeapAllocator :: unsafe impl :: Allocator` calls heap intrinsics in method bodies without per-method `unsafe` keywords. Application code must still use `unsafe { … }` when calling `alloc` / `dealloc` on a trait receiver.

**Not in the trait (Std v0):** `realloc`, `grow`, `shrink`, `allocate_zeroed`. Collections compose `alloc` + copy + `dealloc` until a later std revision adds optional extension traits.

---

## `VmHeapAllocator` and `Global`

| Type | Role |
|------|------|
| `VmHeapAllocator` | ZST; **only** std type whose implementation calls `alloc_bytes` / `dealloc_bytes` (inside `unsafe`) |
| `Global` | Public default name; **type alias** for `VmHeapAllocator` in V0-066 (wrapper struct with forwarding lives in a follow-up when field trait dispatch is stable) |

Application and library code should import `Global` (or a custom `Allocator`), not `std::core::alloc` intrinsics. Direct intrinsic use remains legal for low-level tests and compiler fixtures but is discouraged outside `std::core::memory`.

---

## Orphan rules

Same family constraint as [traits.md](traits.md#trait-impl-scope-and-orphans):

- `Allocator` is defined in `std::core::memory::allocator`.
- `Global :: impl :: Allocator` and `VmHeapAllocator :: impl :: Allocator` live in that module (trait + type co-located).
- Downstream packages may implement `Allocator` for **their own** types (e.g. `Arena`, `Pool`).
- Downstream packages must **not** implement `Allocator` for std types they do not own (e.g. `Global`), preventing conflicting impls.

The compiler does not enforce orphans in V0 beyond name resolution and duplicate-impl rejection; the rule is a **design contract** for std and ecosystem layout.

---

## Compiler and VM boundaries

| Layer | Responsibility |
|-------|----------------|
| Compiler | Lower `alloc_bytes` / `dealloc_bytes` call sites to `ALLOC` / `FREE`; require `unsafe` at call sites |
| VM | Bump heap + `(ptr, size)` ledger; double-free and size mismatch errors |
| Std | `Allocator` trait, `Layout`, `Global`, `VmHeapAllocator`; future `Box` / `DynamicArray` |
| Language | **No** allocator type in `Ty`; **no** built-in `Allocator` trait |

---

## Std module layout

| Path | Contents |
|------|----------|
| `std::core::alloc` | Intrinsic stubs `alloc_bytes`, `dealloc_bytes` (V0-065) |
| `std::core::memory::allocator` | `Layout`, `Allocator`, `VmHeapAllocator`, `Global` (alias) (V0-066) |
| `std::collections::dynamic_array` | `DynamicArray<T, A: Allocator>` (Std v0, after V0-066) |

---

## Acceptance (V0-066)

- [x] Design doc: trait shape, `Layout`, orphan rules, layering to `Box` / `Drop`
- [x] Std implementation: `Allocator`, `Global`, `VmHeapAllocator` in Phoenix source
- [x] No `Ty::Allocator` or compiler builtin for allocation policy
- [ ] `Box<T, A>` and `DynamicArray<T, A>` — Std v0 follow-ups (out of V0-066 scope)
