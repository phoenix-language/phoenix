# Agent task queue

**Updated:** 2026-06-20  
**Maintainer:** v0 sprint orchestrator (master agent) — **not** a static checklist.

This file is the **active work queue** for autonomous agents. The orchestrator **reads it every iteration**, assigns the top unchecked items to workers, and **updates it** when tasks complete, block, or are split.

**Authority for behavior:** `docs/design/`  
**Backlog when queue is thin:** `docs/mvp-finish-todo.md` (P3), `docs/ROADMAP.md`, live codebase reconnaissance (see `.cursor/skills/v0-sprint-orchestrator/SKILL.md`).

**Work mix:** Prefer **FEATURE / TESTS / BUGFIX** over docs-only. At most **one DOCS-only** task per iteration unless the user requests a docs pass.

---

## Queue policy

### Capacity

| Limit | Value |
|-------|--------|
| **Active tasks** (unchecked in table below) | **8 max** |
| **Workers per iteration** | 1–3 (no overlapping files) |

### When the queue is full (8 unchecked tasks)

1. **Do not add** new rows until a task is marked `[x]`, moved to **Deferred**, or cancelled with a one-line reason in the handoff.
2. **Still run iterations** — pull work only from the **top unchecked** tasks (by priority order in the table).
3. **One-off fixes** (flaky test, CI red on `trunk`) may be fixed **without** queueing if they block the sprint; note them in **Handoff notes** and optionally add a retroactive row after the fix.
4. **Replenish** when active count drops below **5**: pull the next **vertical slice** from `mvp-finish-todo.md` P3 or from recon (missing test, verifier gap, diagnostic golden).
5. **Docs-only rustdoc** does **not** get queue slots while active FEATURE/TESTS tasks remain — use the optional **Backlog (unscheduled)** section instead.

### Task lifecycle

```text
Backlog (mvp-finish / recon) → Active queue (this file) → Worker branch + PR → [x] + handoff → Completed log
```

- Mark `[x]` only after gates pass and acceptance criteria are met (`just pre-commit` when semantics change).
- **BLOCKED** tasks: leave `[ ]`, add `**Blocked:** …` under the task body; orchestrator picks a different task.
- **Split** a task that grew too large: mark original `[x]` with “split into #N/A, #N/B”, add child rows.

---

## Active queue

Work **top to bottom** within each priority band. Orchestrator may run up to three **non-overlapping** tasks per iteration.

| # | Task | Stream | Priority | Status |
|---|------|--------|----------|--------|
| 20 | Golden diagnostic — release build without source spans | TESTS | P3 | `[ ]` |

**Optional (queue when &lt; 5 active):**

| # | Task | Stream | Status |
|---|------|--------|--------|
| — | _Replenish from Backlog when #20 completes_ | — | — |

---

## Task 6 — PHX-070-p4: Release strip of section 5

**Stream:** FEATURE  
**Design:** `docs/design/features/debug.md` (Layer 2, release profile)  
**Depends on:** PHX-070-p1/p2 on trunk (`PcSpanTable`, CLI `format_vm_error`, link `merge_from`)

### Goal

`phx build --release` (or equivalent profile flag) produces PHX0 **without** section 5; verifier accepts stripped modules; dev builds still emit spans.

### Work

1. Wire release profile through `build` driver / codegen to omit PC span emission.
2. Clear or omit `PHX0_HAS_DEBUG` when no debug sections are written.
3. Add tests: release artifact has no section 5; `verify()` passes; dev artifact still has spans.
4. Run `just pre-commit`.

### Acceptance

- Release-linked module verifies and runs; no section 5 payload.
- Dev build unchanged for `heap_uaf` source-map integration test.
- `just pre-commit` green.

---

## Task 7 — PHX-070-p5: PC span map for nested / indirect calls

**Stream:** FEATURE  
**Design:** `docs/design/features/debug.md` (Phase 1 PC span map)

### Goal

VM faults in **callees** (not only entry / top frame) resolve to the correct Phoenix source line when section 5 is present.

### Work

1. Audit codegen span recording for `Call` / `CallIndirect` sites and callee bodies.
2. Fix gaps so `(function_id, pc)` in errors from nested calls map via merged `PcSpanTable`.
3. Add integration test: program with helper that traps; stderr shows helper’s `.phx` line (new fixture or extend `heap_uaf`).
4. Run `just pre-commit`.

### Acceptance

- New or extended integration test fails on trunk before fix and passes after.
- `format_vm_error` output includes `path:line:col` for the faulting callee.

---

## Task 8 — PHX-borrow-0: `&mut T` exclusivity

**Stream:** FEATURE  
**Design:** `docs/design/features/ownership.md`, ROADMAP deferred borrow checker

### Goal

First borrow-checker slice: reject **two overlapping `&mut T`** to the same binding (no lifetime syntax).

### Work

1. Type-check `&mut` borrows with an exclusivity map (creation + use sites).
2. Emit a clear `TypeCheckError` (new code or reuse closest existing) with spans on both borrows.
3. Unit tests in `source/phx-compiler/tests/typeck.rs` (positive + negative).
4. Run `just pre-commit`.

### Acceptance

- Minimal `.phx` fixture that takes two `&mut` aliases of one `var` fails type-check.
- Non-conflicting sequential borrows still accepted (if spec allows) or documented as follow-up.

---

## Task 9 — PHX-borrow-1: `&T` shared borrow

**Stream:** FEATURE  
**Design:** `docs/design/features/ownership.md`  
**Depends on:** Task 8 (shared infrastructure)

### Goal

Allow multiple `&T` to the same binding; reject `&T` + `&mut T` overlap.

### Work

1. Extend borrow map for shared vs exclusive.
2. Tests: two `&T` OK; `&T` + `&mut T` error.
3. Run `just pre-commit`.

### Acceptance

- Table-driven typeck tests; no new surface syntax.

---

## Task 10 — PHX-sched-0: Scheduler skeleton

**Stream:** FEATURE  
**Design:** `docs/design/features/runtime-transparency.md`, `docs/design/mvp.md` shipping order

### Goal

In-tree **scheduler data structures** and a **single-threaded** park/resume harness — **no** Phoenix syntax, no std I/O yet.

### Work

1. Add `phx-vm` (or `phx-runtime` if already scaffolded) module: runnable contexts, run queue, park reason enum.
2. Unit tests: spawn N contexts, park one, resume, complete — all on one OS thread.
3. Document public Rust API in module rustdoc (Tier-A).
4. Run `just test` (no language semantics change → `just pre-commit` optional unless touching compiler).

### Acceptance

- Tests pass without new opcodes or `phx` CLI changes.
- No new crates.io deps.

---

## Task 11 — PHX-sched-1: Schedulable I/O contract doc

**Stream:** INFRA  
**Design:** `docs/design/features/runtime-transparency.md`

### Goal

Document the **contract** between future std I/O and the scheduler (park points, wakeup, error propagation) so Task 10+ can proceed without ad-hoc design.

### Work

1. Add a “Schedulable I/O” subsection to `runtime-transparency.md` (or linked doc): who calls `park`, what wakes a context, how errors surface.
2. Cross-link from `mvp-finish-todo.md` P3 scheduler item.
3. No code changes required; if only docs, `just fmt-check` N/A for md — skip `pre-commit` unless Rust touched.

### Acceptance

- Design doc PR reviewable in isolation; orchestrator can queue implementation slices after merge.

---

## Task 12 — Golden diagnostic: double mutable borrow

**Stream:** TESTS  
**Depends on:** Task 8

### Goal

CLI golden locks user-visible diagnostic for double `&mut` borrow.

### Work

1. `tests/integration/diagnostics/double_mut_borrow.phx` + `.stderr` golden.
2. Register in `phx-test` diagnostic case list.
3. Run `just pre-commit`.

### Acceptance

- `cargo test -p phx-integration-tests --test diagnostics` passes.

---

## Task 13 — Verifier: section 5 / stripped module cases

**Stream:** TESTS  
**Design:** `docs/design/features/vm-linear.md`

### Goal

Mutation-style tests proving verifier accepts valid stripped modules and rejects corrupt section 5.

### Work

1. Extend `phx-bytecode` tests: truncated section 5, bad `sub_version`, overlapping entries.
2. Assert stable `VerifyError` / `PcSpanError` kinds — no panic.
3. Run `just pre-commit`.

### Acceptance

- At least 3 negative cases; workspace tests green.

---

## Backlog (unscheduled)

_Not counted toward the 8-task cap._ Orchestrator promotes items here when the active queue has fewer than 5 tasks.

| Idea | Stream | Source |
|------|--------|--------|
| `phx explain` gap for new borrow errors | QOL | recon |
| Tier-A rustdoc on one **complex** pass (only if no FEATURE queued) | DOCS | last resort |
| Website a11y / copy | WEBSITE | `website/` submodule |
| Actors / mailboxes | — | **Deferred** — post-MVP, no slice without design |

---

## Completed — audit phase (2026-06-20)

| # | Task | Notes |
|---|------|-------|
| 1 | Lower layout `unwrap_or(0)` → `LowerError` | PHX-030/041 |
| 2 | Golden: discarded std `Option` (E2042) | PHX-061 |
| 3 | Golden: loop-carried use-after-move | PHX-024 |
| 4 | Golden: if-arm ownership join | PHX-023 |
| 5 | PXI malformed-export rejection tests | PHX-040, PR #14 |

## Completed — sprint / PHX-070 (already on trunk)

| Item | Notes |
|------|-------|
| PHX-070-p1 | `PcSpanTable` / section 5 codegen — PR #12 |
| PHX-070-p2 | CLI `format_vm_error` — PR #13 |
| PHX-070-p2b | Link `PcSpanTable::merge_from` in `link/mod.rs` |
| PHX-070-p2c | Integration `sourcemap_run.rs` / `heap_uaf` CLI span test |

_Do not re-queue the above unless a regression appears._

## Completed — sprint iteration 1 (2026-06-20)

| # | Task | Notes |
|---|------|-------|
| 6 | PHX-070-p4 — Release strip of PHX0 section 5 | PR #101, #121, #123 |
| 7 | PHX-070-p5 — PC span map across nested / indirect calls | PR #103 |
| 8 | PHX-borrow-0 — `&mut T` exclusivity | PR #106 |
| 9 | PHX-borrow-1 — `&T` shared borrow + conflict diagnostic | PR #111, #115 |
| 10 | PHX-sched-0 — Scheduler types + park/resume harness | PR #104 |
| 11 | PHX-sched-1 — Schedulable I/O contract doc | PR #113 |
| 12 | Golden diagnostic — double mutable borrow | PR #110, #119 |
| 13 | Verifier — hostile PHX0 section 5 / stripped-module cases | PR #102 |
| 14 | README + CONTRIBUTING sync | PR #118 |
| 15 | Stabilize flaky `heap_uaf_cli_shows_source_span_on_stderr` | PR #112, #122 |

_Also on trunk from same sprint window: PHX-sched-2 I/O wait registry stub — PR #116._

## Completed — sprint iteration 2 (2026-06-20)

| # | Task | Notes |
|---|------|-------|
| 16 | PHX-sched-3 — AwaitIo opcode contract doc | PR #127 |
| 17 | CLI `--release` e2e integration test | PR #124 |
| 18 | mvp-finish P3 doc truth (ROADMAP + agent queue) | this PR |
| 19 | PHX-borrow-2 — loop / branch borrow join | PR #128 |

---

## Task 16 — PHX-sched-3: AwaitIo opcode contract doc

**Stream:** INFRA  
**Design:** `docs/design/features/vm-linear.md`, `docs/design/features/runtime-transparency.md`  
**Depends on:** PHX-sched-0 (#104), PHX-sched-1 (#113), PHX-sched-2 (#116)

### Goal

Document the **AwaitIo** / scheduler-wakeup opcode contract in `vm-linear.md` so codegen and std I/O slices share one normative reference.

### Work

1. Add opcode stub subsection: stack effect, park reason mapping, wakeup invariants.
2. Cross-link from `runtime-transparency.md` Schedulable I/O contract.
3. No Rust changes required unless a doc example needs a one-line rustdoc pointer.

### Acceptance

- Design doc PR reviewable in isolation; orchestrator can queue opcode implementation after merge.

---

## Task 17 — CLI `--release` e2e integration test

**Stream:** TESTS  
**Design:** `docs/design/features/debug.md` (release profile)  
**Depends on:** PHX-070-p4 (#101, #121, #123)

### Goal

End-to-end test: `phx build --release` + `phx run` on a minimal fixture; artifact verifies, runs, and stderr omits dev source spans.

### Work

1. Add `tests/integration/tests/release_build.rs` (or extend existing) with release profile build + run.
2. Assert no section 5 in linked PHX0; VM fault (if triggered) has no `path:line:col` mapping.
3. Run `just pre-commit`.

### Acceptance

- Integration test passes on trunk; fails if release strip regresses.

---

## Task 18 — mvp-finish P3 doc truth

**Stream:** INFRA  
**Design:** `docs/mvp-finish-todo.md`, `docs/ROADMAP.md`

### Goal

Sync P3 rows with trunk: PHX-070 partial completion, borrow-checker phase-0 slices, scheduler harness + I/O contract.

### Work

1. Update P3 checkboxes and sub-bullets in `mvp-finish-todo.md`.
2. Align ROADMAP PHX-070 / borrow / sched deferred rows with shipped PRs.
3. No Rust changes.

### Acceptance

- P3 PHX-070 shows partial completion; borrow and sched rows reflect #104–#116, #106–#111.

---

## Task 19 — PHX-borrow-2: loop / branch borrow join

**Stream:** FEATURE  
**Design:** `docs/design/features/ownership.md`  
**Depends on:** PHX-borrow-0 (#106), PHX-borrow-1 (#111)

### Goal

First join-rules slice: reject overlapping borrows across **if** arms or **loop** bodies where control-flow merge would alias `&mut T`.

### Work

1. Extend borrow map with branch/loop join points (minimal — no lifetime syntax).
2. Unit tests in `source/phx-compiler/tests/typeck.rs`.
3. Run `just pre-commit`.

### Acceptance

- Fixture with borrows in both if arms fails type-check; sequential non-overlapping borrows still OK.

---

## Task 20 — Golden diagnostic: release build without source spans

**Stream:** TESTS  
**Depends on:** PHX-070-p4 (#101, #121, #123), Task 17

### Goal

Golden locks CLI stderr for a release-built program that traps: no Phoenix source line mapping in output.

### Work

1. `tests/integration/diagnostics/` fixture built with release profile + `.stderr` golden.
2. Register in diagnostic or cli_e2e case list.
3. Run `just pre-commit`.

### Acceptance

- Test passes on trunk; catches accidental re-emission of section 5 in release builds.

---

## Handoff notes (orchestrator fills in)

| Field | Value |
|-------|-------|
| Last completed task | #16–#19 (iteration 2; #18 = ROADMAP/agent queue sync) |
| Last trunk SHA | `2921773c` |
| Active queue count | 1 / 8 |
| Blockers | — |
| Next replenish | When #20 complete → pull from Backlog (unscheduled) |
