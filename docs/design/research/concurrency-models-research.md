# Concurrency Models Research for Phoenix

Status: Research (informing post-MVP runtime design)

**Purpose:** Survey how major languages handle concurrency, what developers love and hate about each approach, and what Phoenix can uniquely offer by combining Rust-like ownership safety with a bytecode VM and managed runtime — **without garbage collection**.

This document is research, not a design lock. Phoenix's current direction lives in [concurrency.md](../features/concurrency.md), [runtime-transparency.md](../features/runtime-transparency.md), and [messages.md](../features/messages.md). Use this file to stress-test those decisions and surface alternatives.

**Implementation options (Tokio vs std vs custom scheduler):** [vm-concurrency-runtime-options.md](vm-concurrency-runtime-options.md)

---

## Table of contents

1. [Executive summary](#1-executive-summary)
2. [Phoenix constraints and positioning](#2-phoenix-constraints-and-positioning)
3. [The three classical models (and hybrids)](#3-the-three-classical-models-and-hybrids)
4. [Language-by-language survey](#4-language-by-language-survey)
5. [Cross-cutting pain points](#5-cross-cutting-pain-points)
6. [What Phoenix can do that others cannot](#6-what-phoenix-can-do-that-others-cannot)
7. [Type system integration](#7-type-system-integration)
8. [VM and runtime architecture](#8-vm-and-runtime-architecture)
9. [Metaprogramming and compile-time concurrency](#9-metaprogramming-and-compile-time-concurrency)
10. [Tooling, LSP, and developer experience](#10-tooling-lsp-and-developer-experience)
11. [Recommendations and open questions](#11-recommendations-and-open-questions)
12. [References](#12-references)

---

## 1. Executive summary

### The landscape in one table

| Language / runtime | Concurrency unit | Scheduling | I/O model | Isolation default | Memory model |
|---|---|---|---|---|---|
| **Java (platform threads)** | OS thread (1:1) | OS kernel | Blocking per thread | None | JMM + GC |
| **Java (virtual threads)** | Virtual thread (M:N) | JVM carrier pool | Blocking-looking, runtime parks | None | JMM + GC |
| **C# async/await** | `Task` state machine | ThreadPool + IOCP | `await` on async APIs | None | CLR + GC |
| **C/C++** | OS thread / coroutine frame | Manual / library | Blocking syscalls | None | UB on data races |
| **Zig** | `Io` tasks | Thread pool or event loop | Explicit `Io` parameter | Manual | Programmer discipline |
| **Odin** | OS thread | Manual pools | Blocking | Manual locks | Futex sync, no runtime |
| **Rust** | `Future` or `std::thread` | Tokio M:N or OS | Reactor + `spawn_blocking` | `Send`/`Sync` bounds | Compile-time + ownership |
| **Go** | Goroutine | M:P:G + netpoller | Hidden park on network I/O | Shared memory | GC; races possible |
| **Erlang/Elixir** | BEAM process | Preemptive per scheduler | Ports/drivers | Process isolation | Per-process GC |
| **Phoenix (target)** | Execution context | M:N VM scheduler | Type-visible schedulable I/O | Opt-in actors (`@spawn`) | Ownership, no GC |

### Core thesis

Decades of JVM, .NET, Go, and Rust async experience converge on a few durable lessons:

1. **M:N scheduling with integrated I/O polling** is the right default for scalable I/O (Go netpoller, Java Loom, Tokio).
2. **Hidden suspension** (Go) and **function coloring** (`async`/`await`, Kotlin `suspend`) are the two dominant failure modes for developer trust.
3. **GC pauses** and **thread-pool starvation** are the managed-runtime taxes that native/no-GC systems avoid — but native systems pay in **UB, lifetime bugs, and manual runtime construction**.
4. **Actor isolation** (BEAM) solves fault domains but is heavy ceremony for simple I/O.

Phoenix's documented bet is a **fourth column**: VM-managed M:N scheduling, **type-visible** schedulable I/O (no `async`/`await`), **opt-in** move-based actors, and **compile-time ownership** without GC. No existing language occupies that exact niche.

---

## 2. Phoenix constraints and positioning

### What Phoenix already decided (design authority)

From [concurrency.md](../features/concurrency.md), [runtime-transparency.md](../features/runtime-transparency.md), [language-v0.md](../language-v0.md):

| Decision | Rationale |
|---|---|
| No `async`/`await` | Avoid function coloring and manual poll/future complexity |
| Everything scheduler-managed (post-MVP) | Even `main` bootstraps into an implicit root execution context |
| Type-visible schedulable I/O | Honest call sites; unlike Go's hidden goroutine blocking |
| Opt-in actors (`@spawn`, `@send`, `@receive`, `@reply`) | Erlang-like isolation without process ceremony for every read |
| No GC | Memory safety via ownership, moves, borrowing (phased) |
| Portable PHX0 bytecode + VM | Distribution artifact; VM owns scheduling, parking, mailboxes |
| Scheduler before std I/O | Blocking on worker threads stalls M:N — must not ship safe I/O without cooperative park |

### What is not built yet

| Area | Status |
|---|---|
| M:N scheduler | Documented; no `scheduler.rs` in `phx-vm` |
| Schedulable-I/O type syntax | TBD |
| Actor opcodes (`PARK`, `AWAIT_IO`, `SPAWN_CONTEXT`, …) | Documented in [vm-linear.md](../features/vm-linear.md); not in MVP bytecode |
| `@` directive semantics | Parsed; typeck rejects as `UnsupportedFeature` |
| Actor trait / protocol checking | Future |

### Phoenix's unique constraint set

Phoenix is not "Rust with a VM" or "Go without GC" or "Erlang with types." The intersection is rare:

```
        Memory safety (ownership, no GC)
                    │
    ┌───────────────┼───────────────┐
    │               │               │
  Rust           Phoenix          Erlang
  (no VM         (VM + ownership   (VM + actors
   scheduler)     + no GC)         + GC)
    │               │               │
    └───────────────┼───────────────┘
                    │
           Managed runtime (M:N, I/O park, actors)
```

**Implication:** Design choices must be validated against *all three* axes simultaneously. A pattern that works in Go (shared mutable state + GC) or Java (virtual threads + synchronized pinning) may fail under Phoenix's ownership + no-GC + worker-pool invariants.

---

## 3. The three classical models (and hybrids)

The following evaluates OS threads, green threads/coroutines, and actors as **building blocks** — not as mutually exclusive choices. Phoenix's design already hybridizes them.

### 3.1 OS threads (1:1)

**Mechanism:** Each concurrent task maps to a kernel thread with its own stack (typically 1–8 MB reserved).

| Pros | Cons |
|---|---|
| Simple mental model; maps directly to OS scheduler | ~1 MB+ stack per thread; practical ceiling ~10k–20k concurrent tasks |
| Preemptive; CPU-bound work cannot starve the pool indefinitely | Expensive context switches (microseconds) |
| Straightforward blocking I/O per thread | Poor fit for millions of lightweight tasks |
| Easy FFI and native library interop | Thread pool sizing is a dark art |
| Excellent debugger/profiler support | Under M:N, blocking an OS thread assigned to a worker stalls scheduling |

**Community sentiment:** Loved for simplicity and debuggability. Hated for pool exhaustion, artificial concurrency caps (e.g. `maxThreads=200`), and "why is my server queuing requests when CPU is idle?"

**Phoenix mapping:** OS threads are the **worker pool substrate**, not the programmer-facing concurrency unit. Safe Phoenix code should not expose raw `pthread`-style APIs on schedulable paths. Blocking syscalls belong behind `#unsafe`/FFI with documented stall risk ([compiler-directives.md](../features/compiler-directives.md)).

### 3.2 Green threads / coroutine runtime (M:N)

**Mechanism:** Many lightweight tasks multiplexed on a small OS thread pool. Tasks park at I/O or yield points; scheduler resumes them on any available worker.

| Pros | Cons |
|---|---|
| Millions of concurrent tasks; stacks are KB-scale | Requires full runtime: scheduler, stack management, wakeup |
| Cheap context switch (hundreds of ns) | Must integrate non-blocking I/O or dedicated blocking thread pool |
| Deterministic stack size control | Preemption is hard (cooperative-only risks CPU hogging) |
| Good fit without `async`/`await` syntax | Stack growth / migration across workers needs careful design |
| Efficient sync primitives at runtime level | FFI and blocking libraries can "pin" carriers (Java Loom lesson) |

**Exemplars:** Go goroutines, Java virtual threads (Loom), Erlang processes (different isolation model), Rust Tokio tasks (stackless, not identical).

**Community sentiment:**

- **Go:** "Just use `go fn()`" — beloved ergonomics; hated hidden blocking (`os.File` vs `net.Conn`), goroutine leaks, shared-state races.
- **Java Loom:** "Make concurrency boring again" — beloved imperative style; hated pinning inside `synchronized`, ThreadLocal explosion at scale, unbounded concurrency without backpressure.

**Phoenix mapping:** This is the **default execution model** for all user code. Execution contexts are VM-managed, not OS-thread-per-task. Key differentiator: schedulable I/O is **type-visible**, not hidden like Go.

### 3.3 Actors (language-level, on any scheduler)

**Mechanism:** Isolated units communicate via asynchronous message passing. State is not shared; failure is contained per actor.

| Pros | Cons |
|---|---|
| Natural message-passing; avoids data races by construction | Requires adopting actor patterns for isolation benefits |
| Fault isolation and supervision hierarchies (BEAM/OTP) | Mailbox backpressure, protocol design, serialization semantics |
| Distribution-friendly (process boundaries map to nodes) | Implementation complexity: mailboxes, routing, correlation |
| Matches supervision and long-lived service designs | Overkill for simple `File.read`-style operations |
| Move-based messaging aligns with ownership (Phoenix) | Runtime cost of protocol validation vs dynamic Erlang |

**Community sentiment:**

- **Erlang/Elixir:** Loved for uptime, "let it crash," OTP patterns. Hated for copying costs on large messages, GenServer boilerplate, NIF blocking footguns.
- **Akka (JVM):** Loved for enterprise actor model; hated for complexity, lifecycle management, and interaction with colored `Future` APIs.

**Phoenix mapping:** **Opt-in** via `@spawn`/`@send`/`@receive`. Not required for schedulable I/O. Move semantics at `@send` ([messages.md](../features/messages.md)) — more efficient than BEAM deep copy, requires static protocol typing.

### 3.4 Hybrid model matrix

Phoenix intentionally combines layers:

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: Explicit actors (@spawn, supervision, mailboxes)  │  opt-in
├─────────────────────────────────────────────────────────────┤
│  Layer 2: Schedulable I/O (type-visible park at call sites) │  default for std I/O
├─────────────────────────────────────────────────────────────┤
│  Layer 1: M:N scheduler + execution contexts                │  always (post-MVP)
├─────────────────────────────────────────────────────────────┤
│  Layer 0: OS thread worker pool                           │  VM substrate
└─────────────────────────────────────────────────────────────┘
```

| Concern | Mechanism | Required? |
|---|---|---|
| Running `main` and normal functions | Implicit scheduler context | Always |
| File/network/timer waits | Schedulable I/O (typed) | Default for safe std I/O |
| Fault isolation / message protocols | Explicit actors | Opt-in |
| Raw FFI / blocking syscalls | `#unsafe` | Escape hatch |

This rejects false dichotomies like "actors *or* green threads" or "async *or* threads."

---

## 4. Language-by-language survey

### 4.1 Java and the JVM

#### Platform threads (historical default)

- **1:1 OS mapping**, ~1 MB stack, kernel-scheduled.
- Thread-per-request was the dominant server pattern for decades.
- **Pain:** `maxThreads=200` caps throughput; 10k threads ≈ 10 GB stacks; blocking I/O wastes OS threads.

#### Virtual threads (Project Loom, Java 21+)

- Heap-allocated threads mounted on **carrier** (platform) threads.
- Blocking I/O unmounts virtual thread; carrier freed for other work.
- **Loved:** Imperative code, `try/catch`, clean stacks, "concurrency boring again."
- **Hated:**
  - **Pinning:** `synchronized` + blocking I/O pins carrier → carrier pool exhaustion (Netflix production rollback).
  - **ThreadLocal explosion:** millions of short-lived VTs × ThreadLocal caches = GB memory.
  - **Unbounded concurrency:** cheap to create, expensive to complete without semaphores.
  - **CPU-bound misuse:** VTs don't accelerate CPU work.

**Lesson for Phoenix:** Pinning equivalents exist — holding VM locks or `#unsafe` blocking calls across park points can stall the worker pool. Document and statically discourage these patterns.

#### CompletableFuture and reactive streams

- **CompletableFuture:** Composable but callback-heavy; fragmented stack traces; `.exceptionally()` instead of `try/catch`.
- **Reactive (Reactor/RxJava):** Extreme throughput potential; **gigantic unreadable stack traces** are the #1 team complaint.

> *"Stacktraces for errors wind up being just absolutely gigantic… figuring out the path through the stream calls is pretty darn difficult."* — common Reactor complaint

**Lesson:** Stack trace quality is an adoption killer. Phoenix VM should preserve synchronous-looking stacks across park/resume.

#### Kotlin coroutines (JVM alternative)

- `suspend` functions = explicit coloring; structured concurrency via scopes.
- **vs Loom:** Coroutines are cooperative at `suspend` points; Loom threads are preemptible on carriers.
- Kotlin team keeps `suspend` intentionally — signals suspension at call sites.

**Phoenix parallel:** Kotlin proves marking works; Loom proves hiding scales. Phoenix uses **types** instead of `suspend` or hidden runtime.

---

### 4.2 C# and .NET

#### async/await + Task

- Compiler rewrites `async` methods to state machines.
- **Loved:** Near-synchronous syntax; deep ASP.NET/EF integration; `try/catch` with async.
- **Hated — colored functions** (Bob Nystrom, *What Color is Your Function?*, 2015):

> *"You've still divided your entire world into asynchronous and synchronous halves… Async-await solves annoying rule #4 [calling async from sync is painful]. But all of the other rules are still there."*

- **Viral asynchrony:** David Fowler — *"Once you go async, all of your callers SHOULD be async."*
- **Sync-over-async:** `.Result`, `.Wait()` → thread pool starvation and deadlocks.
- **API duplication:** `Foo()` and `FooAsync()` throughout the BCL.

#### ThreadPool starvation

- CLR injects threads at ~1–2/second when starved.
- Blocking on pool threads under load → catastrophic latency while CPU looks idle.
- `TaskCompletionSource` inline continuations → deadlocks.

**Lesson:** Never block worker threads on safe paths. Phoenix's `AWAIT_IO` + park model is the architectural fix; `#unsafe` FFI is the documented exception.

#### System.Threading.Channels

- Async producer-consumer queues with bounded backpressure.
- **Loved:** Clean pipelines, `ReadAllAsync`, multi-producer fan-out.
- **Hated:** Unbounded channels → memory bombs; `Writer.Complete()` discipline; not a full actor system.

**Phoenix mapping:** Channels may exist in std for pipelines, but **actor mailboxes** are the isolation primitive. Go research shows channels fail as a universal concurrency API.

---

### 4.3 C and C++

#### pthreads / std::thread

- Maximum control; no runtime tax.
- **Hated:** Data races = UB; lifetime hell; TSAN required; no compile-time help.

#### C++20 coroutines

- **Stackless:** state on heap; **zero standard executor** — every project builds its own runtime.
- Holding `std::mutex` across `co_await` is discouraged.
- Compiler bugs around suspend points (real-world UAF in Clang).

**Lesson:** If you offer suspension, **own the executor**. Phoenix's VM is the single blessed runtime — avoids C++'s fragmentation.

#### OpenMP

- Directive-based parallelism; shared-memory default.
- **Hated:** Wrong `shared`/`private` clauses → subtle races; debugging transformed code.

**Lesson:** Implicit sharing defaults are dangerous. Phoenix moves and actor mailboxes make ownership transfer explicit.

---

### 4.4 Zig

#### std.Io model (2025+ direction)

- Abandoned language-level `async`/`await` for explicit `Io` interface (like `Allocator`).
- **`Io.Threaded`:** thread pool; `io.async()` vs `io.concurrent()` — **asynchrony ≠ concurrency**.
- **`Io.Evented`:** experimental event loop (io_uring, kqueue).
- Structured concurrency: `Io.Group`, `Io.Select`.

**Loved:** No function coloring; honest execution model choice; `ConcurrencyUnavailable` when pool exhausted.

**Hated:** Boilerplate (`io` on every I/O function); evented path still maturing; no compile-time race prevention.

**Phoenix parallel:** Zig's `async` vs `concurrent` split maps to schedulable I/O vs `@spawn`. Phoenix can use **type markers** instead of an `io` parameter on every function.

---

### 4.5 Odin

- **Explicitly no green threads** — requires automatic memory management and large runtime (per creator).
- OS threads + futex sync + optional channels.
- **Loved:** Honest low-level model for engines/tools.
- **Hated:** High ceremony; no scheduler/I/O integration; DIY thread pools.

**Lesson:** Odin is the baseline for "ownership + threads only." Phoenix must beat Odin on **safe schedulable I/O + typed effects** by investing in VM scheduler early ([language-v0.md](../language-v0.md) Runtime v1).

---

### 4.6 Rust

#### std threads + Send/Sync

- Compile-time prevention of data races in safe code.
- `Send`/`Sync` bounds on `thread::spawn`.

#### async Rust + Tokio

- `Future` state machines; work-stealing M:N executor.
- **Loved:** Fearless concurrency for threads; zero-cost abstractions.
- **Hated:**
  - `Send + 'static` on every spawned future
  - Function coloring (`async fn` vs `fn`)
  - Executor ecosystem split (Tokio vs async-std vs smol)
  - `std::sync::Mutex` + `.await` = deadlock; `Arc<Mutex>` proliferation
  - Cancellation not built-in; poor async stack traces

**Phoenix lesson:** Compile-time concurrency safety is a major win. Phoenix should encode move-only cross-actor messaging and reject `&T` in messages ([messages.md](../features/messages.md)). Avoid async coloring entirely.

**Work-stealing implication:** Contexts that migrate between workers need **Send-equivalent** rules for stack state — open design question.

---

### 4.7 Go

#### Goroutines + netpoller + channels

- **M:P:G scheduler** with work stealing; preemption since Go 1.14 (SIGURG).
- Netpoller integrates socket I/O with scheduler.
- **Loved:** `go fn()`; net/http "just works"; fast context switch.
- **Hated:**
  - **Hidden blocking:** `net` hides suspension; `os.File` often blocks OS thread
  - Goroutine leaks (blocked forever on channel)
  - Channel overuse (counter via channel vs mutex — orders of magnitude slower)
  - Shared mutable state + races (race detector runtime-only)
  - Panic in goroutine crashes whole program

Research (*Understanding Real-World Concurrency Bugs in Go*): channels contribute disproportionately to bugs.

**Phoenix response:** Take M:N + poller; refuse hidden suspension; keep channels/mailboxes actor-scoped, not universal.

---

### 4.8 Erlang / Elixir (BEAM)

#### Processes + OTP

- Kilobyte-scale heaps; **no shared mutable memory** between processes.
- **Preemptive** via reduction counting (~4000 reductions per timeslice).
- Message passing with copy semantics (small terms fast; large terms expensive → ETS escape hatch).
- OTP: supervision trees, `GenServer`, "let it crash."

**Loved:** Isolation, uptime, millions of processes, hot code reload.

**Hated:** Copy costs; ceremony; NIF blocking; dynamic typing protocol mismatches.

**Phoenix advantages over BEAM:**
- **Move** at `@send` instead of deep copy
- Static protocol typing via traits and associated types
- No per-process GC — deterministic destruction via ownership

**Phoenix borrow from BEAM:**
- Preemptive fairness (reduction/instruction budgets for pure loops)
- Supervision as first-class VM feature
- Opt-in isolation, not mandatory processes for I/O

---

### 4.9 Comparative sentiment summary

| Ecosystem | Developers love… | Developers rage about… |
|---|---|---|
| **Java** | Loom ergonomics, mature tooling | Pinning, ThreadLocal, reactive debugging |
| **C#** | async syntax, ecosystem integration | Colored functions, pool starvation, `Foo`/`FooAsync` |
| **C/C++** | Control, performance | UB, races, no standard async runtime |
| **Zig** | Explicit `Io`, no coloring | Boilerplate, unfinished evented I/O |
| **Odin** | Simple threads + futex sync | No green threads, DIY everything |
| **Rust** | `Send`/borrow checker | `Send + 'static`, async split, lock+await |
| **Go** | `go` keyword, netpoller | Hidden blocking, leaks, channel abuse |
| **Erlang** | Isolation, OTP | Copy costs, ceremony, NIF blocking |

---

## 5. Cross-cutting pain points

### 5.1 Function coloring

Languages with "two colors" (sync vs async): JavaScript, C#, Python, Rust `async`, Kotlin `suspend`.

Languages often cited as color-free: Go (historically), Erlang, Java virtual threads.

**Phoenix stance:** No `async`/`await`. Schedulability appears in **types** at call sites — middle ground between Go (hidden) and C# (viral coloring).

### 5.2 Hidden suspension

Go's core frustration: you cannot tell from code whether a call may park or block an OS thread.

**Phoenix stance:** [runtime-transparency.md](../features/runtime-transparency.md) — three-way taxonomy: pure computation, schedulable I/O, explicit actor boundary.

### 5.3 Thread / carrier pool starvation

| Runtime | Starvation mechanism |
|---|---|
| .NET ThreadPool | Blocking on pool threads; slow thread injection |
| Java platform pools | Fixed max → request queuing |
| Java VT (pinned) | Carrier pool exhaustion |
| Go | Blocking syscall stalls M; cgo blocks |
| Tokio | `spawn_blocking` pool exhaustion |

**Invariant:** Worker threads must never block on user-level I/O in safe paths ([vm-linear.md](../features/vm-linear.md)).

### 5.4 GC pauses and allocation pressure

- More concurrent tasks → more allocations → more GC (JVM, .NET, Go).
- Virtual threads at scale → millions of short-lived stack chunks.
- **Phoenix advantage:** No STW GC; tail latency from scheduling and locks, not collector.
- **Phoenix cost:** Must solve allocation explicitly (arenas, pools) — GC hid this in managed languages.

### 5.5 Debugging and stack traces

| Model | Stack trace quality |
|---|---|
| Platform threads / VT | Excellent |
| async/await state machines | Poor (poll loops) |
| Reactive streams | Terrible (operator boilerplate) |
| Goroutines | Good with tooling; panics kill process |

**Phoenix requirement:** VM must preserve interpretable stacks across `ParkedAwaitIO` / `ParkedAwaitMessage` transitions.

### 5.6 FFI and blocking libraries

Every M:N system struggles here:

| System | Mitigation |
|---|---|
| BEAM | Dirty schedulers, NIF time limits |
| Go | Dedicated threads for blocking syscalls |
| Tokio | `spawn_blocking` pool |
| Java Loom | Pinning detection; carrier pool |

**Phoenix:** `#unsafe` FFI documented stall tax; consider dedicated **blocking thread pool** outside worker count; never call blocking FFI from schedulable-I/O lowering.

---

## 6. What Phoenix can do that others cannot

### 6.1 The differentiated position

| Capability | Rust | Go | Java/.NET | Erlang | **Phoenix** |
|---|---|---|---|---|---|
| No GC | ✓ | ✗ | ✗ | Per-process GC | **✓** |
| Compile-time ownership | ✓ | ✗ | ✗ | ✗ | **✓** |
| VM-managed M:N | ✗ (library) | ✓ | ✓ (VT) | ✓ | **✓** |
| Suspension visible at call site | `async` coloring | Hidden | Hidden | Messages | **Types** |
| Move-based actor messages | `mpsc` (library) | Channels (copy) | Shared heap | Deep copy | **`@send` move** |
| Portable bytecode | ✗ (native code) | ✗ | IL/JAR | BEAM | **PHX0** |
| Hot reload at message boundary | ✗ | ✗ | Limited | ✓ | **Planned** |
| Deterministic destruction | ✓ | ✗ | ✗ | Per-process GC | **✓** |

### 6.2 Rust-like safety + managed runtime (without GC)

**What managed runtimes enable:**

1. **Universal stack walking** — GC, exceptions, debuggers use runtime frame metadata. Phoenix VM must maintain interpreter frame metadata for ownership tracking, parking, and diagnostics — similar obligation, no reference updating on compaction.
2. **Cooperative suspension** — park at safe points with enumerable locals. Ownership tracker extends to "what is live across park?"
3. **Lightweight contexts without GC stacks** — VT/goroutine stacks are heap objects under GC. Phoenix can use **VM-allocated fixed or growable stacks** with deterministic reclamation on context `Done`.
4. **Integrated I/O reactor** — single VM owns epoll/kqueue/IOCP + scheduler wakeups.
5. **Bytecode portability** — same PHX0 runs everywhere the VM runs; scheduler behavior is part of the language contract.

**What Rust cannot easily do without a blessed runtime:**

- Guarantee all user code runs under a scheduler (Tokio is opt-in ecosystem).
- Type-visible I/O effects without `async` coloring (effect systems are experimental).
- VM-level actor isolation + supervision as language contract.
- Hot reload with message-boundary version handoff ([vm-linear.md](../features/vm-linear.md) §3).

**What JVM/.NET cannot easily do:**

- Sub-millisecond tail latency without GC tuning (ZGC helps but doesn't eliminate the tax).
- Move semantics across mailboxes without shared heap + synchronization.
- Compile-time rejection of cross-actor borrows.

### 6.3 Ownership + concurrency composition

Phoenix can enforce rules Erlang assumes and Go hopes for:

| Rule | Mechanism |
|---|---|
| No cross-actor shared `&mut` | Compile error |
| `@send` moves unless `Copyable` | [ownership.md](../features/ownership.md) |
| No borrows in messages | [messages.md](../features/messages.md) |
| Schedulable I/O may park; pure code may not | Type system + verifier |
| Actor protocols statically typed | Future `Actor` trait + associated types |

**Unique combo:** BEAM isolation discipline + Rust move semantics + Go-scale M:N — without GC copying or `Send + 'static` future bounds.

### 6.4 VM-only capabilities (long-term)

From [vm-linear.md](../features/vm-linear.md):

| Feature | Why VM matters |
|---|---|
| **JIT** | Hot bytecode regions; deopt to interpreter |
| **Hot reload** | Message-boundary version handoff |
| **Crash isolation** | Trap actor faults; supervisor policies |
| **I/O abstraction** | `AWAIT_IO` opcode = contract between type system and runtime |
| **Deterministic MVP → rich runtime** | Same bytecode format versions forward |

---

## 7. Type system integration

### 7.1 Three-way call-site taxonomy (existing design)

| Category | Meaning | How visible |
|---|---|---|
| **Pure computation** | Runs to completion; no implicit park | Ordinary signatures |
| **Schedulable I/O** | May park context | Type/signature marker (syntax TBD) |
| **Explicit actor** | Isolation / messaging | `@spawn`, `@send`, `@receive`, `@reply` |

**Design goal:** Answer "can this call suspend?" from the signature alone — without `async` keyword viral propagation.

### 7.2 Candidate schedulable-I/O type representations

Research-informed options (not decided):

| Approach | Inspiration | Pros | Cons |
|---|---|---|---|
| **Wrapper type** | `IO<T>` / `Schedulable<T>` | Explicit at use site | May need `.unwrap()`-style run or automatic await in VM |
| **Effect annotation on `Result`** | `Result<T, E> + Schedulable` | Composes with error model | Verbose signatures |
| **Trait bound** | `fn read() -> impl SchedulableIO<[u8]>` | Flexible | Harder for beginners |
| **Function attribute** | `#[schedulable]` on fn | Simple | Less granular than type-level |
| **Separate type constructor** | `File.read :: (...) => Sched<Result<Bytes, IOError>>` | Honest; searchable | New type to learn |

**Zig lesson:** Pass capability explicitly (`Io` param). **Kotlin lesson:** Mark suspension in signature. **Phoenix synthesis:** Type-level marker that does not split the function namespace into two colors.

### 7.3 Send / Sync equivalents (open)

Rust's `Send`/`Sync` exist because work-stealing moves tasks between threads.

Phoenix contexts may migrate between workers ([concurrency.md](../features/concurrency.md): "does not promise which worker thread").

**Likely needs:**

| Concept | Purpose |
|---|---|
| **`Migratable` / `Send`-like** | Context can resume on any worker after park |
| **`Pinned` / `!Send`-like** | Context must stay on one worker (thread-local, raw FFI handle) |
| **Stack pinning rules** | What local state survives `ParkedAwaitIO`? |

Parking preserves locals on context stack ([ownership.md](../features/ownership.md)) — distinct from `@send` cross-actor move.

### 7.4 Actor trait contract (future)

From [concurrency.md](../features/concurrency.md):

- Actor-marked types implement `Actor` trait
- Message protocol compatibility via associated types
- Actor handles are library types, not primitives
- Compile-time validation of `@send`/`@receive` pairs

**Advantage over Erlang:** Protocol mismatches are compile errors, not `badarg` at runtime.

### 7.5 Calling schedulable APIs from pure contexts

Deferred rule ([runtime-transparency.md](../features/runtime-transparency.md)):

- Can pure functions call schedulable I/O?
- Likely: caller's context parks — callee's schedulability **inherits** to caller unless explicitly isolated (actor or scoped spawn).
- Alternative: forbid schedulable calls from functions marked `pure` — stricter, Zig-like.

### 7.6 Integration with `Result`, `?`, and errors

Schedulable I/O failures remain `Result` + `?` ([error-handling.md](../features/error-handling.md)) — errors are values, not hidden control flow. Same transparency goal as I/O effects.

---

## 8. VM and runtime architecture

### 8.1 Execution context state machine (existing)

```
                    ┌──────────────┐
         ┌─────────►│   Running    │◄─────────┐
         │          └──────┬───────┘          │
         │                 │                  │
         │    schedulable  │   message        │  wakeup
         │    I/O call     │   wait           │
         │                 ▼                  │
         │          ┌──────────────┐          │
         │          │ ParkedAwaitIO│──────────┤
         │          └──────────────┘          │
         │                 │                  │
         │                 │                  │
         │          ┌──────────────┐          │
         │          │ParkedAwaitMsg│──────────┘
         │          └──────────────┘
         │                 │
         │                 ▼
         │          ┌──────────────┐
         └──────────│     Done     │
                    └──────────────┘
```

### 8.2 M:N scheduler design decisions

| Decision | Recommendation | Rationale |
|---|---|---|
| Worker count | ~`num_cpus` | Standard for CPU-bound + I/O multiplex |
| Context stack | VM-allocated, growable or segmented | No GC; avoid 1 MB OS stacks per context |
| I/O integration | Integrated reactor (epoll/kqueue/IOCP) | Go/Java/Tokio consensus |
| Blocking FFI | Dedicated blocking pool | All M:N systems need this |
| Preemption | Instruction/reduction budget for pure loops | BEAM/Go 1.14 lesson; cooperative-only risks starvation |
| Actor scheduling | One message per actor per turn, then yield | BEAM fairness |
| Backpressure | Bounded mailboxes; `ConcurrencyUnavailable`-style errors | Java VT unbounded concurrency lesson |

### 8.3 Opcode families (planned)

| Opcode | Role |
|---|---|
| `PARK` | Transition to parked state |
| `RESUME` | Mark runnable after wakeup |
| `AWAIT_IO` | Non-blocking I/O + park |
| `ENQUEUE_MAILBOX` / `DEQUEUE_MAILBOX` | Actor messaging |
| `SPAWN_CONTEXT` | Create actor context |

### 8.4 Phoenix vs Go netpoller

| | Go | Phoenix |
|---|---|---|
| Socket I/O | Hidden park via netpoller | Type-visible schedulable |
| File I/O | Often blocks M (OS thread) | Must use `AWAIT_IO` on safe path |
| Programmer model | `go` everywhere | Implicit context + typed I/O |
| Shared state | Default | Ownership; actors opt-in |

### 8.5 Phoenix vs Java Loom

| | Java VT | Phoenix |
|---|---|---|
| Unit | Virtual thread | Execution context |
| Blocking style | Implicit yield points | Typed schedulable signatures |
| Pinning risk | `synchronized`, JNI | VM locks + `#unsafe` FFI |
| Memory | GC stacks | VM stacks, deterministic free |
| Isolation | None default | Opt-in actors |

### 8.6 Shipping order (from language-v0.md)

```
Language v0 (types, ownership, bytecode)
        ↓
Std v0 (Option, Result, traits)
        ↓
Runtime v1 (scheduler + schedulable I/O)
        ↓
Std I/O (File.read, networking)
        ↓
Actors + supervision (full @ directives)
```

Concurrency types in the frontend should be designed **before** Runtime v1 ships, even if rejected at typeck until then — avoids retrofitting signatures across std.

---

## 9. Metaprogramming and compile-time concurrency

Phoenix splits directives: `#` compile-time, `@` runtime ([compiler-directives.md](../features/compiler-directives.md)).

### 9.1 Compile-time (`#`) opportunities

| Feature | Concurrency use |
|---|---|
| **`#derive(Actor)`** | Generate `on_message` dispatch, mailbox registration metadata |
| **`#derive(Message)`** | Encode protocol ID, max size, `Copyable`/`Clone` bounds |
| **`#[must_use]`** | Warn on discarded `Schedulable` or actor handles |
| **`#[cfg(target_os)]`** | Platform-specific I/O backends |
| **`#inline` / `#cold` / `#hot`** | Scheduler-aware placement hints |

### 9.2 Runtime (`@`) directives (existing plan)

| Directive | Role |
|---|---|
| `@spawn(expr)` | Create isolated actor context |
| `@send(target, msg)` | Move message to mailbox |
| `@receive(target)` | Dequeue owned message |
| `@reply(msg)` | Reply in handler context |

### 9.3 Deferred directives (grammar-deferred.md)

- `#actor` — mark types as actor implementations?
- `#supervise` — attach restart policy at compile time?

### 9.4 Static protocol verification

Metaprogramming enables what Erlang does dynamically:

```phoenix
// Illustrative — not final syntax
Request :: enum { Read { path: Path }, Stop }
Response :: enum { Bytes([u8]), Done }

FileReader :: struct { ... }
FileReader :: impl :: Actor
{
  type Message = Request;
  type Reply = Response;
  on_message :: (self: &mut Self, msg: Request) => Response { ... }
}
```

`#derive(Actor)` could generate:
- Message ID → handler dispatch table
- PXI export metadata for cross-module protocol checking
- Verifier rules for mailbox entry sizes

### 9.5 PXI / bytecode metadata

Post-MVP bytecode sections ([vm-linear.md](../features/vm-linear.md)):
- Actor opcode hooks
- Protocol compatibility for hot reload
- Ownership verification metadata

Metaprogramming at compile time populates these sections — VM enforces at runtime.

### 9.6 Structured concurrency (compile-time scopes)

Inspired by Zig `Io.Group`, Kotlin `coroutineScope`, Rust `async_scope`:

Future Phoenix patterns:
- Scoped `@spawn` — child actors cancelled when scope exits
- Compile-time lint for orphaned contexts
- `#derive` generates cleanup hooks

---

## 10. Tooling, LSP, and developer experience

> **Note:** This repository has no TypeScript frontend today. Tooling is Rust (`phx-cli`, `phx-compiler`, `phx-vm`). The compiler exposes a stable facade for external tools ([`source/phx-compiler/src/facade.rs`](../../source/phx-compiler/src/facade.rs)) and stable diagnostic codes ([`source/phx-diagnostics/src/code.rs`](../../source/phx-diagnostics/src/code.rs)). A future LSP (whether implemented in TypeScript, Rust, or otherwise) should treat concurrency effects as first-class semantic information.

### 10.1 What tooling should surface

| Information | Source | IDE value |
|---|---|---|
| Schedulable vs pure call | Type checker | Inline hint: "may park context" |
| Actor boundary | `@` directives | Visual gutter / code lens |
| Move at `@send` | Ownership pass | Highlight invalidated bindings |
| Protocol mismatch | Actor trait check | Compile error with fix suggestion |
| Worker stall risk | `#unsafe` FFI | Warning on blocking patterns |

### 10.2 Diagnostic codes (stable for LSP)

Extend [`phx-diagnostics`](../../source/phx-diagnostics/) with concurrency-specific codes:

| Code | Example |
|---|---|
| `PHX_CONC_SCHEDULABLE_CALL` | Calling schedulable I/O from `#[pure]` context |
| `PHX_CONC_CROSS_ACTOR_BORROW` | `&T` in message |
| `PHX_CONC_USE_AFTER_SEND` | Using moved message binding |
| `PHX_CONC_UNBOUNDED_MAILBOX` | Actor mailbox without capacity |
| `PHX_CONC_FFI_BLOCK` | `#unsafe` call may block worker |

### 10.3 Runtime introspection (VM tooling)

| Tool | Data |
|---|---|
| Context dump | All execution contexts + states |
| Scheduler metrics | Runnable vs parked counts, worker utilization |
| Mailbox depth | Per-actor queue length (backpressure visibility) |
| Park site | Which `AWAIT_IO` / `@receive` site blocked |
| Pinning detection | Context holding lock across park (Loom-style) |

Java's `jdk.VirtualThreadPinned` events and .NET thread-pool starvation events prove **runtime observability drives adoption**. Phoenix VM should emit structured events from day one of Runtime v1.

### 10.4 Testing concurrency

| Layer | Approach |
|---|---|
| Compiler | Typeck tests for schedulable/pure/actor rules |
| VM unit | Scheduler fairness, park/resume, mailbox ordering |
| Integration | `tests/phoenix/` programs with expected scheduling |
| Stress | Many contexts + I/O simulation without real network |

### 10.5 Docs and `--explain`

Stable diagnostic codes enable `phx explain PHX_CONC_*` — critical for schedulable-I/O semantics that differ from intuitive "sync" code.

---

## 11. Recommendations and open questions

### 11.1 Recommendations aligned with existing design

| # | Recommendation | Confidence |
|---|---|---|
| 1 | **Keep no `async`/`await`** | High — validated by coloring pain across C#/Rust/Kotlin |
| 2 | **M:N scheduler as universal substrate** | High — Go/Java/Tokio consensus |
| 3 | **Type-visible schedulable I/O** | High — direct response to Go hidden blocking |
| 4 | **Opt-in actors, not mandatory processes** | High — Erlang ceremony lesson |
| 5 | **Move-based `@send`** | High — ownership advantage over BEAM/Go |
| 6 | **Integrated I/O reactor, not per-thread blocking** | High — starvation lessons everywhere |
| 7 | **Dedicated blocking pool for FFI** | High — NIF/cgo/Tokio pattern |
| 8 | **Instruction budget preemption for pure code** | Medium-high — BEAM/Go lesson |
| 9 | **Preserve stack traces across park** | High — reactive/async debugging failures |
| 10 | **Bounded mailboxes + backpressure** | High — Java VT unbounded concurrency lesson |

### 11.2 Open questions requiring design decisions

| # | Question | Options | Research lean |
|---|---|---|---|
| 1 | Schedulable-I/O type syntax | Wrapper vs trait vs attribute | Wrapper or `Sched<Result<T,E>>` — visible, composable with `?` |
| 2 | Pure function calling schedulable I/O | Allow (context parks) vs forbid | Allow — matches Loom/Go ergonomics with honest types |
| 3 | Preemption granularity | Cooperative only vs reduction budget | Reduction budget — CPU fairness |
| 4 | Context migration / pinning | Full migration vs pinned contexts | `Migratable` trait bound; pin for FFI handles |
| 5 | Std channels vs actors only | Channels for pipelines | Actor mailboxes primary; channels secondary in std |
| 6 | Structured concurrency syntax | Scoped `@spawn` block vs library | Language scope syntax — Zig/Rust lesson |
| 7 | Hot reload + actors | Message-boundary version gate | Follow BEAM + vm-linear.md plan |
| 8 | Selective `@receive` | `select` across mailboxes | Defer; BEAM `select` is complex but valuable |

### 11.3 Models to explicitly reject

| Model | Why |
|---|---|
| `async`/`await` as primary | Viral coloring; documented Phoenix rejection |
| Go-style hidden suspension | Violates runtime transparency |
| Reactive streams by default | Debugging cost; team training time |
| Thread-per-request OS model | Doesn't scale; wrong for M:N |
| Mandatory actors for all I/O | Erlang ceremony for `File.read` |
| Shared-memory default between actors | Defeats isolation; reintroduces data races |

### 11.4 Suggested research prototypes (pre-implementation)

Before Runtime v1 code:

1. **Type system prototype** — schedulable wrapper + `?` interaction on paper; 10 std API signatures.
2. **Scheduler simulation** — Rust prototype: M contexts, N workers, fake I/O wakeups; measure fairness.
3. **Stack trace design** — park/resume with captured source locations; compare to Loom vs Tokio.
4. **FFI stall benchmark** — blocking pool size vs worker starvation.
5. **Actor protocol `#derive` sketch** — codegen output for 3-message enum.

---

## 12. References

### Phoenix design docs

| Document | Path |
|---|---|
| Concurrency model | [features/concurrency.md](../features/concurrency.md) |
| Runtime transparency | [features/runtime-transparency.md](../features/runtime-transparency.md) |
| Actor messages | [features/messages.md](../features/messages.md) |
| VM / bytecode | [features/vm-linear.md](../features/vm-linear.md) |
| Ownership | [features/ownership.md](../features/ownership.md) |
| Directives | [features/compiler-directives.md](../features/compiler-directives.md) |
| Language v0 roadmap | [language-v0.md](../language-v0.md) |

### External

| Topic | Link |
|---|---|
| What Color is Your Function? | [Bob Nystrom (2015)](https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/) |
| ASP.NET async guidance | [David Fowler](https://github.com/davidfowl/AspNetCoreDiagnosticScenarios/blob/master/AsyncGuidance.md) |
| Thread pool starvation | [Vance Morrison / PerfView](https://learn.microsoft.com/en-us/archive/blogs/vancem/diagnosing-net-core-threadpool-starvation-with-perfview-why-my-service-is-not-saturating-all-cores-or-seems-to-stall) |
| Java Virtual Threads (JEP 444) | [OpenJDK](https://openjdk.org/jeps/444) |
| Programming language memory models | [Russ Cox](https://research.swtch.com/plmm) |
| Go concurrency bugs study | *Understanding Real-World Concurrency Bugs in Go* (USENIX) |
| Project Reactor debugging | [reactor.io debugging guide](https://projectreactor.io/docs/core/reference/debugging.html) |
| .NET Channels | [Microsoft docs](https://learn.microsoft.com/en-us/dotnet/core/extensions/channels) |
| Zig std.Io direction | [Zig GitHub issues / std.Io docs](https://github.com/ziglang/zig) |

### Community sentiment sources

- Netflix virtual thread pinning post-mortem (widely cited in Java community, 2023–2024)
- Hacker News threads on Kotlin coroutines vs Loom preemptibility
- Stack Overflow: Spring WebFlux / Reactor stack trace complaints
- Rust internals: `Send + 'static` frustration threads
- Go: channel misuse performance comparisons (mutex vs channel counter)

---

## Appendix A: Phoenix comparison table (extended)

| Dimension | OS threads only | Green threads / M:N | Actors only | **Phoenix hybrid** |
|---|---|---|---|---|
| Scalability (I/O) | Poor | Excellent | Excellent | Excellent |
| Scalability (CPU) | Good | Good (with preemption) | Good (per-actor sequential) | Good |
| Memory per task | ~1 MB | ~KB | ~KB | ~KB (VM stack) |
| Call-site honesty | High | Low (Go) / Medium (Loom) | High | **High (types + @)** |
| Function coloring | No | No (Go/Loom) | No | **No** |
| Data race safety | None | None (Go) | Isolation | **Ownership + opt-in actors** |
| GC required | No | Go/Java yes | BEAM yes | **No** |
| Fault isolation | No | No (Go) | Yes | **Opt-in** |
| FFI story | Simple | Complex (pinning) | Complex (NIF) | **`#unsafe` + blocking pool** |
| Debuggability | Excellent | Good (VT) / Poor (async) | Good (BEAM) | **Target: VT-quality** |

---

## Appendix B: User-provided model notes (evaluated)

The following were initial brainstorming notes — evaluated against research:

| Idea | Verdict |
|---|---|
| OS threads (1:1) as programmer model | **Reject** as default; use as worker substrate only |
| Green threads / M:N | **Adopt** as Layer 1 (already in design) |
| Actors | **Adopt** as opt-in Layer 3 (already in design) |
| Combine small OS pool for blocking syscalls | **Adopt** — Go/Tokio/BEAM dirty schedulers pattern |
| Non-blocking I/O driver integration | **Adopt** — netpoller model |
| Preemption concerns | **Add** — research favors reduction/instruction budgets |
| Efficient sync primitives at runtime | **Adopt** — VM-managed locks aware of park |

Additional ideas from research not in original notes:

- Type-visible schedulable I/O (not hidden goroutines)
- Move-based messaging (not BEAM copy)
- Static actor protocol checking via traits
- VM-level supervision trees
- Hot reload at message boundaries
- `#derive(Actor)` metaprogramming
- Structured concurrency scopes
- Pinning/`Migratable` type bounds for worker migration
- Runtime pinning detection and diagnostics

---

*Last updated: 2026-06-07. Maintainers: update this doc when schedulable-I/O type syntax is decided or Runtime v1 implementation begins.*
