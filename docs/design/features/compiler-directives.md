# Directives

Phoenix uses two directive sigils:

- `#...` compile-time directives
- `@...` runtime directives

This split is the canonical direction. Older drafts that used `@` for both are legacy forms.

---

## Taxonomy

| Category | Sigil / form | Purpose | MVP status |
|---|---|---|---|
| Compile-time keywords | `#` | import, unsafe regions, optimization hints | `#import` wired end-to-end; `#derive` codegen ([V0-056](../language-v0.md#v0-056--derive-minimal)); `#unsafe`, `#inline` / `#cold` / `#hot` parse-only |
| Item attributes | `#[...]` | conditional compilation, deprecation, lint policy | **Implemented** ([V0-039](../language-v0.md#v0-039--item-attributes-and-conditional-compilation)): `cfg`, `deprecated`, `allow`, `must_use` |
| Runtime | `@` | runtime VM actions (especially actor runtime actions) | post-MVP heavy semantics |

---

## Compile-time directives (`#`)

### `#import`

Imports names into scope. **File scope** (module top) and **block scope** (inside `{ … }`) — see [modules.md](modules.md#scoped-imports-mvp).

```phoenix
#import core::mem
#import app::math::{add, sub}

main :: () => {
  #import app::math::mul;
  const _ = mul(1, 2);
};
```

Block imports are compile-time only: they introduce names locally; they do not create runtime module values.

### `#inline`, `#cold`, `#hot`

Optimization and placement hints (implementation-dependent).

### `unsafe`

Marks unsafe function or block region (keyword — not a `#` directive):

```phoenix
unsafe raw_write :: (ptr: *mut u8, len: u32) => ()
{
  // raw pointer logic
};
```

```phoenix
copy_bytes :: (dst: *mut u8, src: *u8, n: u32) => ()
{
  unsafe
  {
    // raw pointer reads/writes
  };
};
```

**`extern "C"` calls require `unsafe`.** See [ffi.md](ffi.md).

**Effectively-unsafe functions require `unsafe` at the call site:** top-level `unsafe fn`, methods of an `unsafe trait`, and explicit `unsafe fn` methods on safe traits. Same rule as intrinsics — wrap the call in `unsafe { … }` or declare the enclosing function `unsafe`. See [traits.md](traits.md#unsafe-trait-and-unsafe-impl-option-b).

### `#derive(...)` (V0-056)

Compiler-generated trait impls on **record structs, tuple structs** ([V0-057](../language-v0.md#v0-057--opaque--newtype-wrappers)), and **enums** ([V0-056](../language-v0.md#v0-056--derive-minimal)). Also accepted as `#[derive(...)]` (see Item attributes below).

| Supported trait | Generated impl |
|---|---|
| `Copyable` | Empty marker impl when all fields / variant payloads are Copyable-eligible |
| `PartialEq` | `eq :: (self: &Self, other: &Self) => bool` — field- or variant-wise `==` |
| `Debug` | `fmt :: (self: &Self) => [u8; 32]` — placeholder type-name buffer (not full formatting) |

**Rejected:** generic types, unknown traits (`Clone`, `Eq`, …), duplicate derive or existing manual impl, derive on functions/traits/impl methods.

---

## Item attributes (`#[...]`)

Item metadata uses **bracket attributes** alongside `#` compile-time directives. **`unsafe` and `extern` are language keywords**, not `#` directives.

```phoenix
#[deprecated(since = "0.2.0", note = "use new_name instead", suggestion = "new_name")]
pub old_fn :: () => () { };

#[cfg(target_os = "linux")]
pub linux_only :: () => () { };

#[must_use]
pub important :: () => s32 { 1 };

#[allow(deprecated)]
main :: () => {
  old_fn();
};
```

`#derive(Debug, PartialEq)` and `#[derive(Debug, PartialEq)]` are equivalent.

### `#[cfg(...)]`

Conditional compilation: items whose `#[cfg]` predicate is false at compile time are **removed** before name resolution.

| Predicate | Form |
|---|---|
| `target_os` | `target_os = "linux"` (and other host OS strings) |
| `target_arch` | `target_arch = "x86_64"` (and other host arch strings) |
| `debug_assertions` | `debug_assertions` (flag, no value) |
| `not(...)` | `not(target_os = "windows")` |

Defaults follow the host compile (`std::env::consts::OS` / `ARCH`; `debug_assertions` true in debug builds). Unknown cfg keys are compile errors.

`all(...)` / `any(...)` and file-level `#![cfg(...)]` are deferred.

### `#[deprecated(...)]`

Emits a **warning** at use sites when a deprecated item is referenced by name.

| Argument | Required | Purpose |
|---|---|---|
| `since` | no | Version string shown in the warning |
| `note` | no | Human-readable deprecation reason |
| `suggestion` | no | Preferred replacement name |

Cross-crate deprecation via `.pxi` export metadata is deferred.

### `#[allow(name)]`

Suppresses listed warning kinds in the attributed item's body (and nested blocks). v1 recognized names: `deprecated`, `must_use`. Unknown `allow` names are compile errors.

`#[deny(...)]` / `#[forbid(...)]` (warnings-as-errors) are deferred.

### `#[must_use]`

Warns when a function's non-unit return value or a constructed `#[must_use]` type is used as a discarded statement expression. Discarding std `Result` / `Option` is enforced separately as a type error (E2041/E2042); `#[allow(must_use)]` does not suppress those errors.

### Deferred item attributes

| Attribute | Status |
|---|---|
| `#[stable(...)]` / `#[since(...)]` | API versioning metadata — docs/manifest only; no stability gates in v1 |
| `#[deny(...)]` / `#[forbid(...)]` | Warnings-as-errors policy |
| Field-level `#[...]` | Not in v1 |
| PXI export of attribute metadata | Cross-crate linting deferred |

Warnings do not fail `phx check`, `phx compile`, `phx run`, or `phx build` in v1; they are printed and compilation continues. Lint warnings are emitted whenever the command type-checks source; incremental project builds and `phx run --no-build` may skip lints when artifacts are fresh.

---

## Runtime directives (`@`)

Runtime directives are expression-level operations for **explicit** actor/message protocols.

Current planned runtime set:

- `@spawn(expr)`
- `@send(target, msg)`
- `@receive(target)`
- `@reply(msg)`

These are opt-in and primarily post-MVP. They are **not** required for:

- running `main` or normal function calls
- basic file/network I/O once std I/O ships (schedulable-I/O types at call sites — see [runtime-transparency.md](runtime-transparency.md))

Use `@spawn` when you want deliberate concurrency, fault isolation, or structured message protocols. See [concurrency.md](concurrency.md) and [runtime-transparency.md](runtime-transparency.md).

---

## `#unsafe` and scheduler bypass

`#unsafe` FFI and raw syscalls can bypass the scheduler and block OS worker threads.

This is discouraged for normal application code. Safe std I/O must lower to schedulable VM operations (`AWAIT_IO`-style) so the M:N runtime keeps making progress.

---

## Migration note

When updating old docs/examples:

- Replace compile-time uses like `@import`, `@unsafe`, `@inline` with `#import`, `#unsafe`, `#inline`.
- Keep runtime actions as `@...`.
- Replace legacy **"transparent I/O"** wording with **schedulable I/O** and **type-visible call sites** (see [runtime-transparency.md](runtime-transparency.md)).

Do not mix sigils for the same directive category.

---

## Post-MVP actor contract note

Actor directives and runtime messaging are deferred from MVP implementation.

Future semantic rule:

- Actor-marked types must satisfy the Actor trait contract.
- Actor handles are library/type-system contracts, not primitive language types.

See [concurrency.md](concurrency.md) and [messages.md](messages.md).
