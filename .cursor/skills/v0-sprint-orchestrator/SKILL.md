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

**Authority for language behavior:** `docs/design/` — especially `mvp.md`, `language-v0.md`, and feature docs. **Work discovery:** `docs/ROADMAP.md`, `docs/mvp-finish-todo.md`, `docs/AGENT_TASKS.md`, open PRs, and the QOL categories below.

---

## Roles

| Role | Who | Responsibility |
|------|-----|----------------|
| **Master orchestrator** | This agent on each loop tick | Orient on `trunk`, build/update the iteration task list, spawn workers, open missing PRs, mark done items, start next iteration when idle |
| **Worker subagent** | One Task per task | Single scoped task on an isolated branch: implement → fmt → test → commit → push → PR |

Workers do **one task per branch**. The master may queue 1–3 parallel workers when tasks do not overlap files.

---

## Sprint constraints

- **Duration:** Loop until `END_EPOCH` (default 10 hours from sprint start).
- **Base branch:** `trunk` (clean before spawning).
- **Branch naming:** `agent/v0-sprint/<YYYYMMDD>-<task-slug>` (e.g. `…-phx-040-pxi-malformed`, `…-docs-typeck-ownership`, `…-website-hero-copy`).
- **Parallelism:** Up to 3 workers per iteration; **no overlapping files** across concurrent workers.
- **Commit format:** `[stage]: description` — e.g. `[typeck]: …`, `[docs]: …`, `[infra]: …`, `[website]: …`.
- **Push + PR:** After gates pass, push branch and open PR to `trunk` (website work: see Website section — PR targets `website` repo `main`, then optional parent submodule bump).
- **Do not merge to `trunk` automatically.**

### Quality gate (every worker, in order)

```bash
just fmt          # required before commit — run even for comment-only edits in Rust
just fmt-check    # required before push — must pass (same as CI); run after fmt if unsure
just test         # required before push
just pre-commit   # required when compiler, VM, bytecode, diagnostics, or CLI semantics change
```

If `just fmt` modifies files, include those changes in the commit. **Do not push if any gate fails.** CI runs `cargo fmt --all --check` — a green `just test` alone is not enough.

---

## What workers may do (allowed)

Improve **existing** code and docs without changing language semantics:

| Stream | Examples | Typical `[stage]` |
|--------|----------|-------------------|
| **Roadmap / audit items** | PHX-040 PXI malformed tests, silent-fallback cleanup, golden diagnostics | `[lower]`, `[typeck]`, `[tests]` |
| **Bug fixes** | Crashes, wrong diagnostics, test failures, verifier gaps | `[vm]`, `[diagnostics]`, … |
| **Compiler internals** | Refactors, split large modules, remove duplication, clearer errors | `[lower]`, `[typeck]`, `[codegen]` |
| **Documentation** | Design doc truth, rustdoc on complex passes, README/command docs | `[docs]` |
| **Comments & readability** | Explain non-obvious invariants in existing code; rename for clarity **without** behavior change | `[refactor]` |
| **Tests & fixtures** | Coverage gaps, negative tests, integration fixtures | `[tests]` |
| **Performance** | Hot-path improvements with benchmarks or tests proving no semantic change | `[perf]` |
| **Website (submodule)** | Copy, layout, accessibility, static assets in `website/` | `[website]` in submodule repo |
| **Developer QOL** | Better CLI messages, `phx explain` gaps, error hints | `[cli]`, `[diagnostics]` |

When picking work, prefer items that are **small, reviewable, and test-backed**. One logical improvement per PR.

---

## What workers must NOT do (forbidden)

- **New language syntax or semantics** not already in `docs/design/` (no new keywords, types, opcodes, or std surface without an approved design doc update first).
- **Post-MVP features** listed as deferred: scheduler, actors, full borrow checker, std I/O, JIT, stable FFI Phase B — see `mvp-finish-todo.md` P3 and Tier C below.
- **Drive-by refactors** unrelated to the task scope.
- **New crates.io dependencies** (project policy: std only).
- **Inventing behavior** when design is ambiguous — stop and update the design doc, or pick a different task.

If a task would require a design decision, the worker returns **blocked** with a one-paragraph note; the master queues something else.

---

## Task discovery (master builds the queue each iteration)

Each iteration the master **composes a short task list** (1–3 items) from sources in priority order:

1. **Open audit items** — `docs/AGENT_TASKS.md` (unchecked rows), `docs/ROADMAP.md`, `docs/mvp-finish-todo.md`.
2. **Open PR follow-ups** — merge conflicts, CI failures on sprint PRs (fix on same branch).
3. **QOL backlog** — master may add tasks it identifies from codebase reading (see allowed streams above).
4. **Done / in-flight check** — skip items with an open branch or merged PR; update the Completed log below.

Record the iteration plan in the handoff (which tasks, which branches, which workers).

### Tier C — Explicitly deferred (never queue)

Scheduler, full borrow checker, actors, std I/O, JIT, stable FFI Phase B, new syntax — see `mvp-finish-todo.md` P3.

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
| PHX-070-p2 | CLI runtime source mapping — PR #13 (stacks #12) |

### Suggested next items (master picks from here or discovers new QOL work)

| ID | Task | Source |
|----|------|--------|
| PHX-040 | PXI malformed-export rejection tests (≥3 cases) | `docs/AGENT_TASKS.md` task 5 |
| PHX-070-p3 | Merge stack #12+#13 or rebase after review feedback | open PRs |
| QOL-docs | rustdoc pass on one complex module (e.g. `lower/`, `ownership.rs`) | master discretion |
| QOL-readability | Split or clarify one hot file without behavior change | master discretion |
| QOL-website | Static site copy, a11y, or layout in `website/` submodule | master discretion |

---

## Website submodule

`website/` is a **separate git repo** (submodule). See `.cursor/rules/website-submodule.mdc`.

Worker workflow for website tasks:

1. `cd website && git checkout main && git pull`
2. Branch: `agent/v0-sprint/<date>-website-<slug>` **inside the submodule**
3. Edit static assets; run any local checks the site has
4. `just fmt` N/A in parent; commit in submodule, push, open PR against `phoenix-language/website` `main`
5. Optionally bump submodule pointer in parent: `[infra]: bump website submodule` (separate small PR to phoenix `trunk`)

Website changes do **not** require `just pre-commit` on the parent unless the submodule pointer is bumped.

---

## Iteration workflow (master)

### 1. Orient

```bash
git fetch origin
git checkout trunk && git pull --ff-only origin trunk
just test
```

Stop the sprint if clean `trunk` tests fail.

### 2. Plan iteration

- Read completed log and open PRs (`gh pr list`).
- Pick 1–3 **parallel-safe** tasks from discovery sources.
- Write a one-line goal + acceptance criteria per task.

### 3. Spawn workers (parallel)

Launch one Task per task in **one message**. Each worker prompt must include:

```
Full Repository Path: /Users/jamallyons/Developer/GitHub/phoenix-lang/phoenix
Branch: agent/v0-sprint/<date>-<slug>

Task: <title>
Goal: <one sentence>
Acceptance: <testable criteria>
Allowed scope: <files/areas>
Forbidden: new syntax, new semantics, new dependencies, scope creep

Workflow:
1. git fetch; checkout -b <branch> from trunk (or website submodule rules)
2. Implement minimal correct diff
3. just fmt
4. just test (+ just pre-commit if compiler/VM/diagnostics/CLI semantics touched)
5. git commit -m "[stage]: …"; git push -u origin <branch>
6. gh pr create --base trunk --head <branch> …  (or website repo PR)
7. Return: branch, SHA, PR URL, test summary, or BLOCKED: <reason>

Do NOT merge. Do NOT push if gates fail.
```

PR body template:

```markdown
## Summary
- …

## Test plan
- [x] `just fmt`
- [x] `just test` green
- [ ] Reviewer: …

Automated sprint agent — do not merge without review.
```

### 4. Integrate

- Log PR URLs; open any missing PRs from worker summaries.
- Mark completed items in the Completed log (this file or tell user).
- **If all workers finished and before `END_EPOCH`:** go to §1 immediately (early iteration).
- **If workers still running:** wait; do not spawn overlapping work.

### 5. Loop tick (15 minutes)

Background sentinel every **900s** until `END_EPOCH`:

```
AGENT_LOOP_TICK_v0sprint {"prompt":"Run v0-sprint-orchestrator …","end_epoch":<unix>,"interval_sec":900}
```

On tick: if workers still running → skip spawn; else run §1–§3.

---

## Stop conditions

- `END_EPOCH` reached
- User asks to stop
- `trunk` broken and not fixable in one iteration
- No suitable tasks remain (master reports idle backlog)

---

## Shell pattern (restart loop)

```bash
END_EPOCH=<unix>; while [ $(date +%s) -lt $END_EPOCH ]; do
  sleep 900
  echo 'AGENT_LOOP_TICK_v0sprint {"prompt":"Run v0-sprint-orchestrator per .cursor/skills/v0-sprint-orchestrator/SKILL.md","end_epoch":'$END_EPOCH',"interval_sec":900}'
done
```

Preserve `END_EPOCH` when restarting mid-sprint.
