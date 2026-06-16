# Phoenix Roadmap — Path to Usable Beta

**Project:** Phoenix language compiler and VM (`phx`)
**Generated:** 2026-06-11, from a full-workspace architecture review (findings `PHX-001`–`PHX-063`)

Phoenix is substantially further along than "MVP": the full pipeline (lex → parse → resolve → typeck → lower → PHX0 → verify → VM) runs end to end; modules, generics with monomorphization, traits with default-body inheritance, `#derive`, heap alloc/free intrinsics, and std-authored `Option`/`Result`/`?`/`Drop`/`DynamicArray` all exist. The workspace has a clean, acyclic crate graph, zero external dependencies, essentially zero `unsafe`, no TODO debt, and a verifier with real adversarial tests. The review found three Critical defects — ownership analysis ignores control flow (branch poisoning and loop back-edge misses), and the bytecode verifier's stack-flow CFG does not model `JumpIfFalse` — plus a recurring family of silent fallbacks (`unwrap_or(0)`, fallback-to-`Unit`) that convert internal compiler errors into miscompiles instead of diagnostics. Multi-module linking is correct today only by accident (whole-program tables mask unpatched operands), and the VM's wide-integer/NaN/shift semantics diverge from spec. These, not missing features, are what stand between the current tree and a credible std-authoring platform.

---

## Beta Definition

Phoenix is at **usable beta** — ready for std to be built in earnest in Phoenix source — when all of the following are true:

1. **No panics, no silent wrong answers.** The compiler never panics on any input, and no internal inconsistency can silently produce wrong bytecode: every fallback path (`unwrap_or(0)`, fallback-to-`Unit`, debug-assert-and-continue) is replaced by a diagnostic or hard error.
2. **Move semantics are trustworthy.** Use-after-move is detected across `if`/`match` arms and loop back-edges with no false positives from sibling branches; the partial-move rule is decided, documented in `ownership.md`, and tested.
3. **The verifier is sound for hostile input.** Every documented PHX0 invariant (jump targets, operand arity, stack-depth joins on *all* branch opcodes, section bounds/overlap/duplicates, version, layouts, allocation bounds at decode) is enforced and mutation-tested.
4. **Numeric execution matches declared widths.** s8–s128, u8–u128, f32/f64 behave per spec, including u128 arithmetic/comparison, shift masking, and documented NaN semantics.
5. **Separate compilation is real, not accidental.** Module-local artifacts link with fully rebased operands; malformed `.pxi` files are rejected; a two-module cross-call fixture (including `GetField`/`MatchTag` across the boundary) passes.
6. **Std substrate is closed.** Heap slices (V0-062) land; std types are located only via the path-scoped kernel (no name-based fallbacks); generic inference recurses structurally so std generics are ergonomic.
7. **The gate is honest.** `just pre-commit` exercises the test suites that protect the above; fixture-gated tests cannot pass vacuously; design docs match the implementation.

Milestone 8's exit criterion is the beta gate; Milestones 0–7 are sequenced so each one's exit criterion is a precondition for the next.

---

## Milestone 0 — Foundation Integrity

**Exit criterion:** All Critical findings (PHX-023, PHX-024, PHX-045) are resolved with regression tests. No silent-fallback path remains that can emit wrong bytecode or wrong types without a diagnostic. The full pipeline runs on all existing fixtures with zero crashes, and `just pre-commit` runs the suites that would catch a regression in any of the above.

| Item | Finding | Work |
|---|---|---|
| Fork/join ownership state across `if`/`match` arms | **PHX-023 (Critical)** | Snapshot per arm, join at merge; kills false `UseAfterMove` from sibling branches |
| Loop back-edge move detection | **PHX-024 (Critical)** | **Decided:** move-in-loop of an outer binding is always an error (Resolved Design Decisions #1); implement + spec in `ownership.md`, test `loop { use(x); consume(x); }` |
| Model `JumpIfFalse` (and `JumpIfTrue` fall-through) in stack-flow CFG; remove `_` wildcard on `Opcode` | **PHX-045 (Critical)** | Done — `conditional_branch_successors` + mutation test |
| Codegen map misses must error, never encode `0` | PHX-034 | `CodegenError` on missing callee/jump/drop-fn targets |
| Lowering `ExprId` cursor drift must error, never fall back to `()` | PHX-037 | `LowerError` on miss; document ordering invariant on `TypedProgram::expr_types` |
| `TypeInterner::get` OOB must not return `Ty::Unit` | PHX-016 | Poison `Ty::Error` or ICE diagnostic |
| `fn_def_for` failure must not fall back to `DefId(0)` | PHX-017 | Skip + internal-error diagnostic |
| Remove the production `expect` in derive expansion | PHX-018 | Propagate as `DeriveError` |
| Silent-fallback cleanup in codegen/lower (`pool_index_for_literal`, `LowerCtx::emit`, `u32::MAX` saturation) | PHX-041, PHX-030, PHX-021 | Convert to errors |
| `[INFRA]` Align `just pre-commit` with what protects the pipeline | PHX-060 | **Done:** `pre-commit` runs `cargo test --workspace` plus serial `test-lang` (matches CI `rust` + `cli` jobs) |
| `[INFRA]` Fixture-gated tests must fail loudly when fixtures are missing | PHX-062 | **Done:** `require_cli_project` / `require_fixture_file`; CI `check-fixture-gates.sh`; compiler ↔ phx-test dev-dep cycle broken |

---

## Milestone 1 — Language Core Hardening

**Exit criterion:** The typechecker correctly rejects all invalid programs in the test suite, including the new ownership control-flow cases; move semantics decisions are documented in `ownership.md`; all Major frontend findings (syntax, resolver, typeck) are resolved; no `_` wildcards remain on AST enums in compiler passes.

| Item | Finding | Work |
|---|---|---|
| Reject partial moves (moving a non-Copyable field out of a struct) | PHX-025 | **Deferred:** v0 tracks whole bindings only — see [ownership.md](design/features/ownership.md#mvp-no-partial-moves); field-extraction rejection is post-v0 |
| Remove name-based std-type fallbacks (`find_enum_def_by_name("Option")`, `"Iterator"`/`"From"`/`"Drop"` scans) | PHX-026 | All std-item lookup goes through path-scoped `StdKernel`/`StdTraitKernel`; missing std ⇒ clear error |
| Generic bounds arity mismatch must error, not silently pass | PHX-027 | Push diagnostic, return failure, negative test |
| Remove module-level `allow(unreachable_patterns)` / `_` arms on AST enums in typeck and lower | PHX-019 | Explicit arms (an `unsupported` diagnostic arm is acceptable) |
| Preserve statement spans in blocks | PHX-004 | `BlockItem::Stmt` carries `StmtNode` |
| Return partial ASTs from parser recovery | PHX-005 | `(SourceFile, ParseBag)`-style API; driver decides whether to continue |
| Eliminate the parser's raw-pointer recovery bag (`unsafe`) | PHX-001 | Own the bag in `Parser` |
| `Interner::resolve` must not mask invalid symbols | PHX-003 | `Option<&str>`; restrict `Symbol::from_raw` |
| Unclosed `{` emits `UnexpectedEof` | PHX-006 | Parser recovery diagnostic |
| `range_pattern`: parse-and-defer | PHX-007 | **Decided:** AST node + `UnsupportedFeature` in typeck (Resolved Design Decisions #9); add row to `grammar-deferred.md` |
| `[INFRA]` Negative-test expansion for typeck: branch/loop moves, cast edges, bounds arity, inference holes | PHX-061 | Done — branch/loop move tests in `typeck.rs`; golden cast fixture added |
| `[FEATURE]` Cast-rule consolidation: move tuple-struct and `[u8; N] as str` special casts into `ops.rs` with documented rules | — | Single cast authority; no implicit widening anywhere (verified none exists today) |

---

## Milestone 2 — Bytecode and VM Hardening

**Exit criterion:** The verifier enforces every documented PHX0 invariant and the mutation suite covers each one. The VM executes all numeric types per spec at declared widths. Drop glue and all emitted code maintain balanced stack discipline. All Major findings in `phx-bytecode` and `phx-vm` are resolved.

| Item | Finding | Work |
|---|---|---|
| Validate operand arity for all opcodes (jumps, calls) | PHX-046 | Done — jump group arity guard + mutation test |
| Verify header version; reject overlapping/duplicate sections | PHX-047 | Done — `validate_section_table`; decode rejects bad layouts |
| Clamp untrusted `with_capacity` counts at decode | PHX-048 | Done — `checked_entry_count` before alloc/loop |
| Fix `DropLocal` stack leak; verifier enforces canonical stack depth at `Return` | PHX-035 | **Decided:** depth at `RETURN` must equal return arity (Resolved Design Decisions #3); spec in `vm-linear.md`, update POP note |
| Split signed/unsigned arithmetic; fix u128 div/mod/compare; float `Mod` = IEEE truncated remainder; cut `Pow` from v0 | PHX-051 | **Decided:** Resolved Design Decisions #4; width-accurate execution per `wide-integers.md` |
| Shift masking and NaN comparison semantics | PHX-052 | **Decided:** mask shift amounts to width; IEEE 754 NaN (`NaN != NaN`, ordered comparisons false) (Resolved Design Decisions #4); spec in `wide-integers.md`/`type-system.md`, then implement |
| Heap allocation cap; remove pointer sentinel | PHX-053 | `VmError::OutOfMemory` instead of abort |
| Use-after-free detection against the live ledger, on by default | PHX-054 | **Decided:** always-checked in v0 behind a single gateable function (Resolved Design Decisions #5); protects the std bootstrap (M6) |
| Verifier fidelity cluster: join-mismatch error kind, `Trap` operand contract, `MakeStr` tag, layouts required at minor ≥ 1 | PHX-049 | done |
| Carry `(function_id, pc)` on `VmError` | PHX-056 | Done — coarse runtime attribution; full source maps → PHX-070 |
| `run_verified()` / `VerifiedModule` so the verify-before-run invariant is type-enforced; PC-past-end is an error | PHX-055 | **Decided:** VM assumes verified input; `VerifiedModule` constructible only via the verifier (Resolved Design Decisions #7) |
| `[INFRA]` Mutation-test expansion: `JumpIfFalse` underflow, join mismatch, section overlap, oversized alloc, non-boundary jump | PHX-061 | Done — join-depth + mid-instruction jump mutations; `heap_alloc_oom` e2e OOM |

---

## Milestone 3 — Module System and Separate Compilation Hardening

**Exit criterion:** Multi-file programs compile to **module-local** artifacts that link with fully rebased operands; a two-module fixture that cross-calls and executes `GetField`/`MatchTag` across the boundary passes end to end; malformed `.pxi` input is rejected with a clear error. (`#import`, `pub` enforcement, block-scoped imports, and child-module resolution already ship and are regression-locked by existing fixtures.)

| Item | Finding | Work |
|---|---|---|
| Patch all type-table/const-pool operands in the linker (`GetField`, `SetField`, `MatchTag`, `MakeStr`, …) | PHX-033 | Exhaustive opcode-driven patch table, no `_` arms |
| Module-local lowering: stop embedding the whole-program constant pool per module | PHX-036 | Unmasks PHX-033; do both in one change with link tests |
| Reject malformed `.pxi` exports instead of partial-parsing | PHX-040 | `Result`-returning PXI body parse |
| Single-module codegen uses `ENTRY_NONE` when `main` is absent | PHX-043 | Entry-semantics consistency for `lib` artifacts |
| `[INFRA]` Two-module link-and-execute integration fixtures (cross-module enum match, field access, drop glue) | PHX-061 | Done — `tests/integration/tests/link_rebase.rs` |
| `[INFRA]` PXI round-trip property tests for nested generic types | — | Hand-rolled JSON parser hardening |

---

## Milestone 4 — Traits and Generics

**Exit criterion:** Trait definitions, impls, default bodies, and static dispatch work end to end with a single source of method-resolution truth shared by typeck and lowering. Structural inference binds type parameters in nested positions. `#derive` behavior matches the design docs. Monomorphization is exercised against std generics in CI.

| Item | Finding | Work |
|---|---|---|
| Structural recursion in `InferenceCtx::unify` (bind `T` inside `Named`/tuple/ref args) | PHX-028 | Required for ergonomic std generics |
| Single source of method resolution: typeck records callee `DefId` + mono args; lowering consumes it | PHX-038 | Delete `resolve_method_callee_for_ty` duplication |
| Align `#derive` with docs (`Debug` documented or removed); track generic derive as a feature | PHX-029 | Doc/code sync; generic derive needed before std types can derive |
| `[INFRA]` Update stale design docs: V0-063 trait defaults and V0-064 Result match are implemented | PHX-032 | Docs are the source of truth — keep them true |
| `[FEATURE]` Mono guardrail: recursion-depth limit with diagnostic for pathological generic nesting | — | **Shipped:** max depth 64, `E2046` at instantiation + mono pass |
| `[INFRA]` CI exercise: monomorphization of `Option<T>`/`Result<T,E>`/`DynamicArray<T>` through trait calls | PHX-061 | — |

---

## Milestone 5 — Std Bootstrap Substrate

**Exit criterion:** The remaining Phase-7 partial — heap slices (V0-062) — is complete and tested through lower/codegen/verify/VM (the in-flight `IndexStore`/slice work in the current working tree, finished). `Option`/`Result` remain std enums reachable **only** via the path-scoped kernel (no name fallbacks — closed in M1). `phoenix.toml`, `lib` vs `bin` artifacts, std layout, and `From`/`Into`/`TryFrom` already ship; this milestone locks them with the hardened separate compilation from M3.

| Item | Finding | Work |
|---|---|---|
| `[FEATURE]` Finish heap slices (V0-062): slices over heap memory, `IndexStore`, slice assignment — complete and land the in-flight working-tree changes | — | The last documented Phase-7 partial |
| `[INFRA]` Lower/codegen/verify tests for `IndexStore` and nested index chains | PHX-061 | Done — `heap_slice_store` / `heap_slice_nested_index` fixtures |
| Drop glue verified end to end on std types after PHX-035 | PHX-035 | Done — `dynamic_array_drop_smoke_verifies_balanced_main_return` |
| `[INFRA]` Delete dead `interface_loader.rs` scaffolding (live path: `exports_for_dependency`) | PHX-044 | Done — removed unused module |
| `[FEATURE]` Document `str`/`Ty::Str` as the sanctioned core text view in `type-system.md` (already in `mvp.md`) | — | **Decided:** `Ty::Str` stays compiler-known through v0; add normative Copyable-view statement + phased migration note (Resolved Design Decisions #6) |

---

## Milestone 6 — Heap Alloc and DynamicArray

**Exit criterion:** `std::core::alloc` (`alloc_bytes`/`dealloc_bytes`, both `unsafe`) is bounded and misuse-detecting: allocation failure is a clean `VmError`, double-free errors (already shipped), and use-after-free is detected in checked mode. `DynamicArray<T>` (std-authored, already shipped) passes push/index/drop semantics tests on top of the hardened VM. The `DynamicArray` naming is already consistent — no `Vec` leaks into the language surface (verified).

| Item | Finding | Work |
|---|---|---|
| Heap cap + OOM error (from M2, validated under std workloads) | PHX-053 | Stress fixture: growth loop hits cap cleanly; **follow-up:** raise/revisit default cap and expose project/CLI configuration (see `vm-linear.md` VM resource limits) |
| UAF detection exercised by std tests | PHX-054 | `DynamicArray` misuse fixtures (dangling index after free) |
| `[INFRA]` `DynamicArray` semantics suite: push-grow-realloc, index bounds, nested drop, move-into/out | PHX-061 | Std-level guarantee tests on `run_captured` |
| `[FEATURE]` `slice_from_raw_parts` + heap-slice interop validated against V0-062 | — | Bridges M5 slices and std collections |

---

## Milestone 7 — Error Handling and `?` Operator

**Exit criterion:** `Result<T,E>`/`Option<T>` are fully usable (largely true today); `?` propagates with `From<E>` conversion and its failure modes are diagnostics, not silent codegen; discarding a `Result`/`Option` without handling it is a type error.

| Item | Finding | Work |
|---|---|---|
| `?`-with-`From` lowering: replace `debug_assert` + silent return with `LowerError` | PHX-042 | Release-mode safety for the desugar |
| `[FEATURE]` Must-use enforcement for `Result`/`Option`: type-aware discard error | PHX-029-adjacent, requires PHX-028's typed-lint plumbing | Pass `TypedProgram` into lint (PHX-029); per `error-handling.md` |
| Lint parity across compiling CLI commands | PHX-059 | **Done:** `check`, `compile`, `run`, and `build` emit lint warnings when type-check runs; incremental cache / `--no-build` may skip lints. `--deny`/lint-config deferred post-beta |
| `[INFRA]` Golden diagnostics for `?` mismatch, missing `From` impl, discarded `Result` | PHX-061 | — |

---

## Milestone 8 — Contributor Readiness (Beta Gate)

**Exit criterion:** All remaining Minor findings and Suggestions are resolved or triaged with a documented decision. `just pre-commit` passes and is trusted. No dead scaffolding in load-bearing paths. A new contributor can orient in each crate from inline docs alone. **After this milestone, std is built in earnest.**

| Item | Finding | Work |
|---|---|---|
| Split `typeck/check.rs` (6,030 lines) into `check/{decl,impl,expr,stmt,pattern,intrinsic}.rs` | PHX-031 | After M0/M1 fixes land, to avoid churn |
| Split backend God modules (`lower/expr.rs`, `build/driver.rs`) and `phx-vm/interpreter/` | PHX-044, PHX-057 | PHX-057 done; PHX-044 triage with M5–M7 |
| IR validator (terminator-last, target-in-range, optional depth simulation) in debug builds | PHX-039 | Catches lowering bugs before they become verify mysteries |
| Span hygiene: spans (or side table) on `IrInst`; eliminate `Span::new(0,0)` synthesis | PHX-063 | done — `SpannedInst` wrapper, lowering spans, CI guard |
| Parse-error formatting moves into `phx-diagnostics`; complete `phx explain` coverage (E3002–E3005, E2033) | PHX-012, PHX-013 | — |
| Diagnostic enum policy: write the `#[non_exhaustive]` exemption into the Rust rules; `macro_rules!` table-driven `TypeCheckError` metadata to stop 4-way match drift | PHX-014, PHX-015 | **Decided:** per-pass error enums stay exhaustively matchable; table-driven definition approved (Resolved Design Decisions #8) |
| Hot-path cleanups: peek-by-reference, literal-payload side table, `strip_underscores`, interner pollution | PHX-002, PHX-009 | Profile-first per project rules |
| Clone audit: `ResolvedProgram` clone, per-mono `TypeInterner` clones, `type_defs.clone()` | PHX-020 | Profile-first |
| ICE handler debug escape hatch (`PHX_ICE_DEBUG`) | PHX-058 | — |
| API surface triage: `ResolvedProgram`/IR/mono re-exports vs `facade` | PHX-022 | Documented decision |
| Test-suite hygiene: stale keyword test, missing `mod`/`reexport`/`extern` parser tests, `extern` `pub`, golden-set expansion, dev-dep cycle decision | PHX-010, PHX-011, PHX-061, PHX-062 | PHX-061 golden set expanded (12 fixtures); PHX-062 fixture gates + cycle break done |
| `verify` validates the in-memory module directly (no re-encode); fallible `Instruction::encode` | PHX-050 | done |
| `load_project_binary` optional verify-on-load | PHX-055-adjacent | Defense in depth; documented decision |
| Doc-truth pass: `ownership.md` (loop/partial-move rules from M0/M1), `wide-integers.md`/`type-system.md` (shift/NaN from M2), `vm-linear.md` (POP/`Trap` contract), V0 checklists | PHX-032 | Docs match code everywhere |

---

## Deferred Work (post-beta)

| Deferred item | Most directly enabled by |
|---|---|
| **M:N scheduler / schedulable I/O** — all code under VM scheduler management, cooperative parking | M2's VM hardening; PHX-057's `ExecutionContext` boundary is the prerequisite refactor |
| **Full borrow checker** (`&T`/`&mut T` exclusivity, lifetimes) | M0/M1's fork-join ownership model — built CFG-shaped so the borrow checker can grow from it instead of replacing a linear tracker |
| **Actors, mailboxes, supervision** (`@spawn`/`@send`/`@receive`) | Scheduler work above; format versioning in PHX0 header already reserves room |
| **Std I/O** (`File.read`, networking) | Explicitly gated on scheduler + schedulable I/O per `mvp.md` shipping order |
| **JIT / hot reload** | M2 verifier completeness (a sound verifier is the JIT's trust anchor) |
| **Bytecode source maps / debugger** (section 5 symbols) — **PHX-070** | M8's IR span work (PHX-063) + M2's `(function_id, pc)` errors (PHX-056, done) |
| **Stable FFI symbol identity** (replace registration-order foreign ids) | M3's link hardening; `ffi.md` Phase B |
| **IDE/LSP mode** | M1's partial-AST recovery (PHX-005) and statement spans (PHX-004) |
| **Generic `#derive`** | M4 derive alignment (PHX-029); needed before std types derive `PartialEq`/`Debug` |

---

## Resolved Design Decisions

These were the review's open questions; each is now decided (2026-06-12). The guiding rule throughout: the conservative option is strictly forward-compatible — it can be loosened later without breaking programs — while the permissive option locks us in. Each decision must land in its design doc as part of the milestone that implements it.

1. **Loop move semantics (PHX-024) — always an error (conservative).** A non-Copyable binding declared outside a loop that is moved inside the loop body is a `UseAfterMove` error at the back-edge, even if reassigned before it. Definite-reassignment analysis belongs to the post-MVP path-sensitive ownership work already scoped in `ownership.md` ("MVP limitation: flow-insensitive"). Document the workaround (restructure or shadow inside the loop). Spec in `ownership.md`; implemented in **M0**.
2. **Partial moves (PHX-025) — deferred.** v0 tracks **whole bindings** only; field reads and pattern binds do not partially invalidate the parent ([ownership.md](design/features/ownership.md#mvp-no-partial-moves)). Rejecting non-Copyable field extraction (`var x = s.field`) is post-v0 — neither whole-value invalidation nor per-field tracking is implemented yet.
3. **Stack-at-return invariant (PHX-035) — canonical depth required.** At `RETURN`, operand-stack depth must be exactly the return arity (0 or 1). Phoenix codegen is the only producer and must emit balanced stacks; the verifier enforces it (the Wasm model). This is what catches leaks like the `DropLocal` bug. Spec in `vm-linear.md`; implemented in **M2**.
4. **Numeric edge semantics (PHX-051/052) — Rust/Wasm defaults, written into the spec.** Shift amounts are **masked to the operand width** (`x << (n & (W-1))`, per Wasm / Rust `wrapping_shl`); a trapping checked mode is post-beta. NaN follows **IEEE 754**: `NaN != NaN`, all ordered comparisons involving NaN are false; no language-level total order (std trait concern later). Float `%` is IEEE truncated remainder (Rust/C `fmod`). `**`/`Pow` is **cut from v0** rather than specified. Whatever the VM does must be in the docs — portable bytecode means every future backend (JIT) reproduces these semantics bit-for-bit. Spec in `wide-integers.md` + `type-system.md`; implemented in **M2**.
5. **Use-after-free policy (PHX-054) — ledger-checked always in v0.** Heap loads/stores validate against the live-allocation ledger by default; the check sits behind a single function so a future `--unchecked` mode can gate it once borrow checking matures. The MVP VM is an interpreter where the lookup is noise next to dispatch cost, and bootstrap debuggability is worth far more than interpreter speed now. Implemented in **M2**, exercised by std misuse fixtures in **M6**.
6. **`str` story — `Ty::Str` stays compiler-known through v0.** `type-system.md` gets a normative statement: `str` is a Copyable `(ptr, len)` UTF-8 view over rodata (or heap via V0-062 slices). Migration to a std-defined fat pointer is deferred with a one-line "phased" note mirroring the Copyable bootstrap exception — no migration design now. Doc work lands in **M5**.
7. **Library trust boundary (PHX-055) — VM assumes verified input, type-enforced.** `phx_vm::run` takes a `VerifiedModule` constructible only via the verifier (the Wasm model: validate once at load, execute trusting static invariants). The interpreter keeps only the dynamic checks the verifier cannot prove (bounds, allocation ledger) and sheds per-instruction re-checking long-term. Implemented in **M2**; `load_project_binary` verify-on-load stays as M8 defense-in-depth.
8. **Diagnostic enum policy (PHX-014/015) — exemption confirmed, table-driven definition approved.** Per-pass error enums are internal, consumed within the workspace where exhaustive matching is a feature; `#[non_exhaustive]` stays reserved for Phoenix language-construct enums (AST, tokens, opcodes). A `macro_rules!`-driven single definition per error is acceptable under the std-only constraint (`macro_rules!` is not a proc macro) — keep the macro simple and messages greppable. Write the exemption into the Rust rules doc as part of **M8**.
9. **`range_pattern` (PHX-007) — parse-and-defer.** AST node + `UnsupportedFeature` in typeck, consistent with every other row in `grammar-deferred.md` (range *expressions* are already "parse; reject at typeck"). Rejecting at parse would make the parser disagree with `grammar.ebnf`. Implemented in **M1**.
10. **Lint surface (PHX-059) — lint parity on all compiling CLI commands in v0.** `check`, `compile`, `run`, and `build` run the lint pass and print warnings (`#[deprecated]`, `#[must_use]` on attributed items) whenever type-check runs. Discarded std `Result`/`Option` fail type-check (E2041/E2042), not lint. Warnings never fail the command alone; invalid `#[allow(...)]` names remain errors. Incremental cache hits and `phx run --no-build` skip type-check and therefore skip lints — use `phx check` or `--build` for a full re-check. `--deny` / lint-config deferred post-beta. Implemented in **M7**.
