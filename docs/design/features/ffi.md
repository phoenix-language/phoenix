# Foreign Function Interface (FFI)

FFI is how Phoenix calls foreign code and (in later phases) exposes Phoenix entry points to other languages. Callable values at the boundary follow [type-system.md — Callable values: four layers](type-system.md#callable-values-four-layers): **static `Call`** for direct Phoenix callees; **function pointers** (Layer 2) for C-style callbacks and **indirect dispatch**.

---

## `extern "C"` declarations (Phase A)

Phase A is **VM-hosted** foreign calls: the compiler records `extern "C"` signatures and the VM resolves symbols at load time (test stubs in v0; `dlopen`/`dlsym` deferred). Native Phoenix export is Phase B.

**No `@extern`.** `@` is reserved for runtime markers. FFI uses the keyword **`extern`** with a **required ABI string** (v0: `"C"` only).

### Block form

```phoenix
#import std::ffi::c_int;

pub extern "C" {
    c_add :: (a: s32, b: s32) => c_int;
    c_free :: (ptr: *mut u8) => ();
};
```

### Item form

```phoenix
extern "C" c_add :: (a: s32, b: s32) => c_int;
```

| Rule | Behavior |
|---|---|
| **ABI string** | Required; v0 accepts `"C"` only |
| **Signatures** | Phoenix `=>` return syntax; reuse `function_signature` |
| **Generics** | Forbidden on extern **import** items (fixed C symbols) |
| **Bodies** | None — link-time / VM stub symbols only |
| **Call sites** | **Compile error outside `unsafe`** — see below |
| **Fn pointer values** | Extern symbols are `Ty::Fn`, Copyable; taking address is safe; **calling** requires `unsafe` |

Example call:

```phoenix
main :: () => {
    unsafe {
        const sum: c_int = c_add(10, 2);
        const _ = sum;
    };
};
```

---

## `std::ffi` — C type aliases

C scalar names are **std type aliases**, not compiler builtins. Opt-in via `#import std::ffi::c_int` (not in prelude).

| Alias | v0 maps to |
|---|---|
| `c_void` | `()` |
| `c_char`, `c_uchar` | `u8` |
| `c_schar` | `s8` |
| `c_short`, `c_ushort` | `s16`, `u16` |
| `c_int`, `c_uint` | `s32`, `u32` |
| `c_long`, `c_ulong` | `s64`, `u64` |
| `c_size`, `c_ssize` | `u64`, `s64` |
| `c_float`, `c_double` | `f32`, `f64` |

v0 uses **fixed-width** mappings for portable tests; platform-specific `c_int` width is deferred until target ABI metadata exists.

---

## Function pointers at the boundary

C interop treats callbacks as raw function pointers — for example `void (*cb)(void*)`. Phoenix needs **concrete fn pointer types** at every `extern "C"` boundary.

| Rule | Rationale |
|---|---|
| Parameter types use explicit `Ty::Fn` shapes | Matches C ABI (fixed calling convention, known arity) |
| Callable values are pointer-sized and Copyable | No-GC; bitwise copy of address at the boundary |
| No closure or `dyn Trait` at C edge without an adapter | C only understands code pointers + explicit context |

**Monomorphization:** Generic Phoenix items cannot cross the C boundary without **explicit specialization**. Generic wrappers may call concrete extern symbols.

---

## Bytecode: `Call` vs `CallIndirect`

| Mechanism | When | Status |
|---|---|---|
| **`Call`** + `function_id` | Direct call to a known Phoenix or foreign stub id | **Implemented** |
| **`MakeFnPtr`** | Materialize fn pointer value (Phoenix `function_id` or foreign stub id) | **V0-053** |
| **`CallIndirect`** | Call through fn pointer on stack | **V0-053** |

Foreign stubs use `MakeFnPtr` / `CallIndirect` with `target_kind = foreign`.

---

## Phased rollout

| Phase | Scope | Callable model |
|---|---|---|
| **A — VM-hosted `extern "C"`** | Import C symbols; VM stubs / future dynamic link | Fn pointer types + `CallIndirect`; calls require `unsafe` |
| **B — Native export** | Phoenix functions callable from C | Monomorphized exports only |

---

## Related documents

- [type-system.md — Callable values: four layers](type-system.md#callable-values-four-layers)
- [vm-linear.md — Calls opcode family](vm-linear.md)
- [ownership.md — Safe vs unsafe regions](ownership.md#safe-vs-unsafe-regions)
