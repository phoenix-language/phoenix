---
name: v0-sprint-orchestrator
description: >-
  Autonomous multi-agent sprint for Phoenix. A master orchestrator plans a task
  queue each iteration, spawns parallel worker agents on isolated branches, runs
  fmt/tests, commits, pushes, and opens PRs. Use on loop ticks or when
  resuming a sprint iteration.
---

# V0 Sprint Orchestrator

You are the **master orchestrator**. Each iteration you **plan**, **delegate**, **integrate**, and **replenish the queue** — not only execute a single frozen checklist item.

**Authority for language behavior:** `docs/design/` — especially `mvp.md`, `language-v0.md`, and feature docs. **Work discovery:** `docs/ROADMAP.md`, `docs/mvp-finish-todo.md`, `docs/AGENT_TASKS.md`, open PRs, **and live codebase reconnaissance** (see § Task discovery).

---

## Roles

| Role | Who | Responsibility |
|------|-----|----------------|
| **Master orchestrator** | This agent on each loop tick | Orient on `trunk`, **recon the codebase**, build/update the iteration task list, spawn workers, open missing PRs, **batch-merge green PRs after the full iteration completes**, mark done items, start next iteration when idle |
| **Worker subagent** | One Task per task | Single scoped task on an isolated branch: implement → fmt → test → commit → push → PR |

Workers do **one task per branch**. The master may queue 1–3 parallel workers when tasks do not overlap files.

---

## Work mix (critical — do not default to docs)

Sprints that spend **>30% of iterations on rustdoc-only PRs** are off-target. The orchestrator **must** bias toward **features, tests, bugs, and infra**.

### Per-iteration default (3 workers)

| Slot | Stream | Examples |
|------|--------|----------|
| **1–2** | **Feature / test / bugfix** (required) | PHX-070 slice, golden diagnostic, verifier gap, mono edge case, CLI behavior |
| **0–1** | **Docs / QOL** (optional) | rustdoc on one thin module, README fix, `phx explain` gap |

**Rules:**

- **At most one docs-only worker** per iteration. If the **last two** iterations were all-docs, the **next must have zero** docs-only workers.
- If the last **two** iterations were all-docs, the **next iteration must be zero docs-only workers**.
- Tag each planned task with a **stream**: `FEATURE` | `TESTS` | `BUGFIX` | `INFRA` | `DOCS` | `WEBSITE`.

---

## Sprint constraints

- **Duration:** Loop until `END_EPOCH` (default 10 hours from sprint start).
- **Base branch:** `trunk` (clean before spawning).
- **Branch naming:** `agent/v0-sprint/<YYYYMMDD>-<task-slug>` (e.g. `…-phx-070-link-span-merge`, `…-golden-loop-uam`, `…-readme-contributing`).
- **Parallelism:** Up to 3 workers per iteration; **no overlapping files** across concurrent workers.
- **Commit format:** `[stage]: description` — e.g. `[typeck]: …`, `[link]: …`, `[tests]: …`, `[docs]: …`.
- **Push + PR:** After gates pass, push branch and open PR to `trunk` (website work: see Website section).
- **Merge policy:** Only the **master orchestrator** may merge into `trunk`, and **only once per iteration** after **all** workers spawned for that iteration have finished (success or BLOCKED). Workers must **never** merge.

### Quality gate (every worker, in order)

```bash
just fmt          # required before commit — run even for comment-only edits in Rust
just fmt-check    # required before push — must pass (same as CI); run after fmt if unsure
just test         # required before push
just pre-commit   # required when compiler, VM, bytecode, diagnostics, or CLI semantics change
```

If `just fmt` modifies files, include those changes in the commit. **Do not push if any gate fails.**

---

## What workers may do (allowed streams)

| Stream | Examples | Typical `[stage]` |
|--------|----------|-------------------|
| **Feature slices** | PHX-070 section 5 link/CLI, borrow-checker phase 0, scheduler skeleton, new opcode + lowering + test | `[codegen]`, `[link]`, `[vm]`, `[typeck]`, `[runtime]` |
| **Roadmap / audit** | PXI malformed tests, silent-fallback cleanup, golden diagnostics | `[lower]`, `[typeck]`, `[tests]` |
| **Bug fixes** | Crashes, wrong diagnostics, flaky tests, verifier gaps | `[vm]`, `[diagnostics]`, `[fix]` |
| **Tests & fixtures** | Coverage gaps, negative tests, integration `.phx` + golden stderr | `[tests]` |
| **Compiler / VM internals** | Refactors with behavior unchanged, split modules, clearer errors | `[refactor]`, `[lower]`, `[typeck]` |
| **Infra / repo** | README, CONTRIBUTING, crate layout notes, CI hardening, `just` recipes | `[infra]`, `[docs]` |
| **Documentation** | Design doc truth, rustdoc on **one** complex pass when no feature work queued | `[docs]` |
| **Developer QOL** | CLI messages, `phx explain` codes, error hints | `[cli]`, `[diagnostics]` |
| **Website (submodule)** | Copy, layout, a11y in `website/` | `[website]` |

Prefer items that are **small, reviewable, and test-backed**. One logical improvement per PR.

---

## Large features — slice, do not boil the ocean

Post-beta and post-MVP items in `docs/mvp-finish-todo.md` **P3** are **allowed** when delivered as **vertical slices** with tests and an existing design doc. Queue the **next shippable slice**, not the whole epic.

### PHX-070 — Bytecode source maps (section 5)

**Design:** `docs/design/features/debug.md` (Phase 1 PC span map). **Partially on trunk:** `PcSpanTable`, dev codegen, CLI `format_vm_error` (PRs #12–#13).

**Example slices (pick one per worker):**

- Link-time merge of `PcSpanTable` across modules; golden PHX0 round-trip
- Release build strips section 5; verifier accepts stripped modules
- CLI integration test: VM trap → Phoenix file:line on stderr
- Extend span map to foreign calls / indirect calls

### Full borrow checker

**Design:** `docs/design/ownership.md`, ROADMAP deferred.

**Example slices:**

- `&T` shared borrow: typeck rules + one positive/negative test (no lifetimes yet)
- `&mut T` exclusivity error when second mutable borrow active
- Diagnostic golden for double mutable borrow

### M:N scheduler + schedulable I/O

**Design:** `docs/design/features/runtime-transparency.md`, `mvp.md` shipping order.

**Example slices:**

- Scheduler data structures + unit tests (no Phoenix syntax yet)
- `park` / `resume` contract on a single OS thread harness
- Std I/O bridge hook points documented + stubbed in VM

**Each slice must:** cite the design section, add tests, stay mergeable in one PR.

---

## What workers must NOT do (forbidden)

- **New language syntax or surface** not in `docs/design/` (no new keywords, types, or std APIs without design update).
- **Whole epics in one PR** — borrow checker, full scheduler, actors, std I/O, JIT in a single branch.
- **Drive-by refactors** unrelated to the task scope.
- **New crates.io dependencies** (project policy: std only).
- **Inventing behavior** when design is ambiguous — worker returns **BLOCKED**; master updates design doc or picks another task.
- **Docs-only sprints** — see Work mix above.

**Still deferred without design + slice plan:** actors/mailboxes, JIT, stable FFI Phase B symbol identity, full std networking.

---

## Task discovery (master builds the queue each iteration)

The master **does not** recycle a static docs backlog. Each iteration, **recon** then **compose** 1–3 tasks.

### 1. Orient (always)

```bash
git fetch origin
git checkout trunk && git pull --ff-only origin trunk
just test
gh pr list --state open
```

Stop if clean `trunk` tests fail (fix or file a blocker task first).

### 2. Codebase reconnaissance (pick 2–4 probes)

Use judgment — not every probe every tick:

| Probe | What to look for | Example task |
|-------|------------------|----------------|
| **Audit queue** | Unchecked rows in `docs/AGENT_TASKS.md` (max 8 active); if full, see queue policy in that file | Feature slice, golden, verifier case |
| **ROADMAP / P3 slices** | Next shippable slice for PHX-070, borrow checker, scheduler | Link span merge test |
| **Test gaps** | Compiler pass with logic but thin `tests/` coverage | New `lower.rs` negative case |
| **Flaky / failing tests** | Recent CI or local flakes | Stabilize `cli_e2e` race |
| **Verifier / bytecode** | `phx-bytecode` verify gaps, mutation tests | New malformed module case |
| **Hot-path risk** | Large `match` on opcodes, `unwrap` in VM interpreter, silent fallbacks | Replace fallback with error |
| **Diagnostics** | `phx explain` missing codes, registry vs `type_error.rs` drift | Add E-code explain text |
| **CLI UX** | `phx run` / `phx build` edge cases without integration tests | New fixture + golden stderr |
| **Package boundaries** | Duplication across `phx-compiler` / `phx-vm` / `phx-cli` | Extract shared helper (no behavior change) |
| **Repo hygiene** | README install steps, CONTRIBUTING gates, `project-layout.mdc` drift | Update contributing rust-docs tier list |
| **Open PRs** | Failing CI, conflicts, review feedback | Fix on same branch |

Record findings in the iteration handoff: **what you checked → what you queued**.

### 3. Priority order (when choosing tasks)

1. **P0/P1 regressions** — broken tests, wrong codegen, announce blockers (`mvp-finish-todo.md` P0–P1).
2. **Feature slices** — PHX-070, borrow checker phase 0, scheduler spike (at least one per 2 iterations when backlog allows).
3. **Active queue** — unchecked rows in `docs/AGENT_TASKS.md` (top-first). If queue full, do not add; pull from P3 only when &lt; 5 active.
4. **Tests & recon** — golden diagnostics, verifier gaps, flakes.
5. **Infra / README / contributing** — when feature queue is thin.
6. **Docs-only rustdoc** — **last**, max one worker, thin modules only.

### 4. De-dupe

Skip items with an open sprint branch or merged PR. Update Completed log below.

---

## Completed sprint items (update as PRs merge)

| ID | Notes |
|----|-------|
| PHX-039 | IR validator — PR #7 |
| PHX-012 | Parse error formatting — PR #10 |
| PHX-013 | `phx explain` E3002–E3005, E2033 — PR #8 |
| PHX-058 | `PHX_ICE_DEBUG` — merged |
| PHX-031 | `typeck/check` split — on trunk |
| PHX-061 | PXI nested generic round-trip — PR #9 |
| PHX-050-adj | verify-on-load — PR #11 |
| PHX-070-p1 | `PcSpanTable` / section 5 — PR #12 |
| PHX-070-p2 | CLI runtime source mapping — PR #13 |
| Sprint 20–33 | Tier-A rustdoc batches — PRs #75–#99 (see git log); **do not repeat module-only docs without feature mix** |

### Suggested next items (features first)

| ID | Task | Stream | Source |
|----|------|--------|--------|
| PHX-070-p3 | Section 5 link-time merge + integration test (VM error → source line) | FEATURE | `debug.md`, `mvp-finish-todo.md` P3 |
| PHX-070-p4 | Release strip of section 5; verifier policy | FEATURE | `debug.md` |
| PHX-borrow-0 | `&T` / `&mut T` exclusivity slice + golden diagnostic | FEATURE | `ownership.md` |
| PHX-sched-0 | Scheduler types + park/resume unit harness (no syntax) | FEATURE | `runtime-transparency.md` |
| PHX-040 | PXI malformed-export rejection tests (if not on trunk) | TESTS | `AGENT_TASKS.md` |
| QOL-infra | README / CONTRIBUTING / architecture doc sync | INFRA | recon |
| QOL-docs | **Only when feature queue empty** — one thin rustdoc module | DOCS | last resort |

---

## Website submodule

`website/` is a **separate git repo** (submodule). See `.cursor/rules/website-submodule.mdc`.

Worker workflow: branch inside submodule → PR to `website` `main` → optional parent submodule bump PR.

---

## Iteration workflow (master)

### 1. Orient + recon

Run orient commands (§ Task discovery). Spend **2–5 minutes** on recon probes before spawning.

### 2. Plan iteration

Write for each task:

- **Stream:** FEATURE | TESTS | BUGFIX | INFRA | DOCS | WEBSITE
- **Goal:** one sentence
- **Acceptance:** testable criteria (test name, CLI output, design section satisfied)
- **Scope:** files/areas; **no overlap** with sibling workers
- **Design ref:** path under `docs/design/` (required for FEATURE slices)

**Self-check:** At least one of {FEATURE, TESTS, BUGFIX} in the batch unless user requested docs-only sprint.

### 3. Spawn workers (parallel)

Launch one Task per task in **one message**. Worker prompt must include:

```
Full Repository Path: /Users/jamallyons/Developer/GitHub/phoenix-lang/phoenix
Branch: agent/v0-sprint/<date>-<slug>
Stream: FEATURE | TESTS | BUGFIX | INFRA | DOCS | WEBSITE

Task: <title>
Goal: <one sentence>
Design ref: <docs/design/...> (FEATURE only)
Acceptance: <testable criteria>
Allowed scope: <files/areas>
Forbidden: whole epics, new syntax without design, new deps, scope creep, docs-only if stream is FEATURE

Workflow:
1. git fetch; checkout -b <branch> from trunk
2. Implement minimal correct diff
3. just fmt → just fmt-check → just test (+ just pre-commit if semantics touched)
4. git commit -m "[stage]: …"; git push -u origin <branch>
5. gh pr create --base trunk …
6. Return: stream, branch, SHA, PR URL, test summary, or BLOCKED: <reason>

Do NOT merge.
```

If Task spawn times out, master **implements or finishes** stalled workers locally (same branch rules).

### 4. Integrate

- Log PR URLs; open missing PRs.
- **Do not merge** until every worker in the iteration has reported.
- When all done → §4a batch merge.

### 4a. Batch merge (master only)

For each successful worker PR: CI green (`rust` SUCCESS), MERGEABLE, rebase if needed, then:

```bash
gh pr merge <num> --merge --delete-branch
```

After batch:

```bash
git checkout trunk && git pull --ff-only origin trunk
just test   # smoke; stop sprint if trunk breaks
```

Update Completed log. Note **stream mix** for this iteration (e.g. `2 FEATURE, 1 TESTS`).

### 5. Loop tick (15 minutes)

On tick: if workers running → skip spawn; else §1–§3.

---

## Stop conditions

- `END_EPOCH` reached
- User asks to stop
- `trunk` broken and not fixable in one iteration
- No suitable tasks remain (report idle backlog + recon notes)

---

## Shell pattern (restart loop)

```bash
END_EPOCH=<unix>; while [ $(date +%s) -lt $END_EPOCH ]; do
  sleep 900
  echo 'AGENT_LOOP_TICK_v0sprint {"prompt":"Run v0-sprint-orchestrator per .cursor/skills/v0-sprint-orchestrator/SKILL.md","end_epoch":'$END_EPOCH',"interval_sec":900}'
done
```

Preserve `END_EPOCH` when restarting mid-sprint.
