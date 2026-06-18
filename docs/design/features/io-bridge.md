# Pre-scheduler I/O bridge

Status: **Language v0 bridge** — documented interim stdout path before schedulable `std::io` ships.

Phoenix defers **safe** std I/O until the M:N scheduler and schedulable-I/O runtime exist ([`mvp.md`](../mvp.md), [`runtime-transparency.md`](runtime-transparency.md)). This document specifies a **minimal bridge** so contributors can run hello-world-style programs that write bytes to the host terminal without inventing ad hoc VM intrinsics.

---

## Scope

| In scope | Out of scope |
|---|---|
| VM-hosted foreign stub `phoenix_write_stdout(s: str) => c_ssize` | libc `dlopen` / raw `write(2)` from Phoenix |
| `std::io::write_stdout(s: str)` wrapper (hides `unsafe` for demos) | `String` printing via `to_string` + `write_stdout` (use `print` / `write_display_buf` instead) |
| `phx run` auto-registers the stub | Scheduler, `AWAIT_IO`, typed schedulable I/O |
| Const-pool `str` literals (rodata) | Heap-backed `str` views, networking, files |

---

## Architecture

Phase-A FFI ([`ffi.md`](ffi.md)): `extern "C"` imports lower to `CallIndirect` with `target_kind = foreign`. The VM dispatches a **Rust-hosted stub** that reads the Phoenix `str` aggregate, resolves UTF-8 bytes from the module constant pool, and calls `std::io::stdout().write_all` on the host.

`str` fat pointers in the VM are **not** native addresses (`PTR_CONST_TAG | pool_index` for literals). A direct libc `write(fd, buf, len)` bridge is therefore deferred; the stub decodes VM representation on the host.

```text
main → std::io::write_stdout → unsafe phoenix_write_stdout → VM stub → host stdout
```

---

## Safety

- **`extern "C"` invocation requires `unsafe`** per [`ownership.md`](ownership.md).
- `write_stdout` wraps the extern call so demo `main` stays safe-looking; this is **not** a stability promise for production APIs.
- The stub performs a **blocking** host write and may stall the single MVP worker thread. Documented risk; acceptable only pre-scheduler.

---

## Stub contract (Phase A)

| Rule | Detail |
|---|---|
| Symbol | `phoenix_write_stdout` |
| Phoenix signature | `(s: str) => c_ssize` |
| Second symbol | `phoenix_write_display_buf(buf: [u8; 32]) => c_ssize` — writes a zero-terminated display buffer (used by `std::text::fmt::print`) |
| Registration | `register_builtin_foreign_stubs()` in `phx-vm`; `phx run` calls it before execution |
| **Id alignment** | Codegen assigns foreign ids by `ExternFn` declaration order in the linked program. Bridge programs may declare **at most** `phoenix_write_stdout` and `phoenix_write_display_buf` until linker-stable foreign ids ship (Phase B / PHX-070). |
| v0 payload | Const-pool-backed `str` literals only; other `str` provenance returns a VM aggregate error |

---

## Migration

When the scheduler and schedulable-I/O types land:

1. Replace `write_stdout` with typed `std::io` APIs that may park the execution context.
2. Retire or gate the bridge stub behind an explicit opt-in / test-only feature.
3. Update [`type-system.md`](type-system.md) std I/O row to point at schedulable APIs.

---

## Related documents

- [`ffi.md`](ffi.md) — Phase-A VM-hosted foreign calls
- [`runtime-transparency.md`](runtime-transparency.md) — target schedulable I/O model
- [`vm-linear.md`](vm-linear.md) — MVP debug channel (`--dump-main`) vs user I/O
