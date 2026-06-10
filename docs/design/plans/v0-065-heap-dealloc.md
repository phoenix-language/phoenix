# V0-065 — Heap deallocation implementation plan

Status: **Done** (implemented)

**Authority:** [language-v0-completion-roadmap.md](../language-v0-completion-roadmap.md) (lines 291–308)

---

## Overview

Add symmetric heap deallocation to V0-030 `ALLOC`: `std::core::alloc::dealloc_bytes` lowered to VM `FREE` opcode (49). The VM tracks live blocks in an allocation ledger; double-free and size mismatch fail at runtime. Heap storage is not compacted — freed regions are zeroed in place.

---

## VM contract

| Operation | Behavior |
|---|---|
| `Alloc` | Append `size` bytes to `heap`; register `(ptr, size)` in ledger |
| `Free` | Pop `ptr`, `size`; verify exact ledger entry; remove; zero `[ptr..ptr+size)` |
| Double-free | `VmError::DoubleFree` |
| Wrong size / OOB / tagged ptr | `VmError::InvalidFree` |

Stack for `Free`: evaluate args left-to-right (`ptr`, `size`); pop `size` then `ptr` (same as `PtrStore`).

---

## Std surface

`std/src/core/alloc.phx` — `dealloc_bytes :: (ptr: *mut u8, size: u32) => ()`, callable only inside `unsafe`.

---

## Acceptance mapping

| Criterion | Artifact |
|---|---|
| alloc + dealloc runs | `tests/cli/fixtures/heap_dealloc/` |
| unsafe required | `tests/cli/fixtures/heap_dealloc_unsafe/` |
| double-free rejected | `tests/cli/fixtures/heap_dealloc_double/` |
| Drop calls dealloc | `tests/cli/fixtures/heap_drop_dealloc/` |
| VM unit tests | `phx-vm` ledger tests |
| Compiler unit tests | `typeck.rs`, `lower.rs`, `codegen.rs` |

**Completion gate:** `just pre-commit` + `cargo test --workspace`.
