# Foreign Function Interface (FFI)

FFI is how Phoenix calls foreign code and (in later phases) exposes Phoenix entry points to other languages. Callable values at the boundary follow [type-system.md — Callable values: four layers](type-system.md#callable-values-four-layers): **static `Call`** for direct Phoenix callees; **function pointers** (planned) for C-style callbacks and indirect dispatch.

---

## `@extern` declarations (Phase A sketch)

Phase A is **VM-hosted** foreign calls: the compiler records `@extern` signatures and the VM resolves symbols at load time. Native Phoenix export is Phase B.

Example:

```phoenix
@extern malloc :: (size: u8) -> *mut u8; // C malloc returns a pointer to the allocated memory
@extern free :: (ptr: *mut u8) -> (); // C free frees the memory pointed to by the pointer
```

`Ty::Fn` in signatures today describes the **type** of a foreign function or parameter; it does not yet produce first-class fn **values** inside Phoenix code.

---

## Function pointers at the boundary

C interop treats callbacks as raw function pointers — for example `void (*cb)(void*)`. Phoenix needs **concrete fn pointer types** at every `@extern` boundary, not opaque “any function” values.

| Rule | Rationale |
|---|---|
| Parameter types use explicit `Ty::Fn` shapes | Matches C ABI (fixed calling convention, known arity) |
| Callable values are pointer-sized and Copyable (target) | No-GC; bitwise copy of address at the boundary |
| No closure or `dyn Trait` at C edge without a documented adapter | C only understands code pointers + explicit context (`void*`) |

**Monomorphization:** Generic Phoenix items cannot cross the C boundary without **explicit specialization**. Monomorphization produces concrete symbols for export (for example `foo$s32` or a user `#[no_mangle]` name when that attribute lands). A generic template never appears as a parameterized C symbol.

| Boundary rule | Detail |
|---|---|
| Export shape | Only monomorphized, concrete signatures are C-visible |
| Import shape | `@extern` declarations name fixed `Ty::Fn` shapes — no generic parameters |
| Future export | `extern "C"` + optional `#[no_mangle]` on specialized defs (Phase B) |
| Future layout | `repr(C)` on structs/enums passed by pointer across the edge |
| Text at edge | Length-prefixed `(ptr, len)` byte views — not a primitive owned `string` |

Phase A remains VM-hosted `@extern` (current sketch). Phase B adds native export via JIT/AOT with the same monomorphization contract.

Example direction (conceptual — export syntax TBD):

```phoenix
// Specialized at compile time before any C-visible symbol exists
sort_s32 :: (data: *mut s32, len: u32, cmp: :: (s32, s32) => bool) => { /* … */ };
```

A C header would see `cmp` as a function pointer with a fixed signature, not a generic Phoenix generic parameter.

---

## Bytecode: static `Call` vs planned `IndirectCall`

| Mechanism | When | Status |
|---|---|---|
| **`Call`** + `function_id` | Direct call to a known Phoenix `DefId` (including monomorphized specials) | **Implemented** |
| **`IndirectCall`** (planned; [vm-linear.md](vm-linear.md) names optional `CALL_INDIRECT`) | Call through a fn pointer value on the stack | **Not implemented** |

Phase A `@extern` may lower foreign entry as a dedicated intrinsic or VM hook before general `IndirectCall` exists; the long-term model still treats boundary callbacks as Copyable fn pointer values.

---

## Phased rollout

| Phase | Scope | Callable model |
|---|---|---|
| **A — VM-hosted `@extern`** | Import C symbols into the Phoenix VM; sketch signatures in source | Static `Call` to VM foreign stubs; fn pointer **types** in `@extern` signatures |
| **B — Native export** | Phoenix functions callable from C/other languages | Monomorphized exports; C-visible fn pointers only with concrete signatures |

---

## Future boundary types (not MVP)

These are design direction for Phase B and later; syntax is conceptual until grammar and `repr` attributes land.

| Feature | Purpose |
|---|---|
| `repr(C)` on structs/enums | Stable layout for C structs passed by pointer |
| `extern "C"` calling convention | Explicit ABI on `@extern` and exported Phoenix fns |
| Length-prefixed `str` at boundary | Pass `(ptr, len)` views compatible with C callers without a primitive owned `string` |

Closures and `dyn Trait` remain in-language dynamism ([type-system.md](type-system.md#callable-values-four-layers)); they do not cross the C ABI without an explicit fn pointer + context pointer adapter.

---

## Related documents

- [type-system.md — Callable values: four layers](type-system.md#callable-values-four-layers)
- [type-system.md — Generics strategy (monomorphization)](type-system.md#generics-strategy-monomorphization)
- [vm-linear.md — Calls opcode family](vm-linear.md)
- [ownership.md — Copyable](ownership.md)
