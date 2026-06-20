# Debug and observability

Status: phased design (interim CLI channel shipped; full protocol post–Language v0)

Phoenix targets a **VM-managed runtime** (M:N scheduler, schedulable I/O, actors). Debuggability is a product requirement, not an afterthought. This document defines **layered** debug features, **dev vs release** artifact policy, and how tooling attaches without changing language semantics in production builds.

**Related:**

- [vm-linear.md](vm-linear.md) — `PHX0` sections, verifier, interpreter contract
- [compiler-directives.md](compiler-directives.md) — `#[cfg(debug_assertions)]`, build-time stripping
- [runtime-transparency.md](runtime-transparency.md) — VM effects visible at call sites; debug surfaces those effects honestly
- [concurrency.md](concurrency.md) — scheduler contexts and actors (debug extensions land with that runtime)
- [language-v0.md](../language-v0.md) — Language v0 checklist; full debug protocol does **not** block v0 completion

---

## Goals

| Goal | Detail |
|------|--------|
| **Inspectable stacks** | Named frames, locals, and call chains in dev builds — including across nested calls, not only `main` at exit |
| **Dev vs release** | Debug metadata and extra runtime checks in dev; stripped, lean artifacts for production distribution |
| **VM-native** | Debug hooks live in the interpreter / scheduler — not only pretty-printing bytecode |
| **Portable bytecode** | Executable `PHX0` stays portable; debug info is optional or separable |
| **Future IDE attach** | Stable event stream (DAP-shaped) for breakpoints, step, stack, variables |
| **Runtime transparency** | When a context parks for I/O or waits on a mailbox, debug mode can report **why** and **which context** |

## Non-goals (this design)

- **Not a language feature for user `println`** — std I/O and formatting are separate ([mvp.md](../mvp.md) defers std I/O until schedulable runtime).
- **Not DWARF-in-PHX0 for v1** — Phoenix owns a compact, VM-oriented debug format tied to `DefId`, function ids, and slot layouts.
- **Not mandatory in Language v0** — interim `phx run --dump-main` suffices until section 5 and trace modes ship.
- **Not JIT/debug interop** — JIT is post-v0; debug design must not assume JIT is always present.

---

## Layered model

Debug capability is split into five layers. Each layer has a **dev** and **release** posture.

```text
┌─────────────────────────────────────────────────────────────┐
│ 5. External protocol (DAP / IDE attach)                     │
├─────────────────────────────────────────────────────────────┤
│ 4. Execution mode (run, trace, breakpoint stop)             │
├─────────────────────────────────────────────────────────────┤
│ 3. Runtime checks (asserts, optional deep verify)           │
├─────────────────────────────────────────────────────────────┤
│ 2. Bytecode + metadata (symbols, line tables, trace maps) │
├─────────────────────────────────────────────────────────────┤
│ 1. Compile-time (debug build, cfg, lowering hooks)        │
└─────────────────────────────────────────────────────────────┘
```

| Layer | Dev build | Release build |
|-------|-----------|---------------|
| **1. Compile-time** | `debug_assertions` true; emit debug sections; optional trace/breakpoint maps | `debug_assertions` false; no debug sections (or external sidecar omitted) |
| **2. Bytecode metadata** | Section 5 (symbols) + section 7 (line program) present | Sections omitted; header `PHX0_DEBUG_STRIPPED` flag set |
| **3. Runtime checks** | Extra interpreter asserts (stack depth, slot kinds); poison fills optional | Verifier contract only; minimal traps |
| **4. Execution mode** | Interpreter honors breakpoints / single-step / call trace | `phx run` — no stepping hooks |
| **5. External protocol** | `phx debug` adapter exposes stacks, locals, events | Adapter not started; no listen socket |

**Invariant:** Release builds remain **correct** when debug layers are stripped. `#[cfg(debug_assertions)]` adds **extra** checks only — never the sole definition of safety ([compiler-directives.md](compiler-directives.md)).

---

## Interim MVP (shipped)

Until layers 2–5 land, Phoenix documents a **minimal debug channel**:

- **`phx run --dump-main`** — after successful run, print `main[N]: <value>` lines to stderr ([vm-linear.md](vm-linear.md#mvp-interpreter-contract-phx-vm)).
- **`phx_vm::run_captured`** — test-only API returning `main_locals`, `aggregates`, optional `return_value`.

**Limits (by design):**

- Snapshot at **`main` return** only; callee frames are already popped.
- Slot indices, not source names (`s`, `first`).
- `Agg(n)` handles not decoded in CLI (arena data exists in capture but is not pretty-printed).

This channel remains supported as a **zero-dependency** smoke path after full debug tooling exists.

---

## Layer 1 — Compile-time

### Build profiles

| Profile | `phx build` flag | `debug_assertions` | Emits debug sections |
|---------|------------------|--------------------|-----------------------|
| **Dev** (default) | (none) or `--debug` | `true` | yes |
| **Release** | `--release` | `false` | no |

`phoenix.toml` may later expose:

```toml
[build]
profile = "dev"   # or "release"
```

Release profile sets compile-time flags consumed by resolver (`#[cfg]`), codegen (omit debug), and packaging (strip sidecars).

### `#[cfg(debug_assertions)]`

Already specified in [compiler-directives.md](compiler-directives.md). Debug-only items (extra logging structs, expensive invariant checks in Phoenix source) gate on this flag.

**Rule:** Do not use `debug_assertions` to hide behavior required for correctness in release.

### Lowering hooks (dev only)

When debug sections are enabled, codegen records per function:

- Source file id + span range for function entry
- Map: `local_slot → (name_symbol_id, type_id, decl_span)`
- Map: `function_id → name_symbol_id`
- Optional: monomorphized mangled name for generic exports (aligns with [pxi-format.md](pxi-format.md) export ids)

No new **user-facing** syntax is required for v1; debug is tooling-driven.

---

## Layer 2 — Bytecode metadata (`PHX0`)

[vm-linear.md](vm-linear.md) reserves **section kind `5`: symbols**. This design activates it in dev builds and adds **section kind `7`: line program**.

### Header flags (proposal)

| Flag bit | Name | Meaning |
|----------|------|---------|
| `0x0000_0001` | `PHX0_HAS_DEBUG` | Sections 5 and/or 7 present |
| `0x0000_0002` | `PHX0_TRACE_MAP` | Call-trace bitmap or opcode hooks present (layer 4) |

Release builds clear these bits.

### Section 5 — Symbols (dev)

Payload (versioned; minor format bump when first written):

#### Phase 1 (PHX-070 D1 spike): PC span map

Until the full symbol/name tables land, dev builds may write a **minimal section 5** containing only a sorted `(function_id, pc) → source span` map. This is sufficient for CLI/VM to resolve [`VmError`](../../source/phx-vm/src/error.rs) sites `(function_id, pc)` to UTF-8 source byte ranges (PHX-063 [`Span`](../../source/phx-diagnostics/src/span.rs) values).

Wire layout (little-endian, `sub_version = 1`):

| Field | Size | Meaning |
|-------|------|---------|
| `sub_version` | 4 | `1` for phase 1 |
| `file_count` | 4 | Number of compilation-unit paths |
| `files[]` | `4 + N` each | `path_len: u32`, UTF-8 path bytes (project-relative) |
| `entry_count` | 4 | Number of PC span rows |
| `entries[]` | 20 each | `function_id: u32`, `pc: u32`, `file_id: u32`, `span_start: u32`, `span_end: u32` |

Rows are sorted by `(function_id, pc)` ascending. `pc` is the byte offset of the instruction within the function body (same convention as VM error sites). When `file_count == 0`, rows must use `file_id == 0` (unknown file).

Implementation: [`PcSpanTable`](../../source/phx-bytecode/src/pc_span.rs); codegen records spans in debug builds (`cfg(debug_assertions)`); linker merges tables across modules.

#### Full symbols (planned D1+)

| Field | Purpose |
|-------|---------|
| `file_count` | Number of compilation unit paths |
| `files[]` | UTF-8 relative paths (project-root relative) |
| `symbol_count` | Interned debug names |
| `symbols[]` | UTF-8 names (functions, locals, types for display) |
| `function_debug[]` | Per `function_id`: `name_id`, `file_id`, `entry_line`, `entry_col`, `local_count` |
| `local_debug[]` | Per slot: `function_id`, `slot`, `name_id`, `type_id` |

Verifier: if section 5 is present, every `function_id` in the function table must have a matching debug record or an explicit “synthetic” marker (intrinsics).

### Section 7 — Line program (dev)

Compact mapping: `(function_id, code_offset) → (file_id, line, col)` for:

- Breakpoint resolution (IDE line → bytecode offset)
- Stack traces (offset → source line)
- Step-over / step-into boundaries

Encoding TBD (same spirit as DWARF line tables or WASM names subsection — **Phoenix-specific**, little-endian, no external deps).

### Sidecar option

For distribution, debug may live in a sibling file:

```text
build/bin/my_app.phx0
build/bin/my_app.phx0.debug   # sections 5 + 7 only
```

`phx run` / VM loader:

- Dev: load sidecar automatically when present next to module.
- Release packaging: omit `.debug` artifact; CI rejects accidental upload of sidecars if policy requires.

---

## Layer 3 — Runtime checks

| Check | Dev | Release |
|-------|-----|---------|
| Bytecode **verifier** before run | always | always |
| Stack depth vs `stack_max` | hard trap + diagnostic | hard trap |
| Local slot kind vs layout table | assert on load/store in debug interpreter | trust layout (verifier proved) |
| Aggregate handle bounds | assert on `Agg(n)` access | trap on corrupt bytecode only |
| Use-after-free on arena | optional poison in dev | omitted |

These checks live in `phx-vm` behind `InterpreterMode::Debug` vs `InterpreterMode::Release`.

**Not duplicated in user code** — the VM enforces interpreter contracts; the compiler enforces static rules.

---

## Layer 4 — Execution modes

### Modes

| Mode | CLI | Behavior |
|------|-----|----------|
| **Run** | `phx run` | Execute until exit; no stops |
| **Dump** | `phx run --dump-main` | Run + print `main` locals at exit (interim) |
| **Trace** | `phx run --trace-calls` | Log `Call` / `Return` with `function_id` and optional symbol names |
| **Dump full** | `phx run --dump-all` | At exit: all frames if stopped early; decode `aggregates` (`str` bytes, struct fields) |
| **Break** | `phx debug` (see layer 5) | Stop on breakpoint / step |

### Trace events (in-process, v1)

Emitted to stderr or a ring buffer consumed by the debug adapter:

```text
call fn=3 name=sum locals=[Scalar(I32(10)), Scalar(I32(2))]
return fn=3 value=Scalar(I32(12))
```

With section 5, `name=` is populated; without it, numeric ids only (current behavior).

### Breakpoints (v1)

- Set by **bytecode offset** or **source line** (requires section 7).
- Implemented as: interpreter checks breakpoint set before each instruction dispatch (dev mode only).
- **No `@breakpoint` runtime directive in v1** — avoids conflating debug with actor syntax ([compiler-directives.md](compiler-directives.md)).

### Stepping

| Command | Semantics |
|---------|-----------|
| **Step over** | Run until PC advances in current frame without entering callee |
| **Step into** | Stop at callee entry |
| **Step out** | Run until current frame returns |

Requires layer 2 line/offset maps and layer 4 interpreter hooks.

---

## Layer 5 — External debug protocol

### Design target: DAP-shaped adapter

Phoenix ships a **`phx debug`** subcommand (or separate `phx-debug` tool) speaking a **Debug Adapter Protocol**–compatible subset over stdio:

- `launch` / `attach` (attach deferred until multi-process / long-running services)
- `setBreakpoints`
- `threads` / `stackTrace` / `scopes` / `variables`
- `continue` / `next` / `stepIn` / `stepOut`
- `terminated` / `stopped` events

**MVP of layer 5:** single-process, launch-only, stack VM — sufficient for IDE integration with VS Code / Cursor debug clients.

### Stable VM event enum (language contract)

Tooling and future scheduler share one event vocabulary:

```text
DebugEvent::
  ProgramExit(exit_code)
  BreakpointHit(function_id, offset, context_id)
  StepComplete(context_id)
  Call(function_id, context_id)
  Return(function_id, context_id, optional_value)
  ContextParked(context_id, reason)      # post-MVP
  ContextResumed(context_id, reason)     # post-MVP
  ActorMessage(context_id, actor_id, …)  # post-MVP
  InternalError(message)                 # never leak Rust backtrace to user
```

`context_id` is `0` for MVP single-threaded VM; scheduler assigns stable ids per execution context later ([concurrency.md](concurrency.md)).

### Security

- Debug adapter listens on **stdio** or **local socket** only by default.
- No remote attach without explicit `--listen` and authentication (out of scope for v1).

---

## Post-MVP: scheduler, I/O park, actors

When the M:N runtime ships, debug layers extend — **same protocol**, richer events:

| Concern | Debug surface |
|---------|----------------|
| **Parked for I/O** | `ContextParked` with schedulable-I/O call site + file/line |
| **Mailbox wait** | `ActorMessage` / blocked reason on `@receive` |
| **Stack across park** | Frames preserved in VM metadata while context is parked ([research note](../research/concurrency-models-research.md) §5.5) |
| **Actor isolation** | Per-actor stacks and mailbox snapshots (read-only introspection) |
| **Supervision** | Tree of actors with last crash diagnostic |

**Requirement (from concurrency research):** stack walks must remain interpretable across `ParkedAwaitIO` / `ParkedAwaitMessage` — design layer 2 line tables and layer 5 events with park state up front, not bolted on later.

---

## Tooling surface (CLI)

| Command | Layer | Status |
|---------|-------|--------|
| `phx run --dump-main` | 4 (dump) | **Shipped** |
| `phx run --trace-calls` | 4 (trace) | Planned |
| `phx run --dump-all` | 4 (dump) | Planned |
| `phx build --release` | 1–2 (strip) | Planned |
| `phx debug [--stdio]` | 5 (DAP) | Planned |
| `phx inspect <file.phx0>` | 2 (static) | Optional — disassemble + section dump for CI |

---

## Phased rollout

Aligned with [language-v0.md](../language-v0.md) and post-MVP runtime. **Do not block Language v0** on later phases.

| Phase | Deliverable | Depends on |
|-------|-------------|------------|
| **D0** (now) | `--dump-main`, `run_captured`, document interim channel | MVP VM |
| **D1** | Emit section 5; named trace (`--trace-calls`); `phx build --release` strips | Codegen + verifier |
| **D2** | Section 7 line tables; breakpoints + step in interpreter; `phx debug` stdio DAP (launch) | D1 |
| **D3** | `--dump-all` aggregate decode; sidecar `.phx0.debug` packaging | D1 |
| **D4** | Scheduler-aware stacks; `ContextParked` / resume events | [concurrency.md](concurrency.md) |
| **D5** | Actor / mailbox introspection; attach to running VM | Actors + messages design |
| **D6** | JIT-aware debug (deopt points, symbol preservation) | JIT (post-v0) |

---

## Open questions

1. **Section 7 encoding** — fixed-width table vs variable-length line program (decide at D1 implementation).
2. **Cross-crate debug** — how `.pxi` + linked `PHX0` expose mangled names across path deps (align with PXI export ids).
3. **Library packages** — `ENTRY_NONE` libs: debug sections still emitted for use when linked into bins?
4. **Hot reload** — breakpoint binding when `function_id` maps change across module versions ([vm-linear.md](vm-linear.md) hot reload section).
5. **`@` debug directives** — defer unless a runtime-visible debug probe is needed (e.g. user-facing trace points); prefer CLI + metadata for v1.

---

## Summary

Phoenix should implement **layered debug** as VM infrastructure: compile-time metadata, optional stripped release artifacts, interpreter modes, and a DAP-shaped adapter. `--dump-main` is the **D0** channel only. Full stacks, named locals, breakpoints, and actor/scheduler introspection follow in **D1–D5** without changing production language semantics when debug layers are absent.
