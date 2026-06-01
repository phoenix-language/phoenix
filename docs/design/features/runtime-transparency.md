# Runtime transparency

Status: post-MVP design principle (documented now; schedulable-I/O type syntax TBD).

Phoenix is a hybrid language:

- **Compile time:** bytecode compilation, static typing, ownership and borrowing (no GC).
- **Runtime:** a VM that schedules execution, parks contexts for I/O, and manages explicit actor lifecycle.

That split is intentional. It also creates a responsibility: **the developer should never be surprised by the runtime.**

---

## Core principle

**Runtime transparency through the type system.**

Wherever the VM can affect program execution — parking a context for I/O, waiting on a mailbox, crossing an isolation boundary — that effect should be visible at the call site through **types** or **syntax**. Not through `async`/`await` noise, but through honest signatures and explicit annotations.

The goal: reading Phoenix code, a developer can always answer:

1. Is this **pure computation** (no VM suspension)?
2. Is this **schedulable I/O** (may park the current context)?
3. Is this an **explicit actor boundary** (`@spawn`, `@send`, `@receive`, …)?

The answer should be in the code itself, not in separate runtime documentation they have to hunt down.

Phoenix does **not** hide the runtime like Go hides goroutines. It surfaces VM effects at the right level of abstraction — like a compiled language would — without ceremony.

---

## Three-way call-site taxonomy

| Category | What it means | How you tell from code |
|---|---|---|
| **Pure computation** | Runs to completion on one execution context without implicit suspension | Ordinary function types with no schedulable-I/O or actor markers in the signature; no `@…` runtime directives |
| **Schedulable I/O** | May park the current context while the VM waits for readiness | **Type/signature** at the call site encodes schedulability (concrete syntax TBD); `?` when the operation returns `Result` |
| **Explicit actor boundary** | Crosses VM-managed isolation: spawn, mailbox send/receive | `@spawn`, `@send`, `@receive`, `@reply` |

### Pure computation

- Executes **uninterrupted to completion** on a single VM execution context.
- Makes **no promise** about which scheduler worker thread runs it.
- Does **not** implicitly suspend — arithmetic, control flow, borrows, and local moves alone never park the context.

### Schedulable I/O

- Safe std I/O (e.g. future `File.read`) lowers to cooperative VM operations: non-blocking syscalls, context park, scheduler wakeup.
- **Must be distinguishable from pure computation by type** at the call site (exact type representation is not locked yet).
- Still **no** `async`/`await`; suspension is a runtime behavior the type system admits honestly.
- Failures remain visible via `Result` and `?` — see [error-handling.md](error-handling.md).

Illustrative pattern (signature details will change when schedulable-I/O types are specified):

```phoenix
main :: () =>
{
  const path: [u8; 10] = [99u, 111u, 110u, 102u, 105u, 103u, 46u, 116u, 120u, 116u];
  // File.read's type will mark this as schedulable I/O — not pure computation
  const bytes = File.read(path)?;
  process(bytes)?;
};
```

### Explicit actor boundaries

- `@spawn(expr)` — create an isolated actor context.
- `@send(target, msg)` — move a message into a mailbox.
- `@receive(target)` — dequeue an owned message.
- `@reply(msg)` — reply in handler context.

Use these when you need fault isolation, supervision, or structured message protocols — not for ordinary file reads. See [concurrency.md](concurrency.md) and [messages.md](messages.md).

---

## Failure visibility

Functions that can fail return `Result<T, E>`. The `?` operator propagates errors at every call site that can fail, inside functions with a compatible return type.

Errors are values, not hidden control flow. This is part of the same transparency goal as schedulable I/O and actor directives.

---

## What stays implicit vs explicit

| Concern | Visible how? | Implicit? |
|---|---|---|
| Running on the VM scheduler | all user code (including `main`) uses an execution context | placement on a worker thread is implicit |
| Suspending for I/O | schedulable-I/O **types** at call sites | suspension itself is not implicit in pure code |
| Actor isolation / messaging | `@…` directives | never implicit |
| Failure | `Result` + `?` | never exceptions |

**Implicit placement, explicit suspension boundaries.** The scheduler moves contexts between workers; only typed schedulable I/O and explicit actor operations may park or cross isolation boundaries.

---

## Anti-patterns we avoid

| Approach | Why not Phoenix |
|---|---|
| Go-style hidden goroutines | Runtime unit exists for everything, but call sites do not reveal suspension or I/O |
| `async`/`await` everywhere | Colors the whole program; manual poll/future complexity |
| Erlang-style processes for every read | Correct isolation, heavy ceremony for simple I/O |
| “Sync-looking” I/O with no type distinction | Surprises developers when the context parks |

Phoenix keeps M:N scheduling and cooperative I/O, but **refuses to lie in the type system** about where the VM can interleave or park.

---

## Deferred design

Not specified in this document:

- Concrete schedulable-I/O type syntax (wrapper, effect annotation, trait bounds, etc.)
- Final std signatures for `File.read`, networking, timers
- Compiler rules for calling schedulable APIs from contexts that must stay pure

When the type shape is chosen, update examples in [concurrency.md](concurrency.md), [error-handling.md](error-handling.md), and the grammar docs.

---

## Related documents

| Topic | Document |
|---|---|
| VM park/resume mechanics | [concurrency.md](concurrency.md) |
| `Result`, `?` | [error-handling.md](error-handling.md) |
| `@` runtime directives | [compiler-directives.md](compiler-directives.md) |
| Message ownership (actors) | [messages.md](messages.md) |
| Ownership vs parking vs `@send` | [ownership.md](ownership.md) |
| Types vs runtime primitives | [type-system.md](type-system.md) |
| `AWAIT_IO` bytecode | [vm-linear.md](vm-linear.md) |
| MVP scope | [../mvp.md](../mvp.md) |
