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
| Default for collections | `Global` is the default backing allocator for `UniquePtr`, `DynamicArray`, and similar owning types |
| No compiler special cases | Trait dispatch and monomorphization only — same as `Iterator` or `Drop` |

---

## Layering

```text
Application / collections
        │
        ▼
  UniquePtr<T, A>  ──Drop──►  A.dealloc(ptr, layout)
  DynamicArray<T, A>         (Std v0)
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

### `Allocator` → `Drop` on `UniquePtr`

`UniquePtr<T, A>` (Std v0; Rust `Box` equivalent) stores:

- `ptr: *mut T` (or raw bytes + typed view),
- `layout: Layout`,
- `alloc: A` (or `A` held by reference / ZST handle).

Construction calls `A.alloc(layout)` (inside `unsafe` at the std boundary). On scope exit, `Drop for UniquePtr<T, A>` calls `A.dealloc(ptr, layout)` — **not** the intrinsics directly. Custom allocators therefore control both allocation and reclamation without changing VM opcodes.

`DynamicArray<T, A: Allocator = Global>` follows the same pattern: grow/reallocate via `Allocator`; `Drop` frees the backing store through the trait.

**Language v0 scope:** document this layering; implement `Allocator`, `Layout`, `Global`, and `VmHeapAllocator` only. `UniquePtr` and `DynamicArray` are subsequent Std v0 items.

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

## Memory alignment and padding (deferred)

Language v0 does **not** apply C-style struct padding or platform alignment rules. This is intentional: the MVP VM uses width-faithful scalars, aggregate arena handles, and a **packed** default layout (field byte sizes are summed without inter-field padding). `Layout.align` is recorded in std for API uniformity but is not enforced by the heap yet.

**Do not** silently add padding to the default struct layout — that would change `size_of`, break the current compiler/VM contract, and still would not match C without an explicit `repr` and target ABI metadata.

### When alignment becomes necessary

| Need | Why padding / align matters |
|------|-----------------------------|
| **`repr(C)` / C struct interop** | C ABIs require specific field offsets and alignment |
| **Raw memory views** | `*T` load/store assuming a contiguous byte layout |
| **Atomics / SIMD** | Often require aligned addresses |
| **Native codegen or mmap** | Platform ABIs and file formats assume alignment |
| **Honest `Layout.align`** | Heap must return blocks aligned to the requested boundary |

### Practical sequencing

Implement alignment as **scoped features**, not a global layout change:

| Phase | Scope | Action |
|-------|--------|--------|
| **Now (Language v0)** | Docs + compiler | Keep packed default. Document that `size_of` is packed; std passes `align: 1u` to VM-backed allocators. No compiler or VM behavior change. |
| **Before C struct FFI** | Compiler + tests | Add opt-in **`repr(C)`** (or equivalent) with platform alignment rules; layout pass computes offsets and padding; golden tests against known C struct sizes. See [ffi.md](ffi.md). |
| **With allocator hardening** | VM + std | Teach `ALLOC` / the heap ledger to honor **`Layout.align`** when `align > 1` (start with power-of-two alignments). Update `VmHeapAllocator` and collection growth paths to request correct alignment for `repr(C)` buffers. |
| **Later (optional)** | Language | **`repr(align(N))`**, **`repr(packed)`**, or explicit **`repr(Phoenix)`** naming the current default; target ABI metadata for platform-specific `c_int` width ([ffi.md](ffi.md)). |

### Target end state

- **Default (`repr(Phoenix)`):** packed, VM-oriented layout — good for Language v0 and in-VM aggregates.
- **Opt-in `repr(C)`:** aligned layout for types and values that cross the FFI or raw-memory boundary.
- **`Layout`:** both `size` and `align` are meaningful at allocation sites; dealloc must use the same pair.

**Related:** [ffi.md](ffi.md) (C ABI phase B), [vm-linear.md](vm-linear.md) (`ALLOC` / `FREE`), [type-system.md](type-system.md) (struct definitions, `size_of`).

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
| `alloc` | Returns a pointer to at least `layout.size` bytes. When the VM heap cap is exceeded, `phx_vm::run` fails with `VmError::OutOfMemory` (no null pointer in v0). Std-level null-on-failure remains post-MVP |
| `dealloc` | Releases the block previously obtained with the **same** `layout.size`; must not be called on pointers not allocated through this allocator instance |

Methods take `&mut Self` so stateful allocators (arenas, pools) can update bookkeeping. `Global` / `VmHeapAllocator` are zero-sized; mutation is a no-op but keeps a uniform trait surface for generic collections.

**V0-066:** `Allocator` is an `unsafe trait` (Option B). `VmHeapAllocator :: unsafe impl :: Allocator` calls heap intrinsics in method bodies without per-method `unsafe` keywords. Application code must still use `unsafe { … }` when calling `alloc` / `dealloc` on a trait receiver.

**Not in the trait (Std v0):** `realloc`, `grow`, `shrink`, `allocate_zeroed`. Collections compose `alloc` + copy + `dealloc` until a later std revision adds optional extension traits.

---

## `VmHeapAllocator` and `Global`

| Type | Role |
|------|------|
| `VmHeapAllocator` | ZST; **only** std type whose implementation calls `alloc_bytes` / `dealloc_bytes` (inside `unsafe`) |
| `Global` | Public default name; **type alias** for `VmHeapAllocator` (trait bound checks resolve aliases to the underlying impl) |

Application and library code should import `Global` (or a custom `Allocator`), not `std::core::alloc` intrinsics. Direct intrinsic use remains legal for low-level tests and compiler fixtures but is discouraged outside `std::core::memory`.

**Heap cap and configuration:** The VM linear heap is capped by default (see [vm-linear.md](vm-linear.md) — VM resource limits). Application authors will eventually configure this via project settings or CLI; v0 relies on the built-in default until that surface ships.

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
| VM | Bump heap + `(ptr, size)` ledger; double-free, size mismatch, and use-after-free errors (`UseAfterFree` when ledger checking is enabled — default in v0) |
| Std | `Allocator` trait, `Layout`, `Global`, `VmHeapAllocator`; `UniquePtr` / `DynamicArray` |
| Language | **No** allocator type in `Ty`; **no** built-in `Allocator` trait |

---

## Std module layout

| Path | Contents |
|------|----------|
| `std::core::alloc` | Intrinsic stubs `alloc_bytes`, `dealloc_bytes` (V0-065) |
| `std::core::memory::allocator` | `Layout`, `Allocator`, `VmHeapAllocator`, `Global` (alias) (V0-066) |
| `std::core::memory::unique_ptr` | `UniquePtr<T, A: Allocator = Global>`; `new(value, layout, alloc)` |
| `std::collections::dynamic_array` | `DynamicArray<T, A: Allocator = Global>`; `empty(alloc)` |

---

## Acceptance (V0-066)

- [x] Design doc: trait shape, `Layout`, orphan rules, layering to `UniquePtr` / `Drop`
- [x] Std implementation: `Allocator`, `Global`, `VmHeapAllocator` in Phoenix source
- [x] No `Ty::Allocator` or compiler builtin for allocation policy
- [x] `UniquePtr<T, A: Allocator = Global>` and `DynamicArray<T, A: Allocator = Global>` with `new` / `empty` factories (`std::core::memory::unique_ptr`, `std::collections::dynamic_array`)
