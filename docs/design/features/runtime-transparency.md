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

## Schedulable I/O contract

Post-MVP contract between **typed std I/O**, the **VM scheduler**, and **Phoenix call sites**. Concrete schedulable-I/O type syntax remains TBD; this section pins down park/resume and error behavior so std and VM can integrate without surprises.

Implementation reference (in-tree harness, PHX-sched-0): [`source/phx-vm/src/scheduler/mod.rs`](../../source/phx-vm/src/scheduler/mod.rs) — [`ParkReason`](../../source/phx-vm/src/scheduler/park.rs), [`SingleThreadScheduler::resume`](../../source/phx-vm/src/scheduler/harness.rs), execution-context states in [vm-linear.md](vm-linear.md).

### Who calls park

| Layer | Responsibility |
|---|---|
| **Phoenix developer** | Calls schedulable std APIs (e.g. future `File.read`); never parks explicitly |
| **Compiler / codegen** | Lowers schedulable calls to `AWAIT_IO` (and related opcodes); emits `Result` return types |
| **VM dispatcher** | On `AWAIT_IO`, issues non-blocking I/O, then parks the **current execution context** |

Park is always a **VM scheduler operation** on the context that is actively executing bytecode on a worker thread. User code does not receive a park primitive — suspension is implied by calling a schedulable-I/O-typed function.

Two park reasons (see [`ParkReason`](../../source/phx-vm/src/scheduler/park.rs)):

| Reason | When the VM parks | Schedulable I/O? |
|---|---|---|
| `AwaitIo` | File, network, or timer readiness not yet available | **Yes** — this section |
| `AwaitMessage` | Explicit actor mailbox wait (`@receive`, …) | **No** — actor boundary; see [concurrency.md](concurrency.md) |

Invariant: **safe std I/O never blocks a worker thread.** If the syscall would block, the VM parks with `AwaitIo` and runs other runnable contexts until a wakeup arrives.

### What wakes a context

A parked context returns to the runnable queue only when its wait condition is satisfied:

| Park reason | Wakeup source | Scheduler action |
|---|---|---|
| `AwaitIo` | Host readiness (epoll / kqueue / IOCP or equivalent), or completion of an async I/O submission | Mark context `Runnable`, enqueue on the run queue |
| `AwaitMessage` | Mailbox delivery to the waiting actor | Same — `Runnable` + enqueue |

End-to-end flow for schedulable I/O (matches [concurrency.md](concurrency.md)):

1. Context executes `AWAIT_IO` for a pending operation.
2. VM transitions context to `ParkedAwaitIO` and removes it from the worker's active slot.
3. Scheduler runs other ready contexts on the pool.
4. I/O subsystem signals readiness for that operation.
5. Scheduler calls resume: `ParkedAwaitIO` → `Runnable`, context is dequeued and continues at the `AWAIT_IO` continuation with the operation outcome.

Timers and multi-stage I/O (partial reads, connect-then-read) use the same contract: each point that would block a worker parks once; wakeups re-enqueue the same context until the std API's `Result` is ready to return to Phoenix.

### How errors surface to Phoenix

Schedulable I/O follows the same **failure visibility** rules as pure computation — no hidden exceptions, no scheduler-specific error channel.

| Situation | VM behavior | Phoenix sees |
|---|---|---|
| Operation completes **before** park (e.g. data already buffered, immediate validation failure) | No park; opcode completes synchronously | `Ok(T)` or `Err(E)` per the std signature |
| Operation parks, then completes successfully after wakeup | Resume with success payload on the continuation | `Ok(T)` — caller receives the value |
| Operation parks, then fails (I/O error, timeout, permission denied) | Resume with error payload on the continuation | `Err(E)` — typed std error (concrete `E` TBD per API) |
| Caller uses `?` | N/A (language) | Error propagates to the enclosing function's return type, same as non-schedulable `Result` calls — see [error-handling.md](error-handling.md) |

Rules:

- **Every schedulable std I/O function returns `Result<T, E>`** (or a type that desugars to one). The type system marks schedulability; `Result` marks failure.
- **Park/resume is not a separate failure mode** for Phoenix code. Suspension is transparent control flow inside the VM; only `Ok`/`Err` cross the language boundary.
- **VM scheduler invariant violations** (e.g. resume while not parked) are internal [`SchedulerError`](../../source/phx-vm/src/scheduler/harness.rs) diagnostics for the harness and runtime — they surface as internal VM faults, not as catchable Phoenix errors.
- **FFI / `#unsafe` blocking I/O** sits outside this contract; it may stall workers and does not get schedulable-I/O typing.

Illustrative call site (error path unchanged by scheduling):

```phoenix
read_config :: () -> Result<Config, IoError> =>
{
  const path: [u8; 10] = [99u, 111u, 110u, 102u, 103u, 46u, 116u, 120u, 116u];
  const bytes = File.read(path)?;   // may park; ? still propagates IoError
  parse_config(bytes)
};
```

When std I/O ships, std authors implement against this contract; the compiler and VM must not expose park/resume to Phoenix while hiding `Result` semantics.

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
