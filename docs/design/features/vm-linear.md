# VM and bytecode format (MVP)

This document defines the concrete bytecode file contract for Phoenix MVP and the minimum VM behavior needed to execute compiled programs.

It also records post-MVP runtime responsibilities so the VM boundary is explicit across the design docs.

Scope:

- single-process stack VM
- no actor runtime requirements in MVP
- no JIT/hot reload requirements in MVP

---

## What the VM does for Phoenix

The VM exists to provide runtime orchestration, portability, and operational capabilities around compiled Phoenix programs.

Important boundary:

- ownership and move semantics define memory safety at the language/type-system level
- the VM is not introduced to replace that ownership model
- the VM is introduced to run and coordinate code at runtime

Core VM value (full vision):

- process scheduling for lightweight actors through an M:N scheduler
- bytecode portability across OS targets where the Phoenix VM is available
- hot code reloading with message-boundary handoff between module versions
- JIT compilation of hot paths while retaining bytecode distribution
- non-blocking I/O and syscall abstraction integrated with scheduler wakeups
- actor lifecycle and supervision orchestration
- crash isolation boundaries so one actor failure does not corrupt others

MVP subset in this document remains intentionally narrow: loader validation plus stack-machine interpretation.

---

## File format overview

Bytecode file layout:

1. File header
2. Section table
3. Section payloads

All integer fields are little-endian.

---

## File header

| Offset | Size | Field | Description |
|---|---:|---|---|
| 0 | 4 | magic | ASCII `PHX0` |
| 4 | 2 | version_major | format major version |
| 6 | 2 | version_minor | format minor version |
| 8 | 4 | flags | reserved; MVP must be `0` |
| 12 | 4 | section_count | number of section table entries |
| 16 | 4 | entry_function_id | function id for `main` (`0xFFFF_FFFF` = no entry, library objects) |
| 20 | 4 | reserved | reserved for alignment/future use |

Header size: 24 bytes.

---

## Section table

Each section entry:

| Field | Size | Meaning |
|---|---:|---|
| section_kind | 2 | enum tag |
| reserved | 2 | reserved |
| offset | 4 | file offset to payload |
| length | 4 | payload byte length |

Section kinds (MVP):

- `1`: constants
- `2`: types
- `3`: functions
- `4`: code
- `5`: symbols (optional debug names; **not written** by the MVP compiler — reserved for future tooling)
- `6`: local layouts (format minor 1+; verifier cross-checks slot kinds)

**Compiler-only notes (MVP):**

- **`POP` (opcode 3)** — defined for the verifier/VM; Phoenix codegen does not emit it (void results are handled via control flow and `STORE_LOCAL` to `_`).
- **`ALLOC` (38) / `PTR_STORE` (40)** — implemented in the VM for future heap/alloc intrinsics; not emitted until the intrinsic kernel spelling is fixed in design (`grammar-deferred.md`).

---

## Constants section

Constants payload begins with `u32 constant_count`, followed by entries:

| Field | Size | Meaning |
|---|---:|---|
| const_tag | 1 | constant type |
| reserved | 1 | reserved |
| byte_len | 2 | payload bytes for this constant |
| payload | N | constant bytes |

Constant tags:

- `1`: signed integer — payload length is **1, 2, 4, 8, or 16** bytes (little-endian), matching the source primitive width (`s8`…`s128`)
- `2`: unsigned integer — payload length is **1, 2, 4, 8, or 16** bytes (`u8`…`u128`)
- `3`: float32 — **4** bytes
- `4`: float64 — **8** bytes
- `5`: byte blob — raw bytes (used for `b"…"` lowering and future static data)
- `6`: bool — **1** byte (`u8` 0/1)

The verifier rejects constant entries whose payload length does not match the primitive kind recorded on the consuming `CONST` instruction operand.

---

## Local layouts section (format minor 1+)

Section kind `6`. Per-function metadata for typed `LOAD_LOCAL` / `STORE_LOCAL`:

Payload begins with `u32 layout_count`, then for each function:

| Field | Size | Meaning |
|---|---:|---|
| function_id | 4 | owning function |
| slot_count | 2 | number of local slots |
| slot_kinds | N | one byte per slot: `0xFF` = aggregate slot; `0`–`12` = [`PrimitiveKind`](#primitive-kind-operands) wire byte |

The verifier uses this table to validate local slot indices and optional stack-kind simulation.

---

## Runtime value model (MVP)

Stack cells and local slots hold either:

- **Width-faithful scalars** — each Phoenix primitive maps to a distinct storage width on the operand stack and in typed local slots (`s32` is 4 bytes, `s128` is 16 bytes, `bool` is 1 byte, `f32`/`f64` are 4/8 bytes). Binary arithmetic/compare opcodes carry a **`prim_kind` operand**; mixed-width stacks are rejected at runtime.
- **Aggregate handles** — indices into the VM aggregate arena (struct, enum, tuple, fixed array, slice).

Raw pointers (`*T`, `&T`, `&mut T`) are **`u64` addresses** with tagged high bits:

- `0x8000…` — address of a local slot in the current frame
- `0x4000…` — address of aggregate storage (for slice data pointers)
- lower range — offset into the VM byte heap (`ALLOC`)

`PTR_LOAD` / `PTR_STORE` dispatch on the tag and use the element **`prim_kind`** operand for width.

Slice values are fat pointers `(data_ptr, len)` stored as an aggregate variant; `MAKE_SLICE` constructs a slice view over an existing fixed array (no heap allocation).

---

## Types section

Types payload begins with `u32 type_count`.

Each type record:

| Field | Size | Meaning |
|---|---:|---|
| type_id | 4 | stable local id |
| kind | 1 | primitive/tuple/array/slice/struct/enum/function |
| flags | 1 | reserved |
| aux_len | 2 | metadata byte length |
| aux_bytes | N | type-specific metadata |

This table supports verifier checks and debug info; execution may rely on pre-lowered opcodes.

---

## Functions section

Functions payload begins with `u32 function_count`.

Each function record:

| Field | Size | Meaning |
|---|---:|---|
| function_id | 4 | unique function id |
| name_symbol_id | 4 | symbol table id (or 0) |
| arity | 2 | parameter count |
| local_count | 2 | local slots |
| stack_max | 2 | max operand stack depth |
| flags | 2 | function flags |
| code_offset | 4 | offset into code section |
| code_len | 4 | byte length of bytecode body |
| return_type_id | 4 | type id for return type |

`entry_function_id` in header must refer to a function whose source signature is `main :: () => ()`.

---

## Code section and instruction encoding

Instruction encoding format:

- `u8 opcode`
- `u8 operand_count`
- `operand_count` operands, each encoded as `u32`

This fixed-width operand unit simplifies MVP decoding.

### MVP opcode families

| Family | Example opcodes |
|---|---|
| Constants/locals | `CONST`, `LOAD_LOCAL`, `STORE_LOCAL`, `POP` |
| Arithmetic | `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `POW`, `NEG` |
| Bitwise/logical | `BIT_AND`, `BIT_OR`, `BIT_XOR`, `SHL`, `SHR`, `NOT`, `AND`, `OR` |
| Compare | `EQ`, `NE`, `LT`, `LE`, `GT`, `GE` |
| Control flow | `JUMP`, `JUMP_IF_TRUE`, `JUMP_IF_FALSE`, `RETURN` |
| Calls | `CALL`, `CALL_INDIRECT` (optional), `RET` |
| Data construction | `MAKE_TUPLE`, `MAKE_ARRAY`, `MAKE_STRUCT`, `MAKE_ENUM`, `MAKE_SLICE` |
| Data access | `GET_FIELD`, `SET_FIELD`, `INDEX` |
| Addressing | `ADDRESS_OF_LOCAL` |
| Pattern helpers | `MATCH_TAG`, `MATCH_INT_RANGE` |
| Std Option/Result helpers (post-MVP) | `MAKE_SOME`, `MAKE_NONE`, `MAKE_OK`, `MAKE_ERR`, `TRY` — only if lowering needs dedicated opcodes after std enums exist |
| Memory intrinsics | `ALLOC`, `PTR_LOAD`, `PTR_STORE` (unsafe boundary) |

Exact opcode numeric assignments are VM-implementation-defined but must remain stable per file format version.

### Primitive kind operands

Several opcodes carry a **`prim_kind` wire byte** (`0`–`12`, see `PrimitiveKind` in the compiler/VM) as the final operand:

| Opcode family | Extra operand | Purpose |
|---|---|---|
| `CONST` | `prim_kind` | Decode constant pool entry at the declared width |
| `LOAD_LOCAL`, `STORE_LOCAL` | `prim_kind` or `0xFF` | Typed scalar load/store vs aggregate slot |
| Arithmetic, bitwise, compare | `prim_kind` | Require matching stack cell widths |
| `NEG`, `NOT`, `BIT_NOT` | `prim_kind` | Unary primitive width |
| `PTR_LOAD`, `PTR_STORE` | `size`, `signed`, `prim_kind` | Memory access width |
| `MAKE_SLICE` | `elem_prim_kind` | Element type of source array |

---

## Verifier/loader invariants (MVP)

Loader must reject bytecode when:

- header magic/version is invalid
- section offsets overlap or exceed file bounds
- function code ranges are out of code section bounds
- jump targets are not aligned to instruction starts
- local slot indexes exceed `local_count`
- stack effect analysis exceeds `stack_max` or underflows
- `entry_function_id` is missing or has non-zero arity

---

## Runtime execution model (MVP)

- one process, one VM instance
- bytecode interpreted by stack machine
- deterministic execution for identical inputs and bytecode
- no actor scheduling obligations in MVP runtime

### MVP interpreter contract (`phx-vm`)

- Production entry is `phx_vm::run` on **verified** bytecode. `run_captured` is `#[doc(hidden)]` for integration tests that scan `main` locals after return.
- Header `entry_function_id` must name a zero-arity `main` for executables. Library objects use `ENTRY_NONE` (`0xFFFF_FFFF`); the VM returns an error if execution is attempted.
- `ConstTag::Bytes` pool entries are not loadable via `Const` in MVP (byte string literals lower to `MakeArray` in the compiler).
- Typeck rejects returning `&T`, `&mut T`, or `[T]` views that borrow function-local bindings; see [`ownership.md`](ownership.md).

---

## Post-MVP runtime architecture targets

These are design targets and not MVP implementation requirements.

### 1) M:N scheduling and worker pool

- VM maintains a fixed worker-thread pool (typically near core count)
- runnable actors are scheduled onto workers; actors are not 1:1 with OS threads
- each actor processes one message at a time, then yields
- actors blocked on I/O are parked until readiness events wake them

### 2) Bytecode portability

- compiler emits Phoenix bytecode as the distribution artifact
- same bytecode payload runs on Linux/macOS/Windows wherever compatible VM version exists
- deployment portability comes from stable bytecode + VM format versioning

### 3) Hot code reloading

- VM tracks active code versions for loaded modules
- running handlers finish on their current version boundary
- new messages dispatch to the newly loaded compatible module version
- compatibility metadata/version gates are required for safe reload rollout

### 4) JIT and profiling

- interpreter gathers lightweight execution profile data
- VM may compile hot bytecode regions to native code
- cold code remains interpreted to preserve startup and portability benefits
- deopt/fallback path returns execution to interpreter when needed

### 5) I/O and syscall abstraction

- VM wraps OS networking, file I/O, and timers behind runtime interfaces
- safe user I/O never blocks worker threads; calls park the current context instead
- scheduler receives readiness signals and resumes parked contexts
- raw blocking syscalls are forbidden on safe I/O paths
- `AWAIT_IO` and related opcodes implement what **schedulable-I/O types** promise at the language layer — see [runtime-transparency.md](runtime-transparency.md)

#### Execution context state machine

Each scheduler-managed context is in exactly one state:

| State | Meaning |
|---|---|
| `Running` | actively executing bytecode on a worker |
| `ParkedAwaitIO` | waiting for file/network/timer readiness |
| `ParkedAwaitMessage` | waiting for mailbox delivery (explicit actors) |
| `Done` | finished; resources eligible for reclamation |

Wakeup sources:

- I/O readiness (epoll/kqueue/IOCP or equivalent)
- mailbox message delivery
- timers (future)

Invariant: worker threads must never block on user-level I/O in safe paths.

#### Post-MVP scheduler/I/O opcode families

| Opcode family | Purpose |
|---|---|
| `PARK` | transition context to parked state |
| `RESUME` | mark context runnable after wakeup |
| `AWAIT_IO` | initiate non-blocking I/O and park until ready |
| `ENQUEUE_MAILBOX` | move message into actor mailbox |
| `DEQUEUE_MAILBOX` | take owned message from mailbox |
| `SPAWN_CONTEXT` | create explicit actor context |

MVP bytecode does not include these opcodes; they are added via format versioning when the scheduler ships.

### 6) Supervision and lifecycle ownership

- VM owns actor spawn/despawn transitions and mailbox registration
- supervisor policies (restart/escalate/stop) execute as runtime decisions
- mailbox queues, process state, and restart metadata are VM-managed runtime data

### 7) Crash isolation

- actor failure is trapped as an actor-scoped runtime fault
- VM notifies supervisor chain and tears down failed actor resources
- other actors continue unless escalation policy requires broader shutdown

---

## Post-MVP notes

Future versions can add:

- actor opcodes and mailbox scheduling hooks
- ownership/borrow verification metadata
- JIT metadata sections
- hot-reload compatibility metadata

These must be added through format versioning.
