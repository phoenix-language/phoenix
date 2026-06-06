# Directives

Phoenix uses two directive sigils:

- `#...` compile-time directives
- `@...` runtime directives

This split is the canonical direction. Older drafts that used `@` for both are legacy forms.

---

## Taxonomy

| Category | Sigil | Purpose | MVP status |
|---|---|---|---|
| Compile-time | `#` | import, optimization hints, derive/config metadata | partially in MVP |
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

### `#unsafe`

Marks unsafe function or block region:

```phoenix
#unsafe raw_write :: (ptr: *mut u8, len: u32) => ()
{
  // raw pointer logic
};
```

```phoenix
copy_bytes :: (dst: *mut u8, src: *u8, n: u32) => ()
{
  #unsafe
  {
    // raw pointer reads/writes
  };
};
```

### `#derive(...)` (future)

Reserved for compiler-generated trait impls (post-MVP).

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
