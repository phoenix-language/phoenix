# MVP finish todo — post architecture review

**Purpose:** Actionable finish list after [phoenix-architecture-review.md](phoenix-architecture-review.md) (70 findings; 68 resolved, 1 doc item open, 1 deferred). This is **not** a duplicate of the review — it tracks what remains to close the **MVP / Language v0 gate** and begin **Std v0** authoring in earnest.

**Authority:** [mvp.md](design/mvp.md), [language-v0-completion-roadmap.md](design/language-v0-completion-roadmap.md), [ROADMAP.md](ROADMAP.md) (M6–M8), [mvp-implementation-checklist.md](mvp-implementation-checklist.md).

**Scope labels used below:**


| Label           | Meaning                                                                                 |
| --------------- | --------------------------------------------------------------------------------------- |
| **MVP**         | Core single-process pipeline per `mvp.md` (already largely shipped)                     |
| **Language v0** | Contributor-ready gate: Phase 7 partials closed, `just pre-commit` green, docs truthful |
| **Std v0**      | Std library authored in Phoenix (`DynamicArray`, `UniquePtr`, owned `String`, …)        |
| **Post-MVP**    | Explicitly out of `mvp.md`; do not implement until gates pass                           |


**Last verified:** 2026-06-21 — `just pre-commit` green on trunk (`8e2ed9d0`); sprint iteration 6 merged multi-module span test (PR #137), M:N worker pool (PR #136), borrow explain coverage (PR #135), loop/if-arm borrow goldens (PR #134), and link test flake stabilization.

---

## P0 — Blockers (Language v0 gate)

These must pass before announcing Language v0 or calling the beta gate complete.

### Compiler regressions (in-flight heap-slice / mono work)

- [x] **Fix expression cursor drift in lowering** — `LowerError::MissingExprType` / cursor drift on `generic_infer.phx` and std builds. **Ref:** PHX-037. Fixed in `f486b64` (fail-closed + mono body typeck), `d04864b` (mono prim_kind fallbacks), `c106981` (cross-crate mono shells + deref lowering). Regression tests in `lower/ctx.rs`. **Gate:** Language v0.

- [x] **Fix `build_std_traits` project build** — `cli_e2e::build_std_traits` green with PHX-037 fixes. **Owner:** compiler + std fixtures. **Gate:** Language v0.

- [x] **Fix generic allocator / `Global` method resolution** — `build_unique_ptr_*` and related mono tests green (2026-06-15). **Ref:** PHX-038, ROADMAP M4. **Owner:** compiler (typeck/mono). **Gate:** Language v0.

- [x] **Fix `DynamicArray` std project builds** — `build_dynamic_array_smoke` and `build_dynamic_array_drop_smoke` green (2026-06-15). **Owner:** compiler + std. **Gate:** Std v0 (blocks DynamicArray authoring).

- [x] **Fix `UniquePtr` std project builds** — `build_unique_ptr_smoke` and `build_unique_ptr_drop_smoke` green (2026-06-15). **Ref:** [allocator.md](design/features/allocator.md). **Owner:** compiler + std. **Gate:** Std v0.

- [x] **Fix return stack depth mismatch in `examples/errors`** — `examples_errors_build_run` green (2026-06-15). **Ref:** PHX-035, ROADMAP M2 stack-at-return invariant. **Owner:** compiler (lower/codegen) + VM verify. **Gate:** Language v0.

- [x] **Restore `run_smoke_fixtures` green** — unblocks single-file CLI regression suite (PHX-037). **Owner:** tests + compiler. **Gate:** Language v0.

### Integration gate

- [x] **All seven failing `cli_e2e` tests pass** — `build_dynamic_array_smoke`, `build_dynamic_array_drop_smoke`, `build_unique_ptr_smoke`, `build_unique_ptr_drop_smoke`, `build_std_traits`, `examples_errors_build_run`, `run_smoke_fixtures`. Verified 2026-06-15. **Owner:** tests (verify) + compiler (fix). **Gate:** Language v0.

- [x] `**cargo test --workspace` fully green** — required by `just pre-commit` / PHX-060. **Owner:** tests. **Gate:** Language v0.

---

## P1 — Beta gate / Language v0 announcement

ROADMAP Milestones 7–8 exit criteria and doc truth. Complete after P0 is green.

### Documentation truth (PHX-032)

- [x] **PHX-032 doc-truth pass** — Sync design docs with implemented behavior: mark V0-063 (trait defaults) and V0-064 (multi-payload `Result` match) **done** in [language-v0-completion-roadmap.md](design/language-v0-completion-roadmap.md); update stale partial rows in [mvp-implementation-checklist.md](mvp-implementation-checklist.md) (survey date, `?` lowering status, opcode counts). **Ref:** PHX-032. **Owner:** docs.

- [x] **Confirm ownership / numeric docs match M0–M2 decisions** — Verify [ownership.md](design/features/ownership.md) documents loop move rule and MVP whole-value move policy; [wide-integers.md](design/features/wide-integers.md) and [type-system.md](design/features/type-system.md) document masked shifts and IEEE NaN; [vm-linear.md](design/features/vm-linear.md) documents stack-at-`RETURN` and `Trap` operand contract. **Ref:** ROADMAP Resolved Design Decisions #1–#4, PHX-032. **Owner:** docs.

- [x] **Document `str` / `Ty::Str` normative Copyable view** — Per ROADMAP M5 / Resolved Decision #6: rodata-backed UTF-8 view, phased migration note in `type-system.md`. **Owner:** docs. **Gate:** Language v0.

### Error-handling polish (ROADMAP M7)

- [x] **Promote discarded `Result`/`Option` from lint warning to type error** — PHX-029 wired typed lint via `StdKernel`; [error-handling.md](design/features/error-handling.md) expects must-use enforcement. E2041/E2042 in typeck; lint retains `#[must_use]` attr only. **Owner:** compiler (typeck/lint). **Gate:** Language v0 (per error-handling design).

- [x] **Golden diagnostics for `?` failure modes** — `.stderr` fixtures for `?` type mismatch (E2029), missing `From` impl (E2031), and discarded `Result` (E2041). **Owner:** tests. **Gate:** Language v0.

### Pre-announce verification

- [x] `**just pre-commit` green on mainline** — fmt, clippy `-D warnings`, dep-check, `cargo test --workspace`, `just test-lang`. **Ref:** PHX-060, project completion gate. **Owner:** CI + all crates. **Gate:** Language v0.

- [x] `**std_platform_smoke` builds and runs** — Phase 7 capstone (Result match + trait defaults + heap slice in one program). Fixture exists; confirm green after P0 fixes. **Ref:** V0-067. **Owner:** tests + std. **Gate:** Language v0.

- [x] `**examples/errors` builds and runs via `just test-lang`** — Primary error-handling demo; currently blocked by stack-depth failure. **Ref:** V0-064, language-v0-completion-roadmap. **Owner:** tests + compiler. **Gate:** Language v0.

---

## P2 — Std v0 bootstrap (post Language v0 announce)

Per [language-v0-completion-roadmap.md](design/language-v0-completion-roadmap.md) Std v0 entry table. **Not MVP blockers** if Phase 7 is green, but required before std collections/text ship.

### Collections and owning types

- [x] **`DynamicArray<T>` semantics suite** — Push-grow-realloc, index bounds, nested drop, move-in/out; `run_captured` value assertions beyond smoke build (2026-06-15). **Ref:** ROADMAP M6, PHX-061. **Owner:** std + tests. **Gate:** Std v0.

- [x] **`UniquePtr<T, A>` end-to-end** — Allocate, move, drop dealloc, use-after-move diagnostic; aligns with [allocator.md](design/features/allocator.md). **Owner:** std + compiler. **Gate:** Std v0.

- [x] **Generic `#derive` for std types** — `DynamicArray<T>` and similar need `PartialEq`/`Debug` without hand-written impls. **Ref:** PHX-030, ROADMAP Deferred. **Owner:** compiler (derive). **Gate:** Std v0.

- [x] **Owned `String` over `DynamicArray<u8>`** — Std v0 module per completion roadmap order #4; no primitive owned string in core. **Ref:** `type-system.md`, `mvp.md`. **Owner:** std. **Gate:** Std v0.

- [x] **`text::fmt` minimal formatting** — Depends on `String` + `Display`; post-collections. **Owner:** std. **Gate:** Std v0.

### VM / runtime hardening for std workloads

- [x] **DynamicArray misuse fixtures (UAF, double-free)** — Exercise PHX-054 ledger checks under std collection patterns (2026-06-15). **Ref:** ROADMAP M6. **Owner:** tests + VM. **Gate:** Std v0.

- [x] **Heap cap configuration follow-up** — Default 64 MiB cap ships (PHX-053); `phoenix.toml` `[vm] heap_cap` and `phx run --heap-cap` (suffix strings like `64mb`, `1gb`). **Ref:** ROADMAP M6, `vm-linear.md`. **Owner:** VM + CLI. **Gate:** Std v0 (operational).

- [x] **Mono guardrail for pathological generic nesting** — Max **64** generic nesting layers; `E2046 GenericNestingTooDeep` at instantiation sites and monomorphization (ROADMAP M4 follow-up). **Owner:** compiler (mono). **Gate:** Std v0.

---

## P3 — Post-beta / post-MVP (do not start until P0–P1 pass)

Explicitly out of [mvp.md](design/mvp.md) scope. Track for planning only.

- [ ] **Bytecode source maps / debugger (PHX0 section 5)** — **Partial (2026-06-21):** PC span pipeline and multi-module stress fixtures on trunk; remaining work is function-name symbols and full debugger. **Ref:** PHX-070. **Owner:** compiler + VM + CLI. **Gate:** Post-beta.
  - [x] `PcSpanTable` codegen + CLI `format_vm_error` (PR #12–#13)
  - [x] Link-time merge + `heap_uaf` integration span test
  - [x] Release profile strips section 5; verifier accepts stripped modules (PR #101, #121, #123)
  - [x] Nested / indirect callee PC span map (PR #103)
  - [x] Hostile section 5 verifier mutation tests (PR #102)
  - [x] Release trap golden without source spans (PR #130)
  - [x] Multi-module linked callee span integration test (PR #137)
  - [ ] Function-name symbol stub; full debugger

- [ ] **Full borrow checker** — **Partial (2026-06-21):** phase-0 exclusivity slices on trunk; lifetimes remain. **Ref:** `ownership.md`, ROADMAP Deferred. **Owner:** compiler. **Gate:** Post-MVP.
  - [x] Overlapping `&mut T` rejection (PR #106)
  - [x] Shared `&T` + `&T`/`&mut T` conflict (PR #111, #115)
  - [x] Golden diagnostics E2047 / E2048 (PR #110, #119)
  - [x] If/match arm borrow join (PR #128)
  - [x] Loop body / back-edge borrow join (PR #131)
  - [x] If-arm and loop overlapping-mut goldens (PR #134)
  - [x] `phx explain` for E2047 / E2048 (PR #135)
  - [ ] Lifetime syntax (post-MVP)

- [ ] **M:N scheduler + schedulable I/O** — **Partial (2026-06-21):** in-tree harness, design contract, I/O wait stub, and M:N worker pool on trunk; no Phoenix syntax or std I/O yet. **Ref:** `mvp.md` shipping order, [`runtime-transparency.md` — Schedulable I/O contract](design/features/runtime-transparency.md#schedulable-io-contract). **Owner:** VM + std. **Gate:** Post-MVP.
  - [x] Scheduler types + single-thread park/resume harness (PR #104)
  - [x] Schedulable I/O contract documented (PR #113)
  - [x] I/O wait registry stub + `AwaitIo` wakeup (PR #116)
  - [x] `AWAIT_IO` opcode contract in `vm-linear.md` (PR #127)
  - [x] M:N OS-thread `WorkerPool` harness (PR #136)
  - [ ] Wire I/O registry through worker pool; std I/O integration

- [ ] **Actors, mailboxes, supervision** — `@spawn` / `@send` execution semantics. **Ref:** `concurrency.md`, `messages.md`. **Owner:** VM + compiler. **Gate:** Post-MVP.

- [ ] **Std I/O and networking** — Blocked on scheduler. Pre-scheduler stdout bridge shipped via [`io-bridge.md`](design/features/io-bridge.md). **Owner:** std + VM. **Gate:** Post-MVP.

- [ ] **JIT / hot reload** — Post-MVP per `mvp.md`. **Owner:** VM. **Gate:** Post-MVP.

- [ ] **IDE / LSP partial-AST mode** — Builds on PHX-005 partial parse recovery (already shipped). **Ref:** ROADMAP Deferred. **Owner:** compiler + tooling. **Gate:** Post-beta.

- [ ] **Stable FFI symbol identity (Phase B)** — Replace registration-order foreign stubs. **Ref:** `ffi.md`, ROADMAP Deferred. **Owner:** VM + linker. **Gate:** Post-beta.

- [x] **`--deny` / lint configuration** — Fail compiles on warnings via CLI `--deny` and `phoenix.toml` `[lint] deny`. **Owner:** CLI. **Gate:** Post-beta.

---

## Explicitly not todo (already resolved in architecture review)

Do **not** re-open unless a regression appears:

- Ownership fork/join across branches and loops (PHX-023, PHX-024)
- Verifier `JumpIfFalse` / stack-flow soundness (PHX-045)
- Silent codegen fallbacks → diagnostics (PHX-016, PHX-034, PHX-037, PHX-041)
- Module-local link rebasing (PHX-033, PHX-036)
- VM width-faithful integers, heap cap, UAF detection, `VerifiedModule` (PHX-051–PHX-055)
- Heap slices V0-062, trait defaults V0-063, Result match V0-064, dealloc V0-065 (implemented; docs may lag — see P1 PHX-032)

---

## Suggested work order

1. **P0 compiler regressions** — cursor drift → allocator/mono → stack depth in `examples/errors` → seven `cli_e2e` tests green.
2. **P1 doc-truth (PHX-032)** — update completion roadmap + implementation checklist; fix `language-v0.md` link target.
3. **P1 error-handling polish** — must-use errors + golden diagnostics.
4. **Announce Language v0** when P0 + P1 verification items pass.
5. **P2 Std v0** — DynamicArray/UniquePtr suites → generic derive → String → fmt.

---

## Quick counts


| Tier                 | Items  | Primary gate |
| -------------------- | ------ | ------------ |
| P0 — Blockers        | 9      | Language v0  |
| P1 — Beta / announce | 10     | Language v0  |
| P2 — Std v0          | 8      | Std v0       |
| P3 — Post-MVP        | 9      | Post-beta    |
| **Total actionable** | **36** |              |


