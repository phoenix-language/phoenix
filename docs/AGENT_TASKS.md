# Agent task queue

**Updated:** 2026-06-20  
**Source:** `docs/ROADMAP.md` Milestones 0–1 and 7 (foundation integrity, diagnostics)

Work tasks **top to bottom**. Mark a task `[x]` only after `just pre-commit` passes and the acceptance criteria are met. Do **one task per loop iteration** — do not start the next task in the same run if the current one is unfinished.

---

## Status

| # | Task | Status |
|---|------|--------|
| 1 | Lower layout `unwrap_or(0)` → `LowerError` | `[x]` |
| 2 | Golden diagnostic: discarded std `Option` (E2042) | `[x]` |
| 3 | Golden diagnostic: loop-carried use-after-move | `[x]` |
| 4 | Golden diagnostic: if-arm ownership join | `[x]` |
| 5 | PXI malformed-export rejection tests | `[x]` |

---

## Task 1 — Replace lower layout `unwrap_or(0)` fallbacks

**Finding:** PHX-030 / PHX-041 (silent fallback cleanup)  
**Priority:** Milestone 0 — no silent wrong bytecode from missing layout metadata

### Problem

Several lowering paths encode `type_id: 0` or `field_index: 0` when layout lookup fails instead of recording a `LowerError`. That can miscompile instead of failing loudly.

### Files

- `source/phx-compiler/src/lower/expr/assign.rs`
- `source/phx-compiler/src/lower/expr/call.rs`
- `source/phx-compiler/src/lower/expr/match.rs`
- `source/phx-compiler/src/lower/expr/mod.rs`
- `source/phx-compiler/src/lower/expr/intrinsic.rs` (if applicable)

Search for `.unwrap_or(0)` on `type_id_for_named`, `type_id`, and `struct_field_index`.

### Work

1. On layout miss, push an appropriate `LowerError` (add a variant if needed) and **return without emitting** the bad instruction.
2. Add or extend unit tests in `source/phx-compiler/tests/lower.rs` that prove lowering fails (bag contains error) when layout metadata is inconsistent — do not rely on bytecode with operand `0`.
3. Run `just pre-commit`.

### Acceptance

- No production `unwrap_or(0)` remains on layout/type-table lookups in the files above.
- At least one new lowering test covers a miss path.
- `just pre-commit` green.

---

## Task 2 — Golden diagnostic: discarded std `Option`

**Finding:** M7 / PHX-061 (golden diagnostics)  
**Priority:** Milestone 7 — std value discard errors are user-visible and regression-locked

### Problem

`DiscardedStdResult` (E2041) has a golden fixture (`tests/integration/diagnostics/discarded_std_result.*`). `DiscardedStdOption` (E2042) does not.

### Work

1. Add `tests/integration/diagnostics/discarded_std_option.phx` — a minimal program that calls a std function returning `Option` and discards it as a statement (mirror the Result fixture pattern).
2. Add matching `discarded_std_option.stderr` golden output.
3. Register the case in `tests/phx-test` diagnostic case list (same pattern as `discarded_std_result`).
4. Run `just pre-commit` (and `UPDATE_GOLDEN=1` only if you intentionally change unrelated goldens).

### Acceptance

- `cargo test -p phx-integration-tests --test diagnostics` passes.
- Golden mentions E2042 / discarded std Option wording consistent with `phx-diagnostics` registry.

---

## Task 3 — Golden diagnostic: loop-carried use-after-move

**Finding:** PHX-024 (Critical) + PHX-061  
**Priority:** Milestone 0 — loop back-edge move semantics are decided and tested

### Problem

Unit tests exist in `source/phx-compiler/tests/typeck.rs` (`loop_move_then_use_after_loop_errors`, etc.) but there is no CLI golden diagnostic fixture for loop-carried moves.

### Work

1. Add `tests/integration/diagnostics/loop_move_use_after_loop.phx` — outer `var` moved inside `loop { … }`, then used after the loop (must error with use-after-move citing the move site).
2. Add `loop_move_use_after_loop.stderr` golden.
3. Register in diagnostic cases.
4. Run `just pre-commit`.

### Acceptance

- Golden shows use-after-move diagnostic with span on the move site inside the loop.
- Matches `docs/design/features/ownership.md` loop semantics (conservative: move-in-loop of outer binding is always an error).

---

## Task 4 — Golden diagnostic: if-arm ownership join

**Finding:** PHX-023 (Critical) + PHX-061  
**Priority:** Milestone 0 — fork/join ownership across branches

### Problem

`OwnershipTracker::join_arms` is implemented, but CLI goldens do not lock the two key user-visible behaviors:
- Sibling branch must **not** cause a false use-after-move (only one arm runs).
- Binding moved on **any** arm must be treated as moved after the `if`/`match`.

### Work

Add **two** golden pairs (four files total):

1. **`if_branch_sibling_no_false_uam`** — e.g. `if cond { use(x); } else { drop(x); }` then `use(x)` after the `if` must **error** (join marks moved), not silently pass.
2. **`if_branch_untaken_no_move`** — e.g. only the non-moving arm is reachable by construction *or* document with a program where one arm moves and the other uses, and use-after-`if` errors. Pick the clearest minimal program; mirror existing typeck tests if helpful.

Register both cases. Run `just pre-commit`.

### Acceptance

- Two new golden diagnostic fixtures pass in integration diagnostics test.
- Behavior matches flow-insensitive join documented in `ownership.md`.

---

## Task 5 — PXI malformed-export rejection tests

**Finding:** PHX-040  
**Priority:** Milestone 3 — reject malformed `.pxi` instead of partial-parse

### Problem

`source/phx-compiler/src/pxi/format.rs` returns `PxiError` for bad input, but there are no dedicated unit tests proving malformed exports fail cleanly.

### Work

1. Add `source/phx-compiler/tests/pxi.rs` (or extend an existing test module) with table-driven cases:
   - unsupported `format_version`
   - missing required fields (`logical_module`, `source_hash`, …)
   - truncated / malformed JSON fragments
2. Assert `PxiFile::parse` returns `Err(PxiError::…)` with stable error kind — no panic.
3. Run `just pre-commit`.

### Acceptance

- At least 3 malformed-input cases covered.
- All tests pass; no new dependencies.

---

## Handoff notes (agent fills in)

| Field | Value |
|-------|-------|
| Last completed task | 5 — PXI malformed-export rejection tests (PHX-040, PR #14) |
| Last green commit | eaec68d |
| Branch | agent/v0-sprint/20260620-docs-agent-tasks |
| Blockers | — |
