# VM Concurrency Runtime Options

Status: Research (informing Runtime v1 implementation)

**Purpose:** Evaluate how Phoenix should implement concurrency in `phx-vm` — whether to adopt an external async runtime (Tokio, smol, etc.), use only `std::thread`/`std::sync`, or build a custom M:N scheduler in-tree — aligned with Phoenix design goals and the **no crates.io dependencies** policy.

**Related documents:**

| Document | Role |
|----------|------|
| [concurrency.md](../features/concurrency.md) | Post-MVP design target (M:N, schedulable I/O, actors) |
| [runtime-transparency.md](../features/runtime-transparency.md) | Pure vs schedulable I/O vs actor boundaries |
| [vm-linear.md](../features/vm-linear.md) | Bytecode contract; post-MVP `PARK` / mailbox opcodes |
| [concurrency-models-research.md](concurrency-models-research.md) | Cross-language survey and stress-test of Phoenix choices |
| [language-v0.md](../language-v0.md) | Shipping order: Language v0 → Std v0 → **Runtime v1** → Std I/O → Actors |

---

## Executive summary

| Question | Recommendation |
|----------|----------------|
| **Tokio / smol / async-std in `phx-vm`?** | **No** for the interpreter scheduler. These are I/O/async *Future* runtimes, not CPU-bound bytecode schedulers. |
| **Can concurrency work with only `std`?** | **Yes.** BEAM, many game engines, and WASM hosts use custom M:N schedulers built on OS threads + queues + park/wake. |
| **MVP (now)?** | Keep the **single-threaded** stack interpreter. Introduce scheduler *types* and a **reduction counter** in the interpret loop as structural prep — no worker pool yet. |
| **Runtime v1?** | **Custom in-tree M:N scheduler:** fixed `std::thread` worker pool, per-worker run queues, `mpsc` for cross-thread wakeups, `VecDeque` mailboxes per actor, cooperative parking via context state machine. |
| **Where async runtimes *do* fit?** | **Host/embedder boundary only** — e.g. a Phoenix program embedded in a Tokio web server parks actors and resumes via channels; the VM core stays synchronous. |
| **If dependency policy relaxes later?** | Consider **Crossbeam** deque/channels first (scheduler primitives only). **May** (stackful coroutines) is closer to goroutines than Tokio but still less controlled than a BEAM-style actor scheduler. **Do not adopt async-std** (discontinued, RUSTSEC-2025-0052). |

**Bottom line:** Phoenix already rejected `async`/`await` in the language. Pulling Tokio into the VM would reintroduce async scheduling *inside* the runtime under a different name. The right model is **BEAM-style processes on a custom scheduler**, implemented with `std` until there is a strong reason to vendor or allow a minimal deque crate.

---

## 1. What Phoenix is trying to achieve

### 1.1 Design commitments (post-MVP)

From [concurrency.md](../features/concurrency.md):

- **No `async`/`await`** — concurrency is VM-managed, not user-polled Futures.
- **Everything scheduler-managed** — even `main` runs in an implicit root execution context.
- **Pure sequential code** runs to completion without implicit suspension.
- **Schedulable I/O** parks contexts cooperatively (`ParkedAwaitIO`); types make suspension visible at call sites.
- **Explicit actors** (`@spawn`, `@send`, …) are opt-in for isolation and supervision.
- **M:N mapping:** fixed worker pool (~CPU cores), many execution contexts; actors handle one message then yield.
- **Blocking I/O forbidden on safe paths** — raw blocking only behind `#unsafe`/FFI.

### 1.2 Shipping order

From [language-v0.md](../language-v0.md):

```
Language v0  →  Std v0  →  Runtime v1 (M:N + schedulable I/O)  →  Std I/O  →  Actors
```

Runtime v1 is **after** the compiler and std bootstrap are solid. MVP and Language v0 intentionally ship **without** a scheduler.

### 1.3 Bytecode contract (future)

[vm-linear.md](../features/vm-linear.md) reserves post-MVP opcode families: `PARK`, `RESUME`, `AWAIT_IO`, `ENQUEUE_MAILBOX`, `DEQUEUE_MAILBOX`, `SPAWN_CONTEXT`. Execution context states: `Running`, `ParkedAwaitIO`, `ParkedAwaitMessage`, `Done`.

MVP bytecode (51 opcodes today) does **not** include these; they arrive via format versioning when Runtime v1 ships.

### 1.4 What “concurrency” means for Phoenix

Phoenix needs **two different mechanisms**, not one generic “async runtime”:

| Mechanism | Purpose | Scheduler involvement |
|-----------|---------|------------------------|
| **Fair CPU sharing** | Many actors/contexts on few OS threads | Run queues, reduction counting, work stealing |
| **I/O readiness** | Park until fd/socket/timer ready | Reactor or host callback → resume parked context |

Tokio excels at the second problem on the **host**. Phoenix must own the first problem in the **VM** because it is tied to bytecode execution, actor mailboxes, and Phoenix-specific isolation semantics.

---

## 2. Current implementation (baseline)

### 2.1 `phx-vm` today

| Aspect | State |
|--------|--------|
| Execution | Single-threaded `while` interpret loop until `main` returns |
| Structure | `ExecutionContext` (stack + frames) + `VmRuntime` (heap, aggregates) + `Machine` facade |
| Threads | None in scheduler sense; `Mutex` on global foreign stub registry only |
| I/O | Pre-scheduler **blocking** `phoenix_write_stdout` foreign stub |
| Actors / mailboxes | Not implemented |
| Opcodes | No `PARK`, `AWAIT_IO`, `SPAWN_CONTEXT`, etc. |

`context.rs` documents the intended split:

> MVP: one `ExecutionContext` per `run()`  
> Post-MVP: scheduler owns a pool of contexts

This is naming/structure for Runtime v1 — no scheduling logic exists yet.

### 2.2 Compiler assumptions

Codegen emits **synchronous** PHX0. `@spawn` / `@send` / `@receive` / `@reply` parse but typeck rejects them as unsupported. Foreign calls use `CallIndirect` with VM-hosted stubs.

### 2.3 Dependency policy (hard constraint)

CI enforces **no crates.io dependencies** (`just dep-check` → `tests/ci/check-deps.sh`). Workspace members may only use `path =` or `workspace =` deps.

`phx-vm` depends only on `phx-bytecode` and `phx-diagnostics`.

**Implication:** Tokio, smol, Crossbeam, and May are **out of scope** unless the policy is explicitly changed or a crate is vendored in-tree (same ownership burden as writing it yourself).

---

## 3. Option analysis

### 3.1 Single-threaded interpreter (MVP / Phase 0)

**Model:** Lua default, V8 isolate default — one OS thread, one VM instance, synchronous loop.

| Pros | Cons |
|------|------|
| Simplest correct implementation | No parallel actors |
| Best debuggability | Blocking host I/O stalls everything |
| Zero scheduler bugs | Must not paint into corner for M:N later |
| Matches MVP design lock | |

**Verdict:** **Correct for now.** Prove bytecode, verifier, ownership at runtime, foreign ABI. Design scheduler types early; implement scheduler later.

**Prep work that pays off:**

- `ActorId`, `ContextId`, `ContextState` enums (even if only one context runs).
- **Reduction counter** in the interpret loop (BEAM-style fairness hook).
- Keep `ExecutionContext` separate from `VmRuntime` (already done).

---

### 3.2 `std::thread` + `std::sync` — custom M:N scheduler (recommended for Runtime v1)

**Model:** BEAM schedulers, Erlang OTP — OS thread pool + many lightweight contexts scheduled cooperatively.

#### Architecture sketch

```
┌────────────────────────────────────────────────────────────┐
│  Scheduler (in phx-vm)                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │ Worker 0 │  │ Worker 1 │  │ Worker N │   OS threads     │
│  │ run queue│  │ run queue│  │ run queue│                  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                  │
│       └──────── work steal ───────┘                        │
│  Global: mpsc for schedule/wake commands                   │
│  Per-actor: VecDeque<Message> mailbox (single consumer)    │
│  I/O: park registry + host/epoll thread (later)            │
└────────────────────────────────────────────────────────────┘
```

#### std primitives map

| Component | std API | Notes |
|-----------|---------|-------|
| Worker threads | `std::thread::spawn` | `available_parallelism()` for default count |
| Run queue (v1) | `Mutex<VecDeque<ContextId>>` per worker | Simple; optimize later |
| Cross-thread wake | `mpsc::Sender<ScheduleMsg>` | Inject runnable context on target worker |
| Park / unpark | `Condvar` + `Mutex` or channel command | Context removed from run queue while parked |
| Mailboxes | `VecDeque` + mutex per actor | BEAM-style; selective receive in VM logic |
| Timers | `BinaryHeap<(Instant, TimerId)>` on scheduler | Can defer to Phase 2 |
| Fairness | Reduction budget per timeslice | Count opcodes in interpret loop |

#### Fit against Phoenix goals

| Goal | Fit |
|------|-----|
| M:N green threads | ★★★★★ — contexts are VM structs, not OS threads |
| Cooperative I/O parking | ★★★★ — state machine + wakeup injection |
| Actor mailboxes | ★★★★★ — natural per-actor queues |
| No `async`/`await` | ★★★★★ — synchronous interpreter, explicit park points |
| Embeddability | ★★★★★ — no global Tokio runtime |
| Debuggability | ★★★★★ — plain stacks, no Future state machines |
| Stdlib-only policy | ★★★★★ — only `std` |

#### What you must build (and own)

| Piece | Effort | Risk |
|-------|--------|------|
| Runnable queue + round-robin | Low | Low |
| Context state machine | Medium | Medium |
| Reduction counting | Low | Low |
| Work stealing | High | Medium — study Crossbeam deque algorithm; can ship without stealing first |
| Cross-thread mailbox delivery | Medium | Medium |
| I/O reactor integration | High | High — platform-specific; may delegate to host initially |
| Supervision trees | Medium | Semantic complexity |

**Verdict:** **Primary path for Runtime v1.** Aligns with design docs, dependency policy, and BEAM research already in [concurrency-models-research.md](concurrency-models-research.md).

---

### 3.3 Tokio

**What it is:** Multi-threaded async executor + I/O driver (`mio`) scheduling `Future::poll` at `.await` points.

| Dimension | Assessment |
|-----------|------------|
| M:N for bytecode | ★★ — tasks are Futures, not interpreter loops |
| CPU-bound interpreter | Poor — long poll loops starve the executor unless you `yield_now().await` every N opcodes |
| Actor mailboxes | Possible via `mpsc` + tasks, but you still design actor semantics |
| Dependency weight | Large tree; **fails dep-check** |
| Philosophy | Conflicts with “no async in language” — reintroduces async inside VM |
| Debuggability | Async backtraces, nested poll states |

**When Tokio *is* appropriate:**

```
┌─────────────────┐     mpsc / oneshot      ┌─────────────────┐
│  Host service   │ ◄──────────────────────► │  phx-vm workers │
│  (Tokio runtime)│   Resume(actor, result)  │  (sync threads) │
└─────────────────┘                          └─────────────────┘
```

- Embed Phoenix in an async server.
- Host performs `read`/`write` on Tokio; VM actor parks; completion message resumes context on a **dedicated VM worker thread** (`spawn_blocking` or a long-lived thread pool **outside** the interpreter loop).

**Verdict:** **Do not use Tokio as the VM scheduler.** Optional at embedder boundary only.

---

### 3.4 smol

Lighter modular async stack (`async-io`, `async-task`). Same fundamental mismatch as Tokio: Future polling, not opcode interpretation. Smaller dependency tree but still **external** and wrong abstraction layer for the interpret hot loop.

**Verdict:** Same as Tokio — host boundary only, if ever.

---

### 3.5 async-std

**Discontinued** (August 2025, RUSTSEC-2025-0052). Do not adopt.

---

### 3.6 May / may_executor (stackful coroutines)

**What it is:** Go-like **stackful** coroutines with M:N scheduling — closer to language-runtime needs than Tokio.

| Pros | Cons |
|------|------|
| Built-in M:N | External dep (`generator-rs`, platform asm) |
| Familiar goroutine model | Stack overflow risk without tuning |
| Less scheduler code than from scratch | Less control over actor/mailbox semantics |
| | Still not BEAM — you own Phoenix isolation rules |

**Verdict:** Reasonable **if dependency policy relaxes** and you want to defer scheduler engineering. Still inferior to in-tree BEAM-style design for actor isolation, supervision, and move-based messages. **Not recommended** while std-only policy holds.

---

### 3.7 glommio (thread-per-core, io_uring)

Linux-focused, tasks pinned to cores, `io_uring`-centric. Poor fit for portable Phoenix VM and general M:N actor model. Niche for high-throughput Linux-only I/O servers.

**Verdict:** **No** for general language runtime.

---

### 3.8 Crossbeam (if policy relaxes)

| Crate | VM use |
|-------|--------|
| `crossbeam-deque` | Work-stealing scheduler — highest ROI |
| `crossbeam-channel` | MPMC routing, `select!`-style wakeups |
| `crossbeam-queue` | Lock-free injection |

Crossbeam replaces **queue/channel primitives** only. Actor semantics, reduction counting, supervision, and bytecode integration remain in-tree.

**Verdict:** First external crate to consider **if** dep-check policy changes. Alternatively, **reimplement a minimal stealing deque** from published algorithms (Chase-Lev) under `phx-vm/src/scheduler/` — more work, keeps policy.

---

## 4. How other VMs handle this

| Runtime | Model | Scheduler | Lesson for Phoenix |
|---------|-------|-----------|-------------------|
| **Erlang BEAM** | M:N processes | Per-scheduler run queues; **~4000 reduction** preempt; work stealing between schedulers | **Primary reference** |
| **JVM + Loom** | Virtual threads on carrier pool | Mount/unmount on few platform threads | Reduction model simpler than full OS preemption |
| **Lua** | Single thread; coroutines cooperative | No OS parallelism | **MVP reference** |
| **V8** | Isolate = single-threaded | Microtasks at host boundary | Embeddability: one instance ↔ one thread |
| **WASM** | Single-threaded default | Host imports for I/O | MVP simplicity; threads proposal = shared memory + atomics (heavy) |
| **Go** | Goroutines + runtime scheduler | M:N in runtime, hidden from user | Phoenix explicitly **rejects** hidden blocking |

### BEAM reduction loop (mapping to Phoenix)

```text
loop {
  reductions = 0;
  while reductions < MAX_REDUCTIONS {
    opcode = fetch();
    execute(opcode);
    reductions += cost(opcode);
    if must_yield() { break; }
  }
  enqueue_for_reschedule(current_context);
  run_next_runnable();
}
```

Phoenix can add `MAX_REDUCTIONS` to the existing interpret loop **now** (no-op with one context) and activate fairness when M:N ships.

---

## 5. Master comparison table

| Approach | CPU bytecode | Fair M:N | Actor mailboxes | Cooperative I/O | stdlib-only | VM debuggability | **Total fit** |
|----------|-------------|----------|-----------------|-----------------|-------------|------------------|---------------|
| **Custom std M:N** | ★★★★★ | ★★★★ | ★★★★★ | ★★★★ | ★★★★★ | ★★★★★ | **Best** |
| **Single-threaded MVP** | ★★★★★ | — | ★ (local only) | ★★ | ★★★★★ | ★★★★★ | **MVP** |
| **Crossbeam + custom** | ★★★★★ | ★★★★★ | ★★★★★ | ★★★★ | ✗ | ★★★★ | Strong if deps OK |
| **May** | ★★★★ | ★★★★ | ★★★★ | ★★★★ | ✗ | ★★★ | Possible fallback |
| **Tokio / smol** | ★★ | ★★★ | ★★★ | ★★★★★ | ✗ | ★★ | **Wrong layer** |
| **glommio** | ★★★ | ★★ | ★★ | ★★★★★ (Linux) | ✗ | ★★★ | Niche |

---

## 6. Recommended phased roadmap

### Phase 0 — MVP / Language v0 (current)

- Single-threaded interpreter; verify-then-run.
- Structural split: `ExecutionContext` / `VmRuntime` / `Machine` (done).
- Blocking foreign stubs only (`phoenix_write_stdout`).
- **Add:** reduction counter in interpret loop (configurable, default “unlimited” for MVP).
- **Add:** `scheduler/` module with traits and stub implementation (`SingleThreadScheduler`).

### Phase 1 — Cooperative actors (one OS thread)

- `spawn` creates actor contexts; round-robin runnable set on one thread.
- `receive` parks actor until mailbox non-empty.
- Mailboxes: `VecDeque` + move semantics per [messages.md](../features/messages.md).
- Supervision tree data structures (no multi-thread yet).
- New bytecode opcodes behind format version bump.

### Phase 2 — M:N worker pool (`std` only)

- `N = available_parallelism()` worker threads (embedder-configurable).
- Per-worker run queues; optional work stealing (v2).
- Cross-thread `@send`: `mpsc` inject to owning worker.
- Context states: `Running`, `ParkedAwaitIO`, `ParkedAwaitMessage`, `Done`.

### Phase 3 — Schedulable I/O

- `AWAIT_IO` parks context; **host or in-tree reactor** completes and sends resume.
- Option A: dedicated I/O thread + `poll`/`epoll`/`kqueue` (platform modules in `phx-vm`).
- Option B: embedder provides readiness (`VmHost::register_io_interest`).
- Std `File.read` ships **after** this layer exists.

### Phase 4 — Host embedding (optional Tokio *outside* VM)

- Document embedder pattern: VM worker threads + Tokio for socket/http.
- `phx` CLI may stay single-threaded or use minimal std I/O thread.

---

## 7. What to implement in-tree (module sketch)

```
source/phx-vm/src/
  scheduler/
    mod.rs           # Scheduler trait, SchedulerConfig
    single.rs        # Phase 0–1: one thread
    worker.rs        # Phase 2: worker loop
    queue.rs         # RunQueue (Mutex<VecDeque> → stealing deque later)
    context.rs       # ContextState, park/unpark
    mailbox.rs       # Per-actor VecDeque, back-pressure policy
    reduction.rs     # Reduction budget policy
  reactor/           # Phase 3 (optional, cfg-gated)
    mod.rs
    unix.rs          # epoll/kqueue
  interpreter/       # existing; calls scheduler on yield points
```

**Public API evolution:**

- MVP: `phx_vm::run(module)` — unchanged.
- Runtime v1: `Vm::new(config)`, `Vm::spawn_bytecode(...)`, `Vm::run_until_idle()` for embedders.

Keep a **single-threaded fast path** for tests and `phx run` simple programs.

---

## 8. Anti-patterns

| Anti-pattern | Why avoid |
|--------------|-----------|
| Interpreter inside `async fn` without yielding | Starves host executor |
| One OS thread per actor at scale | Doesn't scale; BEAM rejected this |
| Tokio “because we'll need async I/O” | Wrong layer; defer to host or Phase 3 reactor |
| Preemptive OS signals before cooperative works | 10× complexity for little gain |
| Shared mutable `Machine` across threads | Data races; breaks actor isolation |
| Adding crates.io dep without policy change | CI fails; undermines ownership goal |
| `async-std` | Discontinued |

---

## 9. Policy decision: keep std-only or relax?

| Keep std-only | Relax for scheduler primitives |
|---------------|-------------------------------|
| Full control and auditability | Faster path to work-stealing deque |
| Matches current CI and philosophy | Crossbeam is battle-tested |
| BEAM proves M:N doesn't need crates | Still need all Phoenix-specific semantics in-tree |
| More code to write and test | Another supply-chain surface |

**Recommendation:** **Stay std-only through Phase 2** with simple `Mutex` run queues. Revisit Crossbeam **only if** profiling shows scheduler overhead dominates and a minimal in-tree deque is not worth the effort.

Tokio/smols should remain **out of scope** for `phx-vm` even if policy relaxes — wrong abstraction.

---

## 10. Open questions

1. **Preemption:** Reduction counting only, or periodic interrupt? (BEAM: reductions only.)
2. **Work stealing:** Required at M:N launch, or round-robin first?
3. **I/O reactor:** In-tree per-platform vs embedder-only for Runtime v1?
4. **Foreign calls:** `spawn_blocking` pool for `#unsafe` FFI, or stall worker? (Design: document stall risk; pool is likely needed.)
5. **Heap isolation:** Per-actor heap vs shared `VmRuntime` heap — ties to [ownership.md](../features/ownership.md) and actor design.
6. **Format versioning:** Opcode addition strategy for `PARK` / mailbox family.
7. **Single-threaded mode forever?** Keep for `phx test`, WASM target, deterministic replay?

---

## 11. Conclusion

Phoenix does **not** need Tokio or a lightweight async runtime inside the VM to achieve its concurrency goals. The language deliberately avoids `async`/`await`; the runtime should avoid Future-based scheduling for the bytecode interpreter.

**Concurrency is not “threads vs async” for Phoenix — it is a custom M:N actor scheduler** with:

- cooperative parking at typed I/O and mailbox boundaries,
- reduction-based fairness,
- message-passing isolation,
- optional host async at the **embedder edge**.

**For the bare-bones VM you are building now:** stay single-threaded, keep dependencies at zero, add reduction counting and scheduler module stubs, and implement BEAM-style scheduling in `std` when Runtime v1 begins. That is the path that matches the design docs, CI policy, and long-term ownership of the runtime.

---

## 12. References

- [concurrency.md](../features/concurrency.md) — Phoenix post-MVP concurrency design
- [vm-linear.md](../features/vm-linear.md) — execution context states and post-MVP opcodes
- [concurrency-models-research.md](concurrency-models-research.md) — language survey (§8 VM architecture)
- [io-bridge.md](../features/io-bridge.md) — pre-scheduler blocking stdout stub
- `source/phx-vm/src/context.rs` — `ExecutionContext` / `VmRuntime` split
- `tests/ci/check-deps.sh` — no crates.io enforcement
