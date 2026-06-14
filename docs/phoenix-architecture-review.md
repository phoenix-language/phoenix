# Phoenix Architecture Review — Phase 1

**Scope:** full workspace at working-tree state (including ~780 lines of in-flight, uncommitted heap-slice work in `typeck`/`lower`/`codegen`/`interpreter`).
**Headline:** the pipeline is in much better shape than a typical pre-v0 compiler — clean crate DAG, zero external deps, near-zero `unsafe`, defensively coded VM, no TODO debt, and real adversarial verifier tests. The serious problems are concentrated in three places: **ownership analysis ignores control flow**, **the verifier's stack-flow CFG has a soundness hole (`JumpIfFalse`)**, and **a family of silent fallbacks (`unwrap_or(0)`, fallback-to-`Unit`) that convert internal errors into miscompiles instead of diagnostics**.

---

## phx-syntax

A clean front end: `lexer`/`token` → `parser/` (split by construct) → plain-data AST under `ast/`, `Symbol(u32)` interning, `SourceFile` bundling. Depends only on `phx-diagnostics`. Parser implements multi-error recovery via `ParseBag` with sync points.

### Rust Expert Findings

---

**[PHX-001] Severity: Major**
**Location:** `phx-syntax/src/parser/mod.rs:77–81` (`recovery_bag`)
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** Error recovery holds a raw `*mut ParseBag` and dereferences it in the only `unsafe` block in the production compiler.
**Detail:** Project policy permits `unsafe` only in the VM and an arena allocator; this block exists to dodge a borrow conflict, and although it carries a `// SAFETY:` comment, the invariant ("parser never outlives the bag, recovery never re-entered") is enforced by nothing. As the parser grows recovery paths, this is exactly the pattern that silently becomes UB.
**Recommendation:** Store the `ParseBag` by value inside `Parser` (or pass `&mut ParseBag` down the parse methods) and delete the raw pointer. This is mechanical, no design discussion needed.

---

**[PHX-002] Severity: Major**
**Location:** `phx-syntax/src/parser/mod.rs:206–213` (`peek_kind` / `peek_at`)
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** Every parser peek clones the full `TokenKind`, including owned `String`/`Vec<u8>` literal payloads.
**Detail:** Expression parsing peeks constantly; each peek over a string/byte literal heap-allocates. This is the parser's hottest loop and the cost is purely accidental.
**Recommendation:** Return `Option<&TokenKind>` from peeks, or move literal payloads into a side table so `TokenKind` is `Copy`.

---

**[PHX-003] Severity: Major**
**Location:** `phx-syntax/src/intern.rs:108–112` (`Interner::resolve`)
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** An invalid `Symbol` resolves to the placeholder string `"<invalid-symbol>"` instead of failing.
**Detail:** Combined with public `Symbol::from_raw` (`intern.rs:33–38`), a corrupted symbol silently flows through name comparisons (e.g. `attr_collect` matching `"derive"`), masking real bugs as wrong-name behavior. Silent placeholder values violate the "internal invariant violations must be loud" principle.
**Recommendation:** Return `Option<&str>`; gate `Symbol::from_raw` behind `#[doc(hidden)]`/test-only.

### Language Designer Findings

---

**[PHX-004] Severity: Major**
**Location:** `phx-syntax/src/parser/stmt.rs:51,58` (`parse_block_item`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Block statements are stored as `BlockItem::Stmt(stmt.inner)`, stripping the `StmtNode` span and node id.
**Detail:** The project rule is "never discard span information." Statement-level spans are exactly what downstream diagnostics (drop planning, move sites, lints) want to point at; imports and expressions keep their nodes but statements do not.
**Recommendation:** Make `BlockItem::Stmt` carry `StmtNode` and update consumers.

---

**[PHX-005] Severity: Major**
**Location:** `phx-syntax/src/parser/mod.rs:541–543` (`parse_with_interner`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Recovery builds a partial AST, but any non-empty `ParseBag` returns `Err(bag)` and discards the tree.
**Detail:** The parser pays the full cost of error recovery and then throws away the result, so resolve/typeck can never run "best effort" on a file with one syntax error. This caps the multi-error UX at parse errors only and blocks future IDE/LSP use.
**Recommendation:** Return the partial `SourceFile` alongside the bag (`(SourceFile, ParseBag)` or a `ParseOutcome` struct); let the driver decide whether to continue.

---

**[PHX-006] Severity: Minor**
**Location:** `phx-syntax/src/parser/stmt.rs:19–22` (`parse_block`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** An unclosed `{` exits the block loop at EOF without recording any diagnostic.
**Detail:** The caller receives a structurally incomplete block with no `UnexpectedEof { expected: "}" }`, so the user may get downstream errors with no mention of the missing brace.
**Recommendation:** Push `UnexpectedEof` when the loop exits at EOF before `}`.

---

**[PHX-007] Severity: Minor**
**Location:** `phx-syntax/src/parser/pat.rs` vs `docs/design/grammar.ebnf:249–251`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** `range_pattern` is in the grammar but not parsed; it fails as a generic `InvalidPattern`.
**Detail:** Design policy is that deferred grammar is *parsed* and rejected in typeck with `UnsupportedFeature`, not rejected at parse with a misleading code.
**Recommendation:** Parse into a `Pattern::Range` node (deferred semantics) or emit `UnsupportedSyntax` naming the feature. Flag for review which is intended.

---

**[PHX-008] Severity: Minor**
**Location:** `phx-syntax/src/attr_collect.rs`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Name-based attribute semantics (`derive`, `deprecated`, `allow`) live in the syntax crate.
**Detail:** Resolving attribute meaning by interner string comparison is a resolution concern; keeping it in `phx-syntax` blurs the stage boundary and forces the syntax crate to know lint names.
**Recommendation:** Move semantic extraction to `phx-compiler` (resolve/lint prep); keep `phx-syntax` purely structural.

### Pragmatic Critic Findings

---

**[PHX-009] Severity: Minor**
**Location:** `phx-syntax/src/parser/decl.rs:348–350`; `phx-syntax/src/lexer.rs:903–906`
**Reviewer:** Pragmatic Critic

**Status:** - [x] Complete

**Issue:** Wasted work in hot paths — tuple-struct parsing interns `"0"`, `"1"`, … and discards the symbols; every underscore numeric literal allocates a fresh `String`.
**Detail:** The interner pollution is dead code in effect; `strip_underscores` allocates for `1_000_000` when in-place digit skipping suffices.
**Recommendation:** Delete the intern loop; parse digits skipping `_` without allocation.

---

**[PHX-010] Severity: Suggestion**
**Location:** `phx-syntax/tests/lexer.rs:772–791`; `phx-syntax/tests/parser.rs`; `phx-syntax/src/parser/decl.rs:686–700`
**Reviewer:** Pragmatic Critic

**Status:** - [x] Complete

**Issue:** Test/grammar drift — the keyword exhaustiveness test hardcodes 35 tokens and omits `mod`/`reexport`/`str`/`unsafe`/`extern`; there are no parser tests for `mod`, `reexport`, `extern`; the parser ignores optional `pub` on `extern` blocks (`grammar.ebnf:108`).
**Recommendation:** Flag for review; regenerate the keyword test from `Keyword` metadata and add the missing production tests.

---

**[PHX-011] Severity: Suggestion**
**Location:** `phx-syntax/src/ast/expr.rs:122–135`, `ast/ident.rs:55–60`, `parser/mod.rs:25–39`
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** API hygiene cluster — `IfCondition` and `PathSegment` lack `#[non_exhaustive]` (other AST enums have it); `Parser` fields are broadly `pub(crate)`.
**Recommendation:** Flag for review.

---

## phx-diagnostics

Dependency-free root crate: `Span`, per-pass error enums (`LexError`, `ParseError`, `ResolveError`, `TypeCheckError`, `LowerError`), stable `DiagnosticCode`s, bags per pass, Cargo-style rendering. There is no unified `Diagnostic` struct — a deliberate per-pass-enum design that mostly works.

---

**[PHX-012] Severity: Major**
**Location:** `phx-diagnostics/src/format.rs` (absent `format_parse_error`) vs `phx-compiler/src/compile.rs:174–196`
**Reviewer:** Pragmatic Critic

**Status:** - [x] Complete

**Issue:** Parse-error formatting lives in `phx-compiler` while lex/resolve/typeck formatting lives in `phx-diagnostics`.
**Detail:** The asymmetry means parse errors get no hint/notes layer (typeck has `type_notes.rs`), and two crates now own rendering conventions that must stay in sync.
**Recommendation:** Add `format_parse_error`/`parse_message` to `phx-diagnostics`; make `compile.rs` a thin caller.

---

**[PHX-013] Severity: Minor**
**Location:** `phx-diagnostics/src/render.rs:332–417` (`explain_code`)
**Reviewer:** Pragmatic Critic

**Status:** - [x] Complete

**Issue:** `phx explain` coverage is incomplete — `E3002`–`E3005` and `E2033` have codes but no explain entries.
**Detail:** This is the first instance of the four-parallel-match drift predicted by PHX-015; users get "no explanation available" for real codes.
**Recommendation:** Backfill entries; longer term, generate the table from one manifest per code.

---

**[PHX-014] Severity: Minor**
**Location:** `phx-diagnostics/src/parse_error.rs`, `lex_error.rs`
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** `LexError`/`ParseError` lack `#[non_exhaustive]` while the workspace convention applies it to extensible domain enums.
**Recommendation:** Add the attribute. Flag for review whether diagnostic enums are intentionally exempt.

---

**[PHX-015] Severity: Suggestion**
**Location:** `phx-diagnostics/src/type_error.rs:352–440` + `code()`/`span()`/`Display`/`type_notes`
**Reviewer:** Pragmatic Critic

**Status:** - [x] Complete

**Issue:** ~40 `TypeCheckError` variants each maintained across four parallel match sites; drift already observed (PHX-013).
**Recommendation:** Flag for review — a macro or table-driven definition (std-only, no external deps required) would collapse the four sites into one.

**Resolution:** Added `type_error_registry.rs` with a table-driven macro that generates `code()` and `span()` from one variant→code map (38 variants, E2001–E2038). Removed the duplicate `code()`/`span()`/`Display` impls from `type_error.rs`; `Display` now delegates to `format::typecheck_message`. Expanded `typecheck_message` to full variant coverage. Added `every_typecheck_code_has_explain_entry` test to guard explain drift. `type_notes` and `explain_code` remain separate (payload-specific notes and long-form text); registry is documented as the canonical code source.

---

## phx-compiler

The largest crate, and structurally sound: `compile.rs` orchestrates parse → `#cfg` strip → `#derive` expansion → resolve (side-table `ResolvedProgram`, AST untouched) → typeck (side-table `TypedProgram`: `expr_types` keyed by `ExprId`, layouts, mono maps, lowering metadata) → lower (CFG-ish IR) → codegen (PHX0) → PXI/build/link. Types never live on AST nodes — a genuinely clean staging discipline. Trait default-body inheritance and multi-payload `Result` match are implemented (the completion-roadmap doc is stale on both). `Option`/`Result` are correctly std enums located via `StdKernel` by module path — there is no `Ty::Option`/`Ty::Result`.

### Rust Expert Findings

---

**[PHX-016] Severity: Major**
**Location:** `phx-compiler/src/typeck/types.rs:129–132` (`TypeInterner::get`)
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** An out-of-bounds `TypeId` silently resolves to `&Ty::Unit`.
**Detail:** Any internal bug that fabricates or corrupts a `TypeId` is laundered into "this expression is `()`", which then type-checks against real rules and produces wrong diagnostics or wrong code. Verified: `self.types.get(...).unwrap_or(&Ty::Unit)`. This is the frontend instance of the codebase-wide silent-fallback pattern (see PHX-034, PHX-037).
**Recommendation:** Return a poison `Ty::Error` (rejected everywhere, suppresses cascading diagnostics) or `Option<&Ty>` with an ICE diagnostic; never default to `Unit`.

**Resolution:** `TypeInterner::get` returns a static poison `Ty::Error` for out-of-range indices (never `Ty::Unit`); regression tests cover empty and populated interners plus `is_error_type`.

---

**[PHX-017] Severity: Major**
**Location:** `phx-compiler/src/typeck/check.rs:2002` (`check_function`)
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** When `fn_def_for` fails, the checker proceeds with `DefId::from_raw(0)`.
**Detail:** Function layout and body results get attached to whatever definition happens to be index 0. After resolve-error recovery this is reachable, producing nonsense diagnostics or corrupt layouts rather than a clean internal error.
**Recommendation:** Skip body checking for that function and record an internal-error diagnostic; never default to def 0.

**Resolution:** `check_function` returns early when `fn_def_for` is `None`, emitting `TypeCheckError::InternalError` (E2039) at the function name span; body checking and layout emission are skipped.

---

**[PHX-018] Severity: Major**
**Location:** `phx-compiler/src/derive/mod.rs:265–269` (`intern_sym`)
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** The only `expect_used` override in production code — derive expansion panics if the intern table is full.
**Detail:** Intern-table exhaustion is driven by user input volume; the workspace deny-policy exists precisely so this surfaces as a diagnostic, not an abort (release profile is `panic = "abort"`, so this kills the process with no ICE message path).
**Recommendation:** Propagate as a `DeriveError` like every other derive failure.

**Resolution:** `AstGen::intern_sym` records `InternError::TableFull` in a short-circuit `failed` slot; `copyable_impl` / `partialeq_impl` / `debug_impl` return `DeriveError` via `finish` instead of panicking. Existing compile/loader paths already map `DeriveError` to user diagnostics.

---

**[PHX-019] Severity: Major**
**Location:** `phx-compiler/src/typeck/check.rs:3–4`; `lower/expr.rs:114, 272, 523, 1813`
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** Module-level `#![allow(unreachable_patterns)]` plus `_` wildcard arms on `#[non_exhaustive]` AST enums.
**Detail:** The project's stated invariant is "every new AST/token/opcode variant is a compile error until all match sites are updated." The module-level allow disables exactly that guarantee in the two passes most likely to miscompile a new construct silently. PHX-045 in `phx-bytecode` is a live demonstration of what `_` wildcards on opcode matches cost.
**Recommendation:** Remove the module-level allows; handle new variants explicitly (an `unsupported(expr)` helper arm that pushes a diagnostic is fine — `_` is not).

**Resolution:** Removed `#[non_exhaustive]` from phx-syntax AST/token enums; deleted module-level `unreachable_patterns` allows in typeck, lower, and resolver; replaced silent `_` catch-alls with explicit variant arms across typeck, lower, resolver, lint, derive, and cfg. New AST variants now fail compilation at every match site.

---

**[PHX-020] Severity: Minor**
**Location:** `phx-compiler/src/typeck/check.rs:5997` (`resolved.clone()`); `typeck/mono.rs:241–242`; pervasive `type_defs.clone()`
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** Wholesale clones across pass boundaries — full `ResolvedProgram` cloned at typeck finish, full `TypeInterner` recloned per monomorphization instance, `TypeDefMap` cloned repeatedly inside function checking.
**Detail:** Violates the "avoid cloning across pipeline stages" rule and will dominate compile time for multi-module std builds.
**Recommendation:** Pass `ResolvedProgram` by value into `type_check`; layer mono specializations over a shared base interner. Profile before further surgery.

**Resolution:** `type_check` now takes `ResolvedProgram` by value (eliminating the finish-time clone). Mono re-check moves the shared `TypeInterner` with `mem::take` instead of cloning per instance. Added `with_pushed_generics` to consolidate scoped `type_defs` save/push/restore; removed duplicate clones in function-body checking and trait/impl lowering. `lower_trait_bound_args` threads `&mut TypeInterner` instead of cloning. Remaining `type_defs` clones are scoped one-per-lower (borrow-checker requirement) or save/restore at impl boundaries — a generic overlay stack is deferred pending profiling.

---

**[PHX-021] Severity: Minor**
**Location:** `phx-compiler/src/resolver/mod.rs:211`, `resolver/scopes.rs:55`, `typeck/trait_defaults.rs:39`
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** `DefId` allocation saturates to `u32::MAX` on overflow instead of erroring.
**Detail:** Theoretical, but it is silent state corruption on (very large) user input — same family as PHX-016.
**Recommendation:** Emit a fatal "program too large" diagnostic.

**Resolution:** Added `DefId::try_from_index` and made resolver `alloc_def` / `define_`* fallible with a one-shot `def_table_full` flag and `ResolveError::ProgramTooLarge` (E1024). Hardened duplicate-definition reporting in `scopes.rs`. Type-check growth sites (`trait_defaults`, mono `alloc_specialized_def`) emit `TypeCheckError::ProgramTooLarge` (E2040). Multi-module merge in `resolve_loaded_program` uses checked remapping instead of saturating to `u32::MAX`.

**[PHX-022] Severity: Suggestion**
**Location:** `phx-compiler/src/resolver/mod.rs:87–113`; `typeck/mod.rs:34–39`; `lib.rs:57–86`
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** `ResolvedProgram`'s fields, mono internals (`apply_mono_worklist`), and IR types are all public crate API despite the `facade` module claiming the stable surface.
**Recommendation:** Flag for review — narrow to `pub(crate)` or move under a documented-unstable `internal` module.

**Resolution:** Added `phx_compiler::unstable` for internal graphs and pass entry points; removed IR, resolver, typeck, and unit re-exports from the crate root. Documented three API tiers in `lib.rs` and `facade.rs`. Narrowed mono worklist helpers (`apply_mono_worklist`, `collect_cross_crate_mono_reqs`) to `pub(crate)`. CI guard in `tests/ci/check-compiler-api.sh`.

---

---

**[PHX-023] Severity: Critical**
**Location:** `phx-compiler/src/typeck/check.rs:4859–4920` (`check_if`/`check_if_arm`), `check.rs:4921–4955` (`check_match`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Ownership state is threaded linearly through `if`/`else-if`/`else` and `match` arms — branches are never forked or joined.
**Detail:** Verified: arms call `check_if_arm`/`check_expr_node` sequentially on the same `OwnershipTracker` with no snapshot/restore (`ownership.rs` has only `enter_scope`/`exit_scope`, which pop bindings *defined* in the scope but persist `Moved` state set on outer bindings). Consequence: moving `x` in a `then` arm makes the `else` arm a false `UseAfterMove`, and moving in match arm 1 poisons arm 2. This mis-enforces the flagship MVP guarantee — valid programs are rejected.
**Recommendation:** Snapshot ownership state per arm and join at the merge point (Moved if moved in *any* arm for post-merge uses; per-arm state during arm checking). Add the missing tests (PHX-067).

**Resolution:** Added `OwnershipTracker::join_arms` and `check_with_ownership_fork` in typeck; `check_if`/`check_match` fork from a pre-branch snapshot per arm and join with “moved if any arm moved”. Pattern arms are scoped with `enter_scope`/`exit_scope`. Branch-move regression tests (PHX-067) live in `source/phx-compiler/tests/typeck.rs`.

---

**[PHX-024] Severity: Critical**
**Location:** `phx-compiler/src/typeck/check.rs` (`check_while`/`check_loop` paths) + `typeck/ownership.rs:17–91`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** No loop back-edge analysis — a move inside a loop body is not detected as use-after-move on the next iteration.
**Detail:** The tracker is strictly linear; `loop { use(x); consume(x); }` checks clean because the textual use precedes the move. `grep` of `tests/typeck.rs` confirms zero loop/branch move tests. This is the false-*negative* counterpart to PHX-023: invalid programs are accepted, and the resulting bytecode operates on moved-from values.
**Recommendation:** Treat any binding moved anywhere in a loop body as moved at the loop head on a second pass (or error outright on non-Copyable moves of loop-external bindings inside loop bodies, the conservative MVP rule). Needs a one-paragraph design-doc decision on which rule Phoenix wants — flag for the author.

**Resolution:** Flow-insensitive loop join (author-confirmed, matches PHX-023): `with_loop_body` forks from pre-loop state, joins with `join_arms`, runs a lightweight AST read-use scan via `newly_moved_since` for loop-carried bindings, and sets post-loop ownership to the joined state. Loop-move regression tests (PHX-068) live in `source/phx-compiler/tests/typeck.rs`.

---

**[PHX-025] Severity: Major**
**Location:** `phx-compiler/src/typeck/ownership.rs:9–15`; `check.rs:3194–3200`, `5170–5269`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** No partial-move tracking — field access and destructuring never invalidate the parent binding.
**Detail:** `BindingState` is only `Valid | Moved(Span)`. Extracting a non-Copyable field leaves the whole parent usable. `ownership.md` is the authority here; if MVP scope is whole-value moves only, the current behavior may be intentional but is undocumented and untested either way.
**Recommendation:** Requires design discussion: either document "MVP moves are whole-value; field extraction of non-Copyable values is a copy/error," or add `PartiallyMoved` states. Do not silently keep the current ambiguous behavior.

**Resolution:** Author confirmed MVP whole-value move policy: field access and pattern binds are reads; only bare-identifier transfer marks a binding moved. Documented in `ownership.md` (`MVP: no partial moves`); no `PartiallyMoved` states. Regression tests (PHX-069) in `source/phx-compiler/tests/typeck.rs`.

---

**[PHX-026] Severity: Major**
**Location:** `phx-compiler/src/typeck/check.rs:2550–2569` (`option_ty_for_item`, `find_enum_def_by_name`); also `check.rs:2312` (`"Iterator"`), `bounds.rs:292` (`"From"`), `builtins.rs:192` (`"Drop"`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Beside the path-scoped `StdKernel`, ad-hoc *name-only* lookups locate std types — `find_enum_def_by_name("Option")` matches **any** enum named `Option` in **any** module.
**Detail:** Verified: `option_ty_for_item` falls back from `std_kernel.option_enum` to a whole-`defs` scan by bare name. A user-defined `Option` enum can be silently adopted by `for-in` desugaring. The trait lookups (`Iterator`, `From`, `Drop`) have the same shape. This is exactly the "compiler special-casing by name" the design forbids — `StdKernel`'s module-path anchoring is the right pattern; the fallbacks undermine it.
**Recommendation:** Delete the name-based fallbacks; if std isn't linked, the features that need these types should error ("`for-in` requires std `Option`"), not guess. Centralize all std-item lookup in `StdKernel`/`StdTraitKernel`.

**Resolution:** Removed name-based fallbacks from `check.rs`, `bounds.rs`, and `builtins.rs`. Extended `StdTraitKernel` with path-scoped `Iterator`, `IntoIter`, and `From` from `std::core::iter` / `std::core::convert`. For-in and `?` now require linked std types via kernel only; regression tests in `typeck.rs` and `module_barrel.rs` (`std_iter`).

---

**[PHX-027] Severity: Major**
**Location:** `phx-compiler/src/typeck/bounds.rs:46–48` (`validate_instantiation_bounds`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** A generic arity mismatch between params and concrete args returns `true` (success) and skips all bound checks.
**Detail:** Inconsistent generic metadata sails through bounds validation; callers in `mono.rs:200–212` only react to `false`. Invalid instantiations reach monomorphization.
**Recommendation:** Push an arity-mismatch diagnostic and return `false`; add a negative test.

**Resolution:** `validate_instantiation_bounds` now emits `TypeCheckError::ArityMismatch` and returns `false` when `generic_params`, `param_defs`, and `concrete_args` lengths disagree; unit test in `bounds.rs`. Defense-in-depth at the bounds layer — callers already skip monomorphization on `false`.

---

**[PHX-028] Severity: Major**
**Location:** `phx-compiler/src/typeck/infer.rs:34–63` (`InferenceCtx::unify`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Unification handles var↔var and var↔concrete, but does not recurse structurally — `Option<T>` cannot unify against `Option<s32>` to bind `T`.
**Detail:** Verified: after `same_type_readonly` (exact equality) the match covers only `Ty::Var` combinations; `(Named, Named)` with differing args hits `_ => false`. Call-site inference therefore works only when a generic param appears as a bare `T`. This will bite immediately when std generics (`DynamicArray<T>`, `Result<T,E>` helpers) are used through wrapper types.
**Recommendation:** Recurse through `Named`/`Tuple`/`Ptr`/`Ref`/`Slice` argument lists in `unify`. Add tests inferring `T` from nested positions.

**Resolution:** `InferenceCtx::unify` now recurses through `Named`, `Tuple`, `Array`, `Slice`, `Ref`, `Ptr`, and `Fn` shapes via `unify_concrete`; unit tests in `infer.rs` and integration tests in `typeck.rs` cover nested struct/enum inference and ambiguous conflicts.

---

**[PHX-029] Severity: Minor**
**Location:** `phx-compiler/src/lint/mod.rs:21, 293–306`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Lint runs on `ResolvedProgram` only — it has no type information, so the documented post-MVP "discarded `Result`/`Option` is an error" rule cannot be implemented in this pass as wired.
**Detail:** `check_discard` keys off `#[must_use]` attributes only. Not an MVP bug, but a known extension point that currently requires re-plumbing.
**Recommendation:** Flag for review now; when M7 lands, pass `TypedProgram` (or `expr_types`) into lint.

**Resolution:** `lint_program` now consumes `TypedProgram`; typeck records `expr_span_types` keyed by `(module, span)`; `check_discard` uses `StdKernel` to warn on discarded std `Result`/`Option` values (lint warning, M7 error promotion deferred). Tests in `tests/lint.rs` and fixture `lint_std_result_discard`.

---

**[PHX-030] Severity: Minor**
**Location:** `phx-compiler/src/derive/mod.rs:30, 342–350`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** `#derive` supports `Debug` (undocumented in design docs) and rejects generic types entirely.
**Detail:** Doc/code divergence in both directions — docs promise `Copyable`/`PartialEq`; code also ships `Debug`; generic derive is a silent capability gap for std authoring (`DynamicArray<T>` cannot derive).
**Recommendation:** Flag for review — either document `Debug` or remove it; track generic derive as a feature item.

**Resolution:** Kept `#derive(Debug)` (already documented in `traits.md` Derive section and `compiler-directives.md`); fixed stale contradictions in `traits.md` (“future feature”) and `mvp.md` (listed `#derive` as unimplemented). Generic derive tracked explicitly in `grammar-deferred.md`; expansion emits a clear error for generic types. Tests in `derive.rs` for `Debug` and generic rejection.

### Pragmatic Critic Findings

---

**[PHX-031] Severity: Major**
**Location:** `phx-compiler/src/typeck/check/` (formerly single 6,340-line `check.rs`)
**Reviewer:** Pragmatic Critic

**Status:** - [x] Complete

**Issue:** `TypeChecker` is a single ~90-method impl covering decls, impls/traits, exprs, stmts, patterns, generics, intrinsics, drops, and unsafe tracking.
**Detail:** This is the workspace's biggest onboarding liability and the file where PHX-023/024 hid. The `lower/` directory demonstrates the intended layout (ctx/expr/stmt/func split); typeck never got the same treatment.
**Recommendation:** Split into `check/{decl,impl,expr,stmt,pattern,intrinsic}.rs` sharing the `TypeChecker` struct. Mechanical, no behavior change; do it after the Critical ownership fixes land to avoid churn.

**Resolution:** Split into `check/{mod,ctx,decl,impls,stmt,expr,pattern,intrinsic}.rs` — one shared `TypeChecker` struct with methods spread across sibling `impl` blocks (mirrors `lower/` module map). No behavior change; same 172/179 typeck tests pass (7 pre-existing failures unchanged).

---

**[PHX-032] Severity: Minor**
**Location:** `docs/design/language-v0-completion-roadmap.md` vs `typeck/trait_defaults.rs`, commit `ff51726`
**Reviewer:** Pragmatic Critic

**Status:** - [ ] Complete

**Issue:** Design docs are stale against the code — the completion roadmap lists trait default bodies (V0-063) and multi-payload `Result` match (V0-064) as open partials, but both are implemented and tested.
**Detail:** Per project rules the docs are the source of truth; when they trail the implementation, agents will re-implement or mis-scope work.
**Recommendation:** Documentation pass updating V0-063/V0-064 status and the trait-defaults note in `traits.md`.

### Backend (ir / lower / codegen / pxi / modules / build / link)

---

**[PHX-033] Severity: Major**
**Location:** `phx-compiler/src/link/mod.rs:225–241` (`patch_instruction`)
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** The linker rebases only `Const` (const_base) and `MakeStruct`/`MakeEnum`/`MakeArray`/`MakeTuple` (type_base); `GetField`, `SetField`, `MatchTag`, `MakeStr`, and `CallIndirect`-adjacent operands keep their module-local indices.
**Detail:** Verified: merged type tables *are* rebased (`r.type_id += type_base`, `link/mod.rs:122–136`), but instructions referencing them are not patched, so after linking they index the first module's region. This is masked today only because every module artifact embeds the *whole-program* type/constant tables (see PHX-036) — the unpatched operands accidentally hit identical entries. The moment tables become module-local, every cross-module field access or enum match miscompiles.
**Recommendation:** Patch every opcode whose operand is a type-table or const-pool index (the `Opcode` enum should drive this exhaustively — no `_` arms); add a two-module link test exercising `GetField`/`MatchTag` across the boundary, which fails today if tables are made module-local.

---

**[PHX-034] Severity: Major**
**Location:** `phx-compiler/src/codegen/emit.rs:404–406, 429–430, 565–566`
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** Missing callee/jump/drop-fn map entries silently encode operand `0` (`def_to_fn.get(...).unwrap_or(0)`, `block_starts.get(...).unwrap_or(0)`).
**Detail:** Verified for the `DropLocal` path. A codegen-internal inconsistency becomes structurally valid bytecode that calls function 0 or jumps to offset 0 — the verifier cannot catch it because it is well-formed. This converts ICEs into wrong-execution, the worst possible failure mode for a compiler.
**Recommendation:** Return `CodegenError` on any map miss. Never emit placeholder control-transfer or call targets.

---

**[PHX-035] Severity: Major**
**Location:** `phx-compiler/src/codegen/emit.rs:555–567` (`DropLocal`) vs `phx-bytecode/src/stack_effect.rs:63–66, 89–92`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** `DropLocal` emits `LoadLocal` + `Call` with no `Pop`; since `Call` nets +1 and `Return`'s stack effect pops nothing, every drop leaks one stack slot.
**Detail:** Verified in both files. The leak survives verification (Return imposes no depth requirement), inflates `stack_max`, and will trip `JoinDepthMismatch` whenever drop glue runs on one edge of a control-flow merge but not the other.
**Recommendation:** Emit `Pop` after the drop call (the compiler currently never emits `Pop` per `vm-linear.md` — that note will need updating), or define `Return`'s verifier contract to require a canonical depth. Needs a small design decision on the stack-at-return invariant; flag for the author.

---

**[PHX-036] Severity: Major**
**Location:** `phx-compiler/src/lower/mod.rs:57–82` (`lower_module`); `codegen/mod.rs` (`codegen_module`)
**Reviewer:** Pragmatic Critic

**Status:** - [x] Complete

**Issue:** Per-module artifacts embed the whole-program literal pool (and effectively whole-program tables); per-module lowering is a filter over a full-program lowering.
**Detail:** Object files are bloated, "incremental" artifacts are not module-local, and — critically — this is the accident that masks PHX-033.
**Recommendation:** Lower per module with module-local pools, fix PHX-033 in the same change, and let the existing link tests catch regressions.

---

**[PHX-037] Severity: Major**
**Location:** `phx-compiler/src/lower/ctx.rs:196–203` (`expr_ty`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Lowering's `ExprId` cursor silently falls back to `unit_ty()` when it drifts from typeck's expression order.
**Detail:** Fixed: `expr_ty` emits `LowerError::MissingExprType` on map miss within the function's expr range; `finish_expr_cursor` rejects `next_expr != expr_end`. Monomorphized impl methods always body-typecheck so `expr_types` is populated. Ordering invariant documented on `TypedProgram::expr_types`.
**Recommendation:** Emit `LowerError` on cursor miss. Document the ordering invariant on `TypedProgram::expr_types`.

---

**[PHX-038] Severity: Major**
**Location:** `phx-compiler/src/lower/expr.rs:774–788, 1344–1370` (`resolve_method_callee_for_ty`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Lowering re-implements method/trait resolution (walking `Ty::Named`, refs, aliases to find impl methods) instead of consuming typeck's answer.
**Detail:** Fixed: `MethodCallSiteMeta` records template callee and per-site mono args during typeck; monomorphization patches each site independently. Lowering reads `method_call_sites` only (plus `primitive_method_sites`); duplicate resolver deleted; missing entries emit `UnresolvedCallee`.
**Recommendation:** Record resolved callee `DefId` + mono args in a `method_call_sites`-style table during typeck; delete the lowering-side resolver.

---

**[PHX-039] Severity: Major**
**Location:** `phx-compiler/src/ir/block.rs:5–10`, `ir/mod.rs:15–22`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** No IR validator — nothing checks that blocks end in terminators, jump targets are valid block ids, or stack discipline holds before codegen.
**Detail:** Fixed: `ir/validate.rs` checks terminators, jump targets, unpatched loop-exit placeholders, and stack depth at merge blocks; shared stack simulation lives in `ir/stack_effect.rs`. The pass runs in debug builds between lower and codegen (`debug_validate_ir`); tests call `validate_ir` directly.
**Recommendation:** Add a debug-mode IR validation pass between lower and codegen (terminator-last, target-in-range, optional depth simulation).

---

**[PHX-040] Severity: Major**
**Location:** `phx-compiler/src/pxi/format.rs:336–367` (`parse_exports`)
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** Malformed `.pxi` exports arrays parse without error — the loop `break`s on the first unexpected byte and returns a partial export list.
**Detail:** Fixed: `parse_json_object_array` drives strict parsing for `exports` and `dependencies`; structural failures (missing keys, truncated arrays, unclosed objects, garbage tokens, missing required fields) return `PxiError::Parse` instead of partial lists. Unit tests cover malformed inputs for both arrays.
**Recommendation:** Make `parse_exports` return `Result` and fail on structural errors; the v1/v2 version gate (`format.rs:255–257`) is already strict, extend that rigor to the body.

---

**[PHX-041] Severity: Minor**
**Location:** `phx-compiler/src/codegen/const_pool.rs:56–66`; `lower/ctx.rs:112–114, 223–237`; `lower/func.rs:73`
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** Silent-fallback cluster: `pool_index_for_literal` docs say "panics" but it does `unwrap_or(literal_index)`; `LowerCtx::emit` drops instructions when the block index is bad; constant/block counters saturate at `u32::MAX`.
**Detail:** Fixed in `1511bcc`: `pool_index_for_literal` returns `CodegenError::MissingLiteralIndex`; `fill_from_ir` returns `CodegenError::SectionTooLarge`; `LowerCtx::emit`/`set_current` record `LowerError::InvalidBlockIndex`; `intern_const`/`fresh_block` and function index overflow record `LowerError::LimitExceeded`; lowering aborts when `LowerBag::has_errors()`. Unit tests cover invalid-block and error display paths.
**Recommendation:** Same policy as PHX-034: convert all to `LowerError`/`CodegenError`.

---

**[PHX-042] Severity: Minor**
**Location:** `phx-compiler/src/lower/expr.rs:1169–1186` (`lower_try_convert_err`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** `?`-with-`From` lowering uses `debug_assert!` + silent `return` on missing layout metadata — wrong codegen in release if typeck regresses.
**Detail:** Fixed in `1511bcc`: `lower_try_convert_err` records `LowerError::MissingTryConvertLayout` when return `Result` layout metadata is missing.
**Recommendation:** Replace with `LowerError`.

---

**[PHX-043] Severity: Minor**
**Location:** `phx-compiler/src/codegen/mod.rs:162–165` vs `232–237`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Single-module `codegen()` defaults a missing `main` to entry id `0`; the multi-module path correctly uses `ENTRY_NONE`.
**Detail:** Fixed in `92d51a1`: `codegen()` uses `ENTRY_NONE` when `ir.entry` is absent, matching `codegen_module()`. Test `codegen_library_module_without_entry_uses_entry_none` guards library-style single-module compiles.
**Recommendation:** Use `ENTRY_NONE` when `ir.entry` is absent.

---

**[PHX-044] Severity: Suggestion**
**Location:** `phx-compiler/src/lower/expr.rs` (1,975 lines); `build/driver.rs` (913 lines); `modules/mod.rs:6–8` (dead `interface_loader.rs`)
**Reviewer:** Pragmatic Critic

**Status:** - [x] Complete

**Issue:** Backend God modules and one dead scaffolding module.
**Recommendation:** Flag for review — split `expr.rs` into match/call/intrinsic/assign units, `driver.rs` into incremental/artifact/link-map units; wire or delete `interface_loader.rs`.

**Resolution:** Deleted dead [`interface_loader.rs`](source/phx-compiler/src/modules/interface_loader.rs) (`exports_for_dependency` in `import_resolve.rs` is the live `.pxi` path). Split [`lower/expr.rs`](source/phx-compiler/src/lower/expr/) into `literal`, `assign`, `call`, `intrinsic`, and `match` submodules. Split [`build/driver.rs`](source/phx-compiler/src/build/driver/) into `package`, `incremental`, `artifacts`, `link_map`, and `util` submodules. Public APIs unchanged.

---

## phx-bytecode

Owns the PHX0 contract: header/section encode-decode, single `Opcode` enum (correctly shared with the compiler — no duplicate tables), per-opcode operand validation, and CFG stack-depth analysis (`stack_flow.rs`). The verifier is real and tested, including a dedicated mutation-test suite — rare discipline at this stage. But it has one soundness hole and several contract gaps.

---

**[PHX-045] Severity: Critical**
**Location:** `phx-bytecode/src/stack_flow.rs:182–204` (`terminators_successors`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** The stack-depth CFG never models `JumpIfFalse` — its targets fall into the `_ => Vec::new()` wildcard, so neither the branch target nor the fall-through is analyzed; `JumpIfTrue` without an immediately following `Jump` likewise loses its fall-through edge.
**Detail:** Verified directly. Phoenix codegen only emits the `JumpIfTrue`+`Jump` idiom, so compiler output is analyzed correctly — but the verifier's job is *hostile* bytecode. A hand-crafted module using `JumpIfFalse` (a documented, decoded, interpreted opcode) gets no stack-depth verification on that path and can reach the interpreter with an underflowing branch. The interpreter's runtime checks make this an error-not-UB today, but the documented invariant "stack depth consistency per basic block" is not enforced. Note the cause: a `_` wildcard on an `Opcode` match, which the coding standard explicitly forbids.
**Recommendation:** Model `JumpIfFalse` (target + fall-through) and `JumpIfTrue` fall-through symmetrically; remove the `_` arm in favor of exhaustive opcode matching; add mutation tests for a `JumpIfFalse` program with an underflowing false-branch.

**Resolution:** Added `conditional_branch_successors` in [`stack_flow.rs`](source/phx-bytecode/src/stack_flow.rs) for both `JumpIfTrue` and `JumpIfFalse` (branch operand + alternate via fall-through or following `Jump` idiom). Replaced `_` wildcard with exhaustive `Opcode` match in `terminators_successors`. Regression tests: `jump_if_false_branch_underflow_rejected`, `jump_if_true_fallthrough_underflow_rejected` (unit), `mutate_jump_if_false_branch_underflow_rejected` (mutation).

---

**[PHX-046] Severity: Major**
**Location:** `phx-bytecode/src/verify.rs:689–697`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Jump opcodes don't validate operand count — a zero-operand `Jump` gets target `0` via `unwrap_or(0)` and verifies if offset 0 is an instruction boundary (it always is).
**Detail:** Verified. Same pattern exists for `Call`'s callee operand (`verify.rs:681`).
**Recommendation:** Reject wrong-arity instructions as `MalformedInstruction` for every opcode with a defined operand contract.

**Resolution:** Added `operands.len() != 1` guard for `Jump` / `JumpIfTrue` / `JumpIfFalse` in `verify_operands` (replacing `unwrap_or(0)` default). Audited remaining opcodes — all other families already reject wrong arity; `Call` was already guarded. Regression tests: `reject_zero_operand_jump`, `reject_zero_operand_jump_if_true`, `reject_zero_operand_jump_if_false`, `reject_extra_operand_jump`, `reject_zero_operand_call`, `mutate_jump_zero_operands_rejected`.

---

**[PHX-047] Severity: Major**
**Location:** `phx-bytecode/src/verify.rs:328–368` (`verify_header_and_sections`)
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** `verify` never checks `header.version_major/minor`; only `Header::decode` gates versions, so in-memory modules (the linker output path) bypass version validation. Additionally, sections are validated only for `offset+length ≤ file_len` — overlapping sections and duplicate section kinds (decode is last-wins, `module.rs:161–177`) are accepted, contrary to `vm-linear.md`.
**Recommendation:** Validate version fields and pairwise section ranges in `verify`; reject duplicate kinds.

**Resolution:** Added `FileHeader::validate_version` and `validate_section_table` (bounds, unique kinds, pairwise non-overlap). `verify_header_and_sections` checks version on in-memory headers, `section_count` consistency with encoded bytes, and the full section table. `BytecodeModule::decode` validates the section table before payload decode (no last-wins on invalid layouts). Regression tests in `verify.rs` and `verify_mutation.rs`.

---

**[PHX-048] Severity: Major**
**Location:** `phx-bytecode/src/const_pool.rs:64`, `function.rs:69`, `types.rs:71`, `local_layout.rs:95`, `instr.rs:38`
**Reviewer:** Rust Expert

**Status:** - [x] Complete

**Issue:** Decode pre-allocates `Vec::with_capacity(count)` from untrusted `u32` counts before validating against remaining section bytes.
**Detail:** A 16-byte hostile file can declare `count = 0xFFFF_FFFF` and force a multi-GB allocation at *decode* time — before the verifier ever runs. With `panic = "abort"` in release, allocation failure kills the process.
**Recommendation:** Clamp capacity to `remaining_bytes / min_entry_size` before pre-sizing.

**Resolution:** Added `decode::checked_entry_count` and applied it before `with_capacity` and decode loops in `const_pool`, `types`, `local_layout`, `function`, and `instr`. Declared counts exceeding `remaining / min_entry_size` return existing `Truncated` errors without huge allocation or iteration. Regression tests in `decode_limits.rs`.

---

**[PHX-049] Severity: Minor**
**Location:** `phx-bytecode/src/verify.rs:456–460, 721–728, 730–740, 483–485`
**Reviewer:** Language Designer

**Status:** - [x] Complete

**Issue:** Verifier fidelity cluster — `JoinDepthMismatch` is reported as `StackUnderflow` (losing the merge diagnostic); `Trap` is verified as one-operand while `opcode.rs` documents zero operands and the interpreter treats the operand as optional; `MakeStr` doesn't check the const entry is `ConstTag::Bytes`; all local-layout checks are skipped when the layout table is empty even though the compiler always emits layouts at format minor ≥ 1.
**Recommendation:** Add a dedicated join-mismatch error; align the `Trap` contract across docs/verifier/interpreter; check `MakeStr`'s tag; require layouts when `version_minor ≥ 1 && local_count > 0`.

**Resolution:** Dedicated `VerifyError::JoinDepthMismatch`; verifier rejects non-empty `Trap` operands and `MakeStr` non-`Bytes` pool entries; `verify_function_layout` and `check_local_slot` require layout rows at format minor ≥ 1 when `local_count > 0`; VM `Trap` always returns `GivenMismatch`; codegen emits zero-operand `Trap`.

---

**[PHX-050] Severity: Minor**
**Location:** `phx-bytecode/src/verify.rs:328–331`; `instr.rs:18`
**Reviewer:** Pragmatic Critic

**Status:** - [ ] Complete

**Issue:** Section bounds are validated by *re-encoding* the module (coupling verify correctness to encoder correctness), and `Instruction::encode` silently truncates operand counts > 255.
**Recommendation:** Validate the in-memory structure directly; make `encode` fallible.

---

## phx-vm

A fully **safe-Rust** interpreter (zero `unsafe` in the crate — better than the design docs anticipated): `Machine` with operand stack, frames with typed locals, aggregate arena, and a bump byte-heap with an exact `(ptr, size)` ledger giving real double-free/invalid-free errors. Every stack pop and index is checked; no panic paths found on malformed input. The weaknesses are numeric-semantics fidelity and the trust boundary.

---

**[PHX-051] Severity: Major**
**Location:** `phx-vm/src/interpreter.rs:1172–1189` (`scalar_as_i128`), `1052–1104` (`arith_scalar`), `1115–1134` (`binop_cmp`)
**Reviewer:** Language Designer

**Status:** - [ ] Complete

**Issue:** All integer arithmetic and comparison funnels through `i128` — `U128(x) => x as i128` mis-signs values above `i128::MAX`, breaking `u128` div/mod/comparison; float `Mod`/`Pow` return the unrelated error `VmError::InvalidConstPayload`.
**Detail:** Verified. Add/sub/mul survive via two's-complement truncation, but `u128::MAX / 2` computes as `-1 / 2`, and `binop_cmp` orders large `u128` values negative. `wide-integers.md` promises width-accurate execution through `u128`.
**Recommendation:** Split signed/unsigned arithmetic paths on `PrimitiveKind`; give float `Mod`/`Pow` either real semantics or a dedicated unsupported-op error.

---

**[PHX-052] Severity: Major**
**Location:** `phx-vm/src/interpreter.rs:1149–1155` (shifts), `1118–1121` (float compare)
**Reviewer:** Language Designer

**Status:** - [ ] Complete

**Issue:** Shift amounts are not masked to the operand width (`wrapping_shl(bi as u32)` — `1_u8 << 9` zeroes instead of width-masked behavior, and `bi as u32` truncates arbitrarily), and NaN comparisons use `partial_cmp(..).unwrap_or(Equal)`, making `NaN == NaN` true and `NaN < x` follow Equal-ordering — non-IEEE behavior.
**Detail:** Verified. Neither shift semantics nor NaN ordering is pinned down in the design docs — the implementation chose silently.
**Recommendation:** Requires a design decision (document Phoenix shift/NaN semantics in `type-system.md`/`wide-integers.md`), then implement to spec with width-edge tests. Flag for the author; IEEE-754 NaN inequality is the strongly recommended default.

---

**[PHX-053] Severity: Major**
**Location:** `phx-vm/src/frame.rs:168–177` (`alloc_bytes`), `interpreter.rs:375–376`
**Reviewer:** Rust Expert

**Status:** - [ ] Complete

**Issue:** `Alloc` resizes the heap by an unchecked runtime `u32` size — verified bytecode can demand 4 GB per instruction in a loop; allocation failure aborts the process (`panic = "abort"`).
**Detail:** Also note `u64::try_from(start).unwrap_or(u64::MAX)` returns a garbage pointer on (unreachable today) overflow rather than erroring.
**Recommendation:** Impose a configurable heap cap returning `VmError::OutOfMemory`; remove the `unwrap_or(u64::MAX)` sentinel.

---

**[PHX-054] Severity: Major**
**Location:** `phx-vm/src/frame.rs:191–211` (`free_bytes`), `interpreter.rs:916–928`
**Reviewer:** Language Designer

**Status:** - [ ] Complete

**Issue:** `Free` removes the ledger entry and zeroes bytes, but subsequent loads through dangling pointers succeed (reading zeros) because heap loads only check `end ≤ heap.len()`.
**Detail:** Not UB (safe Rust), but silently wrong values flow from use-after-free instead of a trap — at odds with Phoenix's ownership-as-safety story, and it will mask std `DynamicArray`/`UniquePtr` bugs during the std bootstrap, exactly when detection matters most.
**Recommendation:** Validate heap loads/stores against live ledger ranges (the ledger already exists; this is a range query), at least in a checked/debug VM mode. Flag for review whether release-mode checking is wanted.

---

**[PHX-055] Severity: Minor**
**Location:** `phx-vm/src/lib.rs:36–38` (`run`), `interpreter.rs:656–659` (`function_code`)
**Reviewer:** Pragmatic Critic

**Status:** - [ ] Complete

**Issue:** The verify-before-execute invariant is enforced only by CLI convention — `phx_vm::run` executes whatever it is given (and the in-crate mutation tests document that unverified invalid modules run "successfully"); separately, `function_code` returns `&[]` on bad offsets so `main` falling off the end returns `Ok`.
**Detail:** All CLI paths verify (confirmed: `run.rs:129–133`, `compile.rs:73–76`, `build/driver.rs:226`), so this is a library-boundary hazard, not a product bug.
**Recommendation:** Add `run_verified()` (or a `VerifiedModule` newtype that is the only thing `run` accepts) so the type system carries the invariant; treat PC-past-end as an error.

---

**[PHX-056] Severity: Minor**
**Location:** `phx-vm/src/error.rs:5–48`; `phx-bytecode/src/opcode.rs:84`
**Reviewer:** Language Designer

**Status:** - [ ] Complete

**Issue:** `VmError` carries no `(function_id, pc)` and `Trap` carries no payload metadata, so runtime failures cannot be attributed to source even coarsely.
**Recommendation:** Record `(function_id, pc)` in `VmError` at dispatch; full source maps are deferred (see PHX-070, Deferred Work).

---

**[PHX-057] Severity: Suggestion**
**Location:** `phx-vm/src/interpreter.rs` (1,245 lines); `foreign.rs:38–68`
**Reviewer:** Pragmatic Critic

**Status:** - [ ] Complete

**Issue:** Interpreter is a single God module; foreign-stub ids are process-global and registration-order-dependent (fine for the documented FFI Phase A, unstable for real linking). The frame/heap model is single-threaded `Vec`s with no execution-context abstraction — adding the scheduler will be invasive.
**Recommendation:** Flag for review — split dispatch/arith/memory/aggregates; introduce an `ExecutionContext`-style boundary before scheduler work begins (post-beta).

---

## phx-cli (and `phx`)

`phx` is a thin binary with a `catch_unwind` ICE boundary (exit 6); `phx-cli` holds command logic with structured exit codes. `check`/`compile`/`run`/`build` all verify bytecode before writing or executing. Workflow correctly distinguishes project vs standalone.

---

**[PHX-058] Severity: Minor**
**Location:** `source/phx/src/main.rs:13–14`
**Reviewer:** Rust Expert

**Status:** - [ ] Complete

**Issue:** The ICE handler installs an empty panic hook, so even the panic *message* is unavailable when reporting internal errors — debugging an ICE requires rebuilding.
**Recommendation:** Print message + backtrace when `PHX_ICE_DEBUG=1` (or `RUST_BACKTRACE` is set).

---

**[PHX-059] Severity: Minor**
**Location:** `phx-cli/src/commands/check.rs:141–158` vs `standalone.rs:90–100`
**Reviewer:** Language Designer

**Status:** - [ ] Complete

**Issue:** Lints (deprecation, must-use) run only on `phx check`, not on `compile`/`run`/`build` — users compiling directly never see warnings.
**Recommendation:** Run lint on all compiling commands or document the asymmetry. Flag for review.

---

## Tests and harness

Three-layer harness (fixtures → `phx-test` lib → `tests/integration`) with golden-stderr support, an FS lock for parallel project builds, and a real mutation-test suite for the verifier. Strong bones, uneven coverage.

---

**[PHX-060] Severity: Major**
**Location:** `justfile:30–39`
**Reviewer:** Pragmatic Critic

**Status:** - [ ] Complete

**Issue:** `just pre-commit` — the agent completion gate — runs only fmt/clippy/doc/dep-check plus **three** integration tests (`cli_e2e`, `run_smoke`, `diagnostics`); CI runs `cargo test --workspace`.
**Detail:** Typeck, verifier, VM, and build unit/integration tests are all outside the gate that the workspace rules tell agents to trust. Regressions in exactly the areas this review flags (ownership, verifier, link) pass pre-commit.
**Recommendation:** Add `cargo test --workspace` (or a curated fast superset including typeck/verify/vm suites) to pre-commit, or rename the gate so it doesn't imply commit-readiness.

---

**[PHX-061] Severity: Major**
**Location:** `phx-compiler/tests/typeck.rs`; `phx-bytecode/tests/verify_mutation.rs`; `tests/integration/diagnostics/`
**Reviewer:** Pragmatic Critic

**Status:** - [ ] Complete

**Issue:** Coverage gaps cluster precisely on this review's Criticals: zero tests for moves across `if`/`match` arms or loop iterations (PHX-023/024), no `JumpIfFalse`/join-depth/section-overlap/oversized-alloc mutation tests (PHX-045/047/053), no multi-module link execution tests (PHX-033), no `IndexStore`/drop-glue stack verification (PHX-035, in-flight slice work), and only 6 golden diagnostics vs ~15 substring-only negative fixtures.
**Recommendation:** Land regression tests alongside each fix; grow the golden set for move/trait/unsafe diagnostics.

---

**[PHX-062] Severity: Minor**
**Location:** e.g. `phx-compiler/tests/typeck.rs:151` and the `if !path.is_file() { return; }` pattern; `phx-compiler` ↔ `tests/phx-test` dev-dep cycle
**Reviewer:** Pragmatic Critic

**Status:** - [ ] Complete

**Issue:** Fixture-gated tests silently pass when the fixture is missing — a moved directory turns a suite green-by-vacuity; the dev-only dependency cycle lengthens the test build graph.
**Recommendation:** Panic (in tests) when an expected fixture is absent; flag the cycle for review.

---

## Cross-Cutting Issues

**Span propagation.** Spans are excellent from lexer through typeck (`Node<T>` everywhere, errors carry spans) and then fall off a cliff: `ir/inst.rs` has **no** span field (verified), codegen/verify/VM errors have no source mapping, and several synthesizers fabricate `Span::new(0,0)` (resolver `self` receiver `walk.rs:471,507`; `if var` desugar `lower/stmt.rs:230`; import-cycle fallback `graph.rs:209`; return-mismatch `check.rs:2178`).

---

**[PHX-063] Severity: Major**
**Location:** `phx-compiler/src/ir/inst.rs` (whole file); `lower/stmt.rs:230`; `resolver/walk.rs:471,507`; `modules/graph.rs:209`; `typeck/check.rs:2178`
**Reviewer:** Language Designer

**Status:** - [ ] Complete

**Issue:** Span propagation ends at typeck — IR carries no spans, lowering/codegen errors and VM traps cannot cite source, and synthetic nodes use zero spans that render as caret-at-byte-0.
**Detail:** `vm-linear.md` reserves section 5 for future debug symbols, so omitting spans from *bytecode* is by design; omitting them from the *IR* is not documented anywhere and makes every backend diagnostic and future debugger work harder. Zero-span synthesis violates "never discard span information."
**Recommendation:** Carry a span (or side table keyed by instruction index) on `IrInst` for diagnostics; replace all `Span::new(0,0)` with the span of the construct being desugared. Bytecode-level source maps stay deferred.

**Error recovery.** Strategy is consistent and good: every pass collects multiple errors in a bag (`ParseBag`, `DiagnosticBag`, `TypeCheckBag`, `LowerBag`); the pipeline aborts *between* stages on a non-empty bag; the CLI wraps everything in `catch_unwind` → exit 6. The two real gaps are PHX-005 (partial ASTs discarded, so recovery never crosses the parse boundary) and the silent-fallback family (PHX-016/017/034/037/041), which is worse than panicking — those paths neither panic nor diagnose.

**Crate dependency graph.** Verified clean and acyclic in production: `phx-diagnostics` ← `phx-syntax` ← `phx-compiler` (also ← `phx-bytecode`); `phx-vm` ← {`phx-bytecode`, `phx-diagnostics`} — `**phx-vm` correctly does not depend on `phx-compiler`**; `phx-cli` links all; `phx` ← `phx-cli`. Zero external crates anywhere, enforced by `tests/ci/check-deps.sh`. Only dev-time wrinkles: `phx-compiler` ↔ `phx-test` cycle and `phx-bytecode` dev-depending on `phx-vm` (PHX-062).

**Invariant enforcement.** "Operands are indices only" holds (operands are `Vec<u32>`; the symbols section is unwritten as documented; `Pop` is defined-but-never-emitted, confirmed). "Verify before execute" holds on every CLI path but is convention-only at the `phx_vm::run` library boundary (PHX-055). "Copyable = bitwise copy" is enforced in typeck (`CopyableDropConflict` exists) and trusted by the VM — appropriate. "No `_` wildcards on AST/token/opcode enums" is partially held: PHX-019 resolved for AST/token in compiler passes; PHX-045 resolved for `terminators_successors` opcode match in `stack_flow.rs`. No `Vec`-naming leaks into the language surface (std ships `DynamicArray`); `str` as a view type is sanctioned by `mvp.md` (the older "byte-first, no string" framing in the workspace rules is stale relative to the docs, not a code bug).

**Post-MVP readiness.** Honest assessment: the **typeck side-table architecture** and **PHX0 versioned format** are good extension points. Three things will need surgery, none of which is stubbed: (1) the ownership tracker has no CFG notion at all — the full borrow checker cannot grow out of a linear `Vec<BindingEntry>`; expect replacement, which makes fixing PHX-023/024 with a properly shaped fork/join model doubly valuable; (2) the VM has no execution-context abstraction — scheduler work means refactoring `Machine`/frame ownership first (PHX-057); (3) lowering's order-coupled `ExprId` cursor (PHX-037) is fragile under any future reordering optimization; a keyed map or explicit typed-IR would be sturdier. The std bootstrap substrate, by contrast, is largely *done*: `Option`/`Result`/`?`/`Drop`/`DynamicArray`/allocator traits exist as std-authored Phoenix code with a path-scoped kernel — the remaining risk there is PHX-026's name-based fallbacks.

---

## Finding Summary


| ID      | Status | Severity     | Crate           | Short Description                                                        |
| ------- | ------ | ------------ | --------------- | ------------------------------------------------------------------------ |
| PHX-001 | - [x]  | Major        | phx-syntax      | Raw `*mut ParseBag` in only unsafe block                                 |
| PHX-002 | - [x]  | Major        | phx-syntax      | Parser peeks clone full `TokenKind` with heap payloads                   |
| PHX-003 | - [x]  | Major        | phx-syntax      | Invalid `Symbol` silently resolves to placeholder string                 |
| PHX-004 | - [x]  | Major        | phx-syntax      | Block statements strip `StmtNode` span                                   |
| PHX-005 | - [x]  | Major        | phx-syntax      | Partial AST discarded on any parse error                                 |
| PHX-006 | - [x]  | Minor        | phx-syntax      | Unclosed `{` exits block loop with no diagnostic                         |
| PHX-007 | - [x]  | Minor        | phx-syntax      | `range_pattern` fails as `InvalidPattern` instead of `UnsupportedSyntax` |
| PHX-008 | - [x]  | Minor        | phx-syntax      | Attribute semantics resolved in syntax crate                             |
| PHX-009 | - [x]  | Minor        | phx-syntax      | Wasted allocations in hot paths                                          |
| PHX-010 | - [x]  | Suggestion   | phx-syntax      | Test/grammar drift on keywords and `extern`/`mod`                        |
| PHX-011 | - [x]  | Suggestion   | phx-syntax      | API hygiene: missing `#[non_exhaustive]`, broad `pub(crate)`             |
| PHX-012 | - [x]  | Major        | phx-diagnostics | Parse-error formatting lives in wrong crate                              |
| PHX-013 | - [x]  | Minor        | phx-diagnostics | Incomplete `phx explain` coverage                                        |
| PHX-014 | - [x]  | Minor        | phx-diagnostics | `LexError`/`ParseError` missing `#[non_exhaustive]`                      |
| PHX-015 | - [x]  | Suggestion   | phx-diagnostics | ~40 variants maintained across 4 parallel match sites                    |
| PHX-016 | - [x]  | Major        | phx-compiler    | Out-of-bounds `TypeId` silently resolves to `Ty::Unit`                   |
| PHX-017 | - [x]  | Major        | phx-compiler    | `fn_def_for` failure proceeds with `DefId::from_raw(0)`                  |
| PHX-018 | - [x]  | Major        | phx-compiler    | Derive expansion panics on intern table exhaustion                       |
| PHX-019 | - [x]  | Major        | phx-compiler    | `#![allow(unreachable_patterns)]` in typeck and lower                    |
| PHX-020 | - [x]  | Minor        | phx-compiler    | Wholesale clones across pass boundaries                                  |
| PHX-021 | - [x]  | Minor        | phx-compiler    | `DefId` allocation saturates silently at `u32::MAX`                      |
| PHX-022 | - [x]  | Suggestion   | phx-compiler    | Internal types are public crate API                                      |
| PHX-023 | - [x]  | **Critical** | phx-compiler    | Ownership state not forked/joined across `if`/`match` arms               |
| PHX-024 | - [x]  | **Critical** | phx-compiler    | No loop back-edge analysis for move detection                            |
| PHX-025 | - [x]  | Major        | phx-compiler    | No partial-move tracking for field access                                |
| PHX-026 | - [x]  | Major        | phx-compiler    | Name-only std type lookups bypass `StdKernel` path anchoring             |
| PHX-027 | - [x]  | Major        | phx-compiler    | Generic arity mismatch returns `true` and skips bound checks             |
| PHX-028 | - [x]  | Major        | phx-compiler    | Type unification does not recurse structurally                           |
| PHX-029 | - [x]  | Minor        | phx-compiler    | Lint pass has no type information                                        |
| PHX-030 | - [x]  | Minor        | phx-compiler    | Undocumented `Debug` derive; generic derive unsupported                  |
| PHX-031 | - [x]  | Major        | phx-compiler    | `TypeChecker` is a 6,030-line God module                                 |
| PHX-032 | - [ ]  | Minor        | phx-compiler    | Design docs stale on trait defaults and `Result` match                   |
| PHX-033 | - [x]  | Major        | phx-compiler    | Linker does not rebase all type-referencing opcodes                      |
| PHX-034 | - [x]  | Major        | phx-compiler    | Missing codegen map entries silently encode operand 0                    |
| PHX-035 | - [x]  | Major        | phx-compiler    | `DropLocal` leaks one stack slot per drop call                           |
| PHX-036 | - [x]  | Major        | phx-compiler    | Per-module artifacts embed whole-program tables                          |
| PHX-037 | - [x]  | Major        | phx-compiler    | Lowering `ExprId` cursor silently falls back to `unit_ty()`              |
| PHX-038 | - [x]  | Major        | phx-compiler    | Lowering re-implements method/trait resolution                           |
| PHX-039 | - [x]  | Major        | phx-compiler    | No IR validator between lower and codegen                                |
| PHX-040 | - [ ]  | Major        | phx-compiler    | Malformed `.pxi` parses without error                                    |
| PHX-041 | - [x]  | Minor        | phx-compiler    | Silent-fallback cluster in const pool and lower                          |
| PHX-042 | - [x]  | Minor        | phx-compiler    | `?`-lowering uses `debug_assert!` instead of `LowerError`                |
| PHX-043 | - [x]  | Minor        | phx-compiler    | Single-module codegen defaults missing `main` to entry 0                 |
| PHX-044 | - [x]  | Suggestion   | phx-compiler    | Backend God modules and dead `interface_loader.rs`                       |
| PHX-045 | - [x]  | **Critical** | phx-bytecode    | `JumpIfFalse` never modeled in stack-depth CFG                           |
| PHX-046 | - [x]  | Major        | phx-bytecode    | Jump opcodes don't validate operand count                                |
| PHX-047 | - [x]  | Major        | phx-bytecode    | Version and section overlap not validated in `verify`                    |
| PHX-048 | - [x]  | Major        | phx-bytecode    | Decode pre-allocates from untrusted `u32` counts                         |
| PHX-049 | - [x]  | Minor        | phx-bytecode    | Verifier fidelity cluster (`Trap`, `MakeStr`, layout checks)             |
| PHX-050 | - [ ]  | Minor        | phx-bytecode    | Section bounds validated by re-encoding; `encode` silently truncates     |
| PHX-051 | - [ ]  | Major        | phx-vm          | All integer arithmetic funnels through `i128`, breaking `u128`           |
| PHX-052 | - [ ]  | Major        | phx-vm          | Shift amounts unmasked; NaN comparison non-IEEE                          |
| PHX-053 | - [ ]  | Major        | phx-vm          | Unchecked `u32` alloc size; no heap cap                                  |
| PHX-054 | - [ ]  | Major        | phx-vm          | Use-after-free reads zeros instead of trapping                           |
| PHX-055 | - [ ]  | Minor        | phx-vm          | Verify-before-execute not enforced at library boundary                   |
| PHX-056 | - [ ]  | Minor        | phx-vm          | `VmError` carries no `(function_id, pc)`                                 |
| PHX-057 | - [ ]  | Suggestion   | phx-vm          | Interpreter God module; no `ExecutionContext` abstraction                |
| PHX-058 | - [ ]  | Minor        | phx-cli         | ICE handler discards panic message                                       |
| PHX-059 | - [ ]  | Minor        | phx-cli         | Lints only run on `phx check`, not `compile`/`run`/`build`               |
| PHX-060 | - [ ]  | Major        | tests           | `just pre-commit` gate excludes typeck/verifier/VM suites                |
| PHX-061 | - [ ]  | Major        | tests           | Coverage gaps on all three Critical findings                             |
| PHX-062 | - [ ]  | Minor        | tests           | Fixture-gated tests silently pass when fixture is missing                |
| PHX-063 | - [ ]  | Major        | cross-cutting   | Span propagation ends at typeck; IR carries no spans                     |


