---
name: v0-sprint-orchestrator
description: >-
  10-hour autonomous sprint toward Language v0+ and post-beta gates. Each
  iteration picks independent tasks, spawns parallel subagents in isolated
  branches, runs tests, commits, pushes, and opens PRs. Use when continuing a v0
  sprint loop tick or starting a new sprint iteration.
---

# V0 Sprint Orchestrator

Autonomous multi-agent sprint for the Phoenix compiler. **Authority:** `docs/design/language-v0.md`, `docs/design/language-v0-completion-roadmap.md`, `docs/mvp-finish-todo.md`, `docs/ROADMAP.md`.

## Sprint constraints

- **Duration:** Loop until `END_EPOCH` (set when the loop starts; default 10 hours).
- **Base branch:** `trunk` (must be clean before spawning agents).
- **Parallelism:** Up to 3 subagents per iteration on **independent** tasks (no overlapping files).
- **Isolation:** Each subagent works on `agent/v0-sprint/<YYYYMMDD>-<task-id>-<slug>`.
- **Quality gate before push:** `just test` must pass. For compiler/VM semantic changes, also run `just pre-commit`.
- **Commit format:** `[stage]: description` (e.g. `[typeck]: split expr checking into submodule`).
- **Push:** Push the feature branch to `origin` after tests pass. Do **not** merge to `trunk` automatically.
- **Pull request:** After a successful push, open a PR to `trunk` with `gh pr create` (see PR template below). If a PR already exists for the branch, skip creation and return its URL.
- **Design rule:** Never invent language semantics — update design docs first when behavior is ambiguous.

## Current priority backlog (post Language v0)

Work top-to-bottom within a priority tier. Skip items already in flight on an open branch.

### Tier A — M8 contributor readiness (ROADMAP M8)

| ID | Task | Stage | Notes |
|---|---|---|---|
| PHX-039 | IR validator (terminator-last, target-in-range, optional depth sim) in debug builds | lower/ir | **Done** — PR #7 |
| PHX-012 | Move parse-error formatting into `phx-diagnostics` | diagnostics | **Done** — PR #10 |
| PHX-013 | Complete `phx explain` for E3002–E3005, E2033 | diagnostics/cli | **Done** — PR #8 |
| PHX-058 | ICE handler debug escape hatch (`PHX_ICE_DEBUG`) | cli | **Done** — PR #6 |
| PHX-031 | Split `typeck/check.rs` into `check/{decl,impl,expr,stmt,pattern,intrinsic}.rs` | typeck | **Done** on trunk (`c36ada0`) |

### Tier B — Post-beta foundations

| ID | Task | Stage | Notes |
|---|---|---|---|
| PHX-070 | PHX0 section 5 source maps: encode `(function_id, pc) → span` at link; CLI maps runtime errors | codegen/link/cli | Read `docs/design/features/debug.md` first |
| PHX-061 | PXI round-trip property tests for nested generic types | tests/pxi | **Done** — PR #9 |
| PHX-050-adj | `load_project_binary` optional verify-on-load | vm/cli | **Done** — PR #11 |

### Tier C — Explicitly deferred (do not start)

Scheduler, full borrow checker, actors, std I/O, JIT, stable FFI Phase B — see `mvp-finish-todo.md` P3.

## Iteration workflow (every loop tick)

### 1. Orient (5 min)

```bash
git fetch origin
git checkout trunk && git pull --ff-only origin trunk
just test
```

If `trunk` is dirty or tests fail on clean trunk, fix or stop the sprint and report.

### 2. Select tasks (parallel-safe)

Pick 1–3 items from Tier A/B that:

- Have no file overlap (e.g. do not run PHX-031 and another typeck edit together).
- Have clear acceptance criteria in ROADMAP or mvp-finish-todo.
- Fit in ~30–90 minutes of agent work.

Record selected IDs for this iteration.

### 3. Spawn subagents (parallel)

Launch one Task per task with `subagent_type: "generalPurpose"` (or `best-of-n-runner` when comparing approaches). Each prompt must include:

```
Full Repository Path: /Users/jamallyons/Developer/GitHub/phoenix-lang/phoenix
Branch: agent/v0-sprint/<date>-<task-id>-<slug> (create from trunk)

Task ID: <PHX-NNN or V0-NNN>
Goal: <one sentence>
Acceptance: <from ROADMAP>
Design refs: <doc paths>

Workflow:
1. git checkout -b <branch> from trunk
2. Implement minimal correct diff
3. Add/extend tests for observable behavior
4. Run `just test`; if compiler/VM semantics changed, run `just pre-commit`
5. If green: commit with [stage]: message, push -u origin <branch>
6. Open PR to trunk (skip if one already exists):
   gh pr create --base trunk --head <branch> --title "[<task-id>] <short title>" --body "$(cat <<'EOF'
   ## Summary
   - <bullet 1>
   - <bullet 2>

   ## Test plan
   - [x] `just test` green
   - [ ] Reviewer: spot-check acceptance criteria for <task-id>

   Automated v0 sprint agent — do not merge without review.
   EOF
   )"
7. Return: branch name, commit SHA, PR URL, test summary

Do NOT merge to trunk. Do NOT push if tests fail. Do NOT skip PR creation after a successful push.
```

Launch all tasks in **one message** so they run concurrently.

### 4. Integrate results

When subagents return:

- If all green: log branches pushed and PR URLs. If a subagent forgot to open a PR, open it from the orchestrator with `gh pr create` using the subagent's bullet summary.
- If any failed: log failure; do not push failed work; leave branch for manual review or retry next iteration.
- Update this skill's mental backlog — mark completed items.

**Early next iteration:** When all subagents for the current iteration have finished (success or failure) and `date +%s` < `END_EPOCH`, **immediately** start §1 again. Do not wait for the 15-minute tick.

### 5. Schedule next tick

If `date +%s` < `END_EPOCH`:

- **Idle + agents done:** Start the next iteration immediately (see §4).
- **Agents still running:** Do not spawn a new batch. The 15-minute heartbeat will wake you to check again.
- **On `AGENT_LOOP_TICK_v0sprint`:** If subagents are still in flight → skip spawn, log status, wait for the next tick. If idle → run §1–§3 (new iteration).

Tick interval: **15 minutes** (`sleep 900`). This is a poll when work may still be running, not a mandatory delay between iterations.

## Subagent spawn template

Use the Task tool:

```
description: "V0 sprint: <task-id>"
subagent_type: generalPurpose
prompt: |
  <full prompt from section 3>
```

For risky refactors with multiple valid approaches, use `best-of-n-runner` instead (2 approaches max per task).

## Stop conditions

Stop the sprint loop when:

- `END_EPOCH` reached
- User asks to stop
- `trunk` tests fail and cannot be fixed in one iteration
- No Tier A/B tasks remain

## Loop sentinel

The background shell emits every **15 minutes** (or until `END_EPOCH`):

```
AGENT_LOOP_TICK_v0sprint {"prompt":"Run v0-sprint-orchestrator iteration","end_epoch":<unix>,"interval_sec":900}
```

On wake: read this skill. If subagents from the current iteration are still running, **skip spawning** and wait for the next tick or subagent completion. If idle and before `end_epoch`, run §1–§3. When subagents finish early, start the next iteration immediately without waiting for the tick.

Shell pattern (preserve `END_EPOCH` when restarting mid-sprint):

```bash
END_EPOCH=<unix>; while [ $(date +%s) -lt $END_EPOCH ]; do
  sleep 900
  echo 'AGENT_LOOP_TICK_v0sprint {"prompt":"...","end_epoch":'$END_EPOCH',"interval_sec":900}'
done
```
