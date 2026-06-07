# Concurrency model

Status: post-MVP design target (documented now; scheduler and std I/O implemented later).

**Prerequisite:** [Language v0](../language-v0.md) — static types, ownership moves, `Result`/`Option`, trait-based error conversion (`From`), portable PHX0 bytecode, and C-ABI function pointers. The scheduler and schedulable-I/O runtime build on that compile-time contract; they are not a substitute for it.

Phoenix has no `async`/`await`. Concurrency is built into the VM runtime. **What the VM can do** (park for I/O, cross actor boundaries) must be visible at call sites — see [runtime-transparency.md](runtime-transparency.md).

---

## Core principle: everything is scheduler-managed

There is no truly synchronous execution context outside the VM scheduler for Phoenix user code.

- `main :: () => { ... }` is normal syntax, but at runtime it bootstraps into an **implicit root scheduler context**.
- Function calls, loops, and schedulable I/O run inside VM-managed execution contexts.
- The VM handles suspension and resumption only at **typed schedulable-I/O call sites** and **explicit `@…` actor operations** — not implicitly in pure sequential code.

This avoids the M:N stall problem: if worker threads block on synchronous file reads, the entire runtime stops and no other contexts can run.

---

## Sequential execution guarantee

Pure sequential computation (ordinary functions with no schedulable-I/O types in their signatures and no `@…` directives):

- runs **uninterrupted to completion** on one execution context
- does **not** implicitly suspend
- does **not** promise which scheduler worker thread executes it

Only **schedulable I/O** (type-visible at the call site; syntax TBD) and **explicit actor operations** may park the context or cross isolation boundaries.

---

## Schedulable I/O (default path)

Safe std I/O does not block OS worker threads. It is cooperative under the hood and **distinguishable from pure computation by type** at the call site.

```phoenix
main :: () =>
{
  const path: [u8; 10] = [99u, 111u, 110u, 102u, 105u, 103u, 46u, 116u, 120u, 116u];
  // Schedulable I/O — signature encodes VM park (exact type TBD)
  const bytes = File.read(path)?;
  process(bytes)?;
};
```

What happens at runtime:

1. `File.read` lowers to a schedulable VM I/O operation.
2. VM issues a non-blocking syscall (or async runtime equivalent).
3. Current context transitions to `ParkedAwaitIO`.
4. Scheduler runs other ready contexts on worker threads.
5. When data is ready, VM resumes the parked context with the result.

No `async`/`await`, no `@spawn` for simple reads — but the **type** tells you the context may park. See [runtime-transparency.md](runtime-transparency.md).

**MVP note:** MVP includes no std I/O and no scheduler runtime. This model is the target contract for when std I/O ships.

---

## Why blocking I/O cannot be exposed in safe paths

| Model | Problem |
|---|---|
| Optional actors only | Code outside actors runs on OS threads; blocking I/O stalls workers |
| `async`/`await` everywhere | Adds syntax coloring and manual poll/future complexity |
| Erlang-style explicit processes | Correct isolation, but heavy ceremony for simple reads |
| Go-style hidden goroutines | Scheduling works, but call sites hide suspension and I/O effects |

Phoenix choice:

- **Implicit scheduler context** for placement (all user code runs on the VM worker pool)
- **Type-visible schedulable I/O** for operations that may park (unlike hidden goroutine blocking)
- **Explicit actors** (`@spawn`, …) when isolation, supervision, or message protocols are intentional

Raw blocking syscalls are allowed only behind `#unsafe`/FFI boundaries, with documented risk of stalling worker threads.

---

## Implicit vs explicit concurrency

| Concern | Mechanism | Required? |
|---|---|---|
| Running `main` and normal functions | implicit scheduler context | always (post-MVP runtime) |
| File/network/timer waits | schedulable I/O (type-visible call sites) | default for safe std I/O |
| Fault isolation and supervision | explicit actors | opt-in |
| Deliberate message protocols | `@spawn`, `@send`, `@receive`, `@reply` | opt-in |

Explicit opt-in example:

```phoenix
const reader = @spawn(FileReader::new(path));
@send(reader, Read);
const reply = @receive(reader);
```

Use explicit actors when you want:

- isolated failure domains and supervisor trees
- long-lived services with structured message protocols
- intentional parallelism with explicit ownership transfer

Do **not** require explicit actors for basic `File.read`-style operations.

---

## Runtime architecture

### Execution contexts

An execution context is VM-managed runtime state:

- stack/registers or interpreter frame
- scheduling status (`Running`, `ParkedAwaitIO`, `ParkedAwaitMessage`, `Done`)
- optional mailbox linkage when the context represents an explicit actor

`main` starts in a root context. Explicit `@spawn` creates additional contexts registered as actors.

### Actor data model (explicit path)

When using explicit actors:

- actor = heap object with state + VM-managed mailbox queue
- idle actors consume minimal resources
- actors do not map 1:1 to OS threads

### Scheduler (M:N)

- fixed worker-thread pool (typically near CPU core count)
- M:N mapping of runnable contexts to workers
- explicit actors process one message at a time, then yield
- contexts blocked on I/O or mailbox waits are parked until wakeup

---

## Comparison: Erlang, Go, Phoenix

| | Erlang/Elixir | Go | Phoenix |
|---|---|---|---|
| Unit of concurrency | process (always) | goroutine (always) | scheduler context (always) |
| Visible at call site? | yes (`spawn`, messages) | mostly hidden | schedulable I/O in types; actors via `@…` |
| Simple file read | via process/port abstraction | blocking-looking, runtime hidden | schedulable type + cooperative VM park |
| Explicit isolation | default | opt-in (channels, design) | opt-in (`@spawn`, supervision) |
| `async`/`await` | no | no | no |

Phoenix combines cooperative M:N scheduling with **honest call-site types** for I/O and **opt-in** Erlang-like actor isolation.

---

## Trait contract (explicit actors)

Actor capability is a trait-level contract, not a primitive type.

Future rule:

- actor-marked types must implement the `Actor` trait contract
- compile-time checks validate message protocol compatibility
- actor handle types come from runtime/core libraries, not built-in primitives

---

## Runtime directives (explicit, opt-in)

| Directive | Role |
|---|---|
| `@spawn(expr)` | create explicit actor context and register with scheduler |
| `@send(target, msg)` | move message into target mailbox |
| `@receive(target)` | dequeue owned message from mailbox |
| `@reply(msg)` | send reply in handler context |

These are not required for schedulable I/O or ordinary function calls.

---

## Failure model (explicit actors)

- per-actor isolation boundaries
- supervisor-managed restart policies
- VM traps actor crashes, applies policy, reclaims actor resources
- implicit root context failures are program-level (no supervisor unless configured)

---

## Related documents

| Topic | Document |
|---|---|
| Runtime transparency principle | [runtime-transparency.md](runtime-transparency.md) |
| Message ownership (explicit path) | [messages.md](messages.md) |
| VM scheduler/I/O contract | [vm-linear.md](vm-linear.md) |
| Directive split | [compiler-directives.md](compiler-directives.md) |
| Runtime vs language primitives | [type-system.md](type-system.md) |
| MVP scope (no std I/O) | [../mvp.md](../mvp.md) |
