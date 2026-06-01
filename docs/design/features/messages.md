# Actor messages and ownership

Status: post-MVP design for **explicit** actor messaging.

This document covers the opt-in message-passing path (`@spawn`, `@send`, `@receive`, `@reply`). It does **not** describe schedulable I/O (`File.read`, etc.) — that uses **type-visible schedulable-I/O call sites**, not `@send`. See [runtime-transparency.md](runtime-transparency.md) and [concurrency.md](concurrency.md).

Phoenix plans move-oriented message passing between explicit actors. This differs from Erlang-style deep-copy messaging.

---

## Core semantics

| Directive | Ownership |
|-----------|-----------|
| `@send(target, msg)` | **Moves** `msg` into target mailbox. Sender binding becomes invalid unless `Copyable` or cloned. |
| Delivery | VM transfers owned `msg` into receiver mailbox/runtime heap; `on_message` receives owned `msg`. |
| `@receive(target)` | Returns owned message value, not mailbox borrow. |
| `@reply(msg)` | **Moves** reply to caller; same rules as `@send` (handler context only). |

Type checking target (future):

- actor-marked types satisfy an `Actor` trait contract
- handle/message compatibility is validated via trait and associated types
- actor handles are not primitive language types

## Implicit context vs explicit messaging

| Mechanism | Purpose |
|---|---|
| Schedulable I/O (`File.read`, etc.) | park execution context until ready; visible in types at call site |
| `@send` / `@receive` | deliberate protocol between explicit actors |

Do not use explicit messaging for simple reads — use schedulable I/O APIs instead.

---

## Patterns

### Copyable messages

```
Message :: enum { Ping, Pong }

@send(echo, Ping);   // Copyable — sender may still use Ping
```

### Keep-and-send with Clone

```
const payload: [u8; 3] = [1u, 2u, 3u];
@send(worker, payload.clone());
log(payload);
```

### Large payloads: Arc and Bytes

```
const body = Arc::new(load_large_config());
@send(parser, body);   // move Arc handle
```

---

## Forbidden patterns

- `&T` / `&mut T` into another actor’s message or state
- Cross-actor references in closures passed to `@spawn`
- `@receive` returning a borrow into mailbox memory

Use shared-handle types (such as Arc-like library types) for immutable fan-out payloads.

---

## Message types and protocols

Protocols are ordinary enum or struct types (`Name :: enum`, `Name :: struct`) declared by actor implementations.

Request/response: message enum variants, or `@receive` after `@send` with std correlation helpers (future).

---

## Mailbox placement

The mailbox is VM-managed. Actor state structs should not store mailbox internals directly.

Runtime integration:

- mailbox enqueue/dequeue and runnable-state transitions are VM operations
- when an explicit actor blocks on schedulable I/O, VM parks its context and schedules others (same park/resume mechanism as non-actor contexts)
- on actor crash, VM isolates the failure, notifies supervisors, and cleans up mailbox/state resources for that actor boundary

There is no implicit deep copy at `@send`. Use the patterns below to keep a local copy.

---

## Related documents

| Topic | Document |
|---|---|
| Runtime transparency | [runtime-transparency.md](runtime-transparency.md) |
| Concurrency architecture | [concurrency.md](concurrency.md) |
| Directives split | [compiler-directives.md](compiler-directives.md) |
| VM format | [vm-linear.md](vm-linear.md) |
