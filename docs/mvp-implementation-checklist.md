# Phoenix MVP implementation checklist

**Purpose:** Single reference for humans and coding agents: what the [MVP spec](design/mvp.md) requires, what is already implemented under `source/`, and what remains for a **credible demo** (working control flow, arithmetic, functions, types — not post-MVP runtime).

**How to use with agents:** Attach this file to prompts. Work top-down in [Suggested implementation order](#suggested-implementation-order). For each row, read **Status**, implement in **Where** until **Acceptance** passes. Do not invent semantics — [design docs](design/README.md) are authoritative.

**Last surveyed:** MVP partials (recursion + `given` enum fixtures). **`cargo test --workspace`:** all crates green. **`cargo clippy --workspace --all-targets -- -D warnings`:** green. **`tests/cli/run.sh`:** 31 single-file fixtures + `modules/main.phx`. **MVP acceptance:** [tests/cli/fixtures/mvp_acceptance/](../tests/cli/fixtures/mvp_acceptance/). **CI:** `.github/workflows/ci.yml` `rust` (fmt, clippy, tests) + `cli` (check, run, build, compile, help).

---

## MVP audit matrix

High-level pass/fail against [mvp.md](design/mvp.md) and [type-system.md](design/features/type-system.md). Labels: **pass** = end-to-end or spec-aligned; **partial** = implemented with known gaps; **missing** = not started; **deferred** = intentionally post-MVP.

### [mvp.md](design/mvp.md) — in scope

| Area | Status | Notes |
|------|--------|-------|
| Pipeline: parse → typeck → lower → bytecode → VM | **pass** | Full driver in `compile.rs`; verifier before run |
| `main :: () =>` required | **pass** | `ResolveError::MissingMain` / `InvalidMainSignature` |
| Declarations (`const`, `var`, functions) | **pass** | CLI + unit tests |
| Numeric primitives, `bool`, `()`, tuples | **pass** | Width-faithful VM; `primitives_*.phx` fixtures |
| Raw pointers, `&T` / `&mut T` in signatures | **partial** | Address-of + deref; no borrow checker |
| Fixed arrays, slices | **partial** | Arrays + stack-backed slice views; no heap slices |
| User `struct` / `enum` / type aliases | **pass** | Type aliases resolve + unify (`typeck.rs` tests) |
| Traits: parse + `Type :: impl :: Trait` | **pass** | Static dispatch; `trait_eq.phx` |
| Control flow (`if`, `match`, loops, `return`, `given`) | **pass** | `given` pattern dispatch + enum exhaustiveness |
| Expressions + explicit `as` casts | **pass** | Cross-width/int/float explicit `as`; VM `Cast` opcode |
| Modules `#import` + `pub` (M1) | **pass** | `--module-src` / `check_file_with_module_path` |
| M2 project build (`phoenix.toml`, linker) | **pass** | `build.sh`, `run_build.rs`, `run_dep_build.rs` |
| CLI (`phx check`, `run`, `compile`, `build`) | **pass** | CI: `rust` + `cli` jobs (`check`, `run`, `build`, `compile`, `help`) |
| Use-after-move (MVP ownership) | **pass** | Move-site `note:` in `use_after_move.phx` |

### [mvp.md](design/mvp.md) — out of scope (correctly absent)

| Area | Status | Notes |
|------|--------|-------|
| Scheduler, actors, `@spawn` / mailboxes | **deferred** | Parse-rejected or documented only |
| Full borrow checker | **deferred** | MVP: use-after-move only |
| Std I/O, networking, collections | **deferred** | No std I/O in VM |
| JIT, hot reload, `#derive` codegen | **deferred** | — |
| Primitive `string` (owned) | **done** | Core **`str`** UTF-8 view + `"…"` literals; std **`String`** deferred |

**Rough in-scope pass rate:** ~12 **pass**, ~4 **partial**, 0 **missing** on MVP-required surface (excluding deferred rows).

### [type-system.md](design/features/type-system.md) — deterministic rules

| Rule | Status | Notes |
|------|--------|-------|
| 1–2. Literal defaults (`s32`, `u`→`u32`, `f32`) | **pass** | `typeck/builtins.rs` |
| 3. No implicit numeric widening | **pass** | `mixed_width.phx` fails check |
| 4–6. Inference, explicit params, default ret `()` | **pass** | Tests + fixtures |
| 7. `if` / `match` branch unification | **pass** | `unify.rs` |
| 8. `return;` only for `()` | **pass** | typeck tests |
| 9. Operators on primitives / `bool` only | **pass** | User types rejected for ops |
| 10. Generic inference local only | **partial** | Named types + args scaffold |
| Option / Result / `?` rejected until std | **pass** | `*_unresolved_until_std` tests |
| Enum `match` exhaustiveness | **pass** | `NonExhaustiveMatch`; struct variants covered |
| Type aliases | **pass** | `type_alias_*` tests in `typeck.rs` |

**Type-system rule pass rate:** 11 **pass**, 0 **partial**.

### Known gaps (do not assume done)

| Issue | Impact |
|-------|--------|
| `#import` via in-process `compile_source` | Single-buffer API has **no** module root → `ImportNotSupported`. Use `check_file` / `check_file_with_module_path`, `compile_to_module*`, or `build_project`. |
| `#import` via CLI on one file | `phx check` / `phx run <file>` use **parent directory** as module root (same as `check_file`). Multi-file trees need `--module-src` or `phoenix.toml` (M2). |
| Explicit drop / scopes | No `Drop` opcodes or scope-end deallocation; memory model TBD |
| Heap user surface | `ALLOC` opcode + VM heap exist; no language syntax for heap boxes yet |
| Generics | V0-020/V0-021: generic decls, local inference, monomorphization (`id$s32`-style mangling); V0-022/V0-023: generic enum match, trait associated types; V0-024: cross-crate `.pxi` mangled fn exports |
| Parser ergonomics | Bounded fixes (e.g. unclosed `(`); broader grammar ambiguities may remain |

---

| Area | State |
|------|--------|
| Scalars | Width-faithful `ScalarValue` (`Bool`, `I8`…`I128`, `U8`…`U128`, `F32`, `F64`, `Ptr`) — not a shared integer lane |
| Locals / stack | Typed slots; `prim_kind` operands on const, load/store, and arithmetic |
| Aggregates | Arena handles: struct, enum, tuple, fixed array, **slice** `(ptr, len)` |
| PHX0 | Format minor **1**; **5** sections (constants, types, functions, code, **local layouts**) |
| Opcodes | **43** wired (`0`–`42`), including `MakeSlice`, `AddressOfLocal`, `PtrLoad`/`PtrStore`, `Alloc` (internal) |
| Stack verify | CFG join analysis in `stack_flow.rs` (deep `&&`/`||` chains) |
| Lifetime / drop | **Not implemented** — values live until frame/arena teardown; see [Roadmap](#roadmap-beyond-single-file-mvp) |
| Text | Core **`str`** view (`"…"` literals, rodata); **`[u8; N]`** / `b"…"` for binary; std **`String`** (owned) post-std |

---

## Demo bar (minimum showcase program)

A credible MVP demo `.phx` should be able to:

- [x] Define `main :: () => { … }` and fail compile without it
- [x] Declare top-level functions and call them with typed parameters
- [x] Use `const` / `var`, assignment, and `s32` arithmetic (`+`, `-`, `*`, `/`, comparisons)
- [x] Use `if` / `else` as expressions with unified branch types
- [x] Use `while`, `loop`, `break`, `continue`, `return`
- [x] Use `match` on `s32` / `bool` literals and `_` (`match scrutinee { … }`, no required parens)
- [x] Use short-circuit `&&` / `||` on `bool`
- [x] Construct and use **user** `struct` / `enum` values with field/tag access at runtime
- [x] Explicit `expr as Type` casts where types differ (per [type-system.md](design/features/type-system.md))
- [x] Run via `phx run file.phx` after bytecode verify (no panic on valid programs)
- [ ] *(Post-MVP std)* `Option` / `Result` / `?` — generic enums in library, not compiler builtins

**Reference fixtures today:** see [tests/cli/README.md](../tests/cli/README.md). **`run.sh`:** 31 programs + `modules/main.phx`. **Acceptance project:** `mvp_acceptance/` via `build.sh`.

---

## Status legend


| Status      | Meaning                                                |
| ----------- | ------------------------------------------------------ |
| **done**    | Implemented and covered by tests or CLI fixtures       |
| **partial** | Some pipeline stage works; gaps block demo correctness |
| **missing** | Not implemented or explicitly rejected                 |


---

## Pipeline infrastructure


| Item                                                                             | Status  | Where                                                  | Notes                                                                | Acceptance                                               |
| -------------------------------------------------------------------------------- | ------- | ------------------------------------------------------ | -------------------------------------------------------------------- | -------------------------------------------------------- |
| Workspace crates (`phx-syntax`, `phx-compiler`, `phx-bytecode`, `phx-vm`, `phx`) | done    | `source/Cargo.toml`                                    | Std-only deps                                                        | `cargo build` succeeds                                   |
| Compile driver: parse → resolve → typeck                                         | done    | `source/phx-compiler/src/compile.rs`                   | `compile_source`, `check_file`                                       | `compile_source("main :: () => { };", None)` ok          |
| Lower → IR                                                                       | done    | `source/phx-compiler/src/lower/`                       | CFG blocks, `IrInst`                                                 | `lower_sample_produces_ir` test                          |
| IR → PHX0 codegen                                                                | done    | `source/phx-compiler/src/codegen/`                     | `codegen`, `emit.rs`                                                 | `codegen_sample_round_trip_and_verify`                   |
| PHX0 encode/decode                                                               | done    | `source/phx-bytecode/src/module.rs`                    | Magic `PHX0`, **5** sections (incl. local layouts), minor v1           | Round-trip test in `codegen.rs`                          |
| Bytecode verifier                                                                | done    | `source/phx-bytecode/src/verify.rs`                    | All 43 opcodes: operands, jumps, stack depth, locals               | Negative tests: jump, local, stack underflow             |
| VM interpret verified module                                                     | done    | `source/phx-vm/src/interpreter.rs`                     | 43 opcodes; width-faithful scalars + arena aggregates + slices       | `tests/cli/run.sh` (31 + modules) |
| Span-preserving AST                                                              | done    | `source/phx-syntax/src/ast/node.rs`, `phx-diagnostics` | Spans on nodes/tokens                                                | Errors include `Span` fields                             |
| Interned identifiers                                                             | done    | `source/phx-syntax/src/intern.rs`                      | `Symbol` in AST                                                      | No raw `String` names in AST                             |
| Source-backed diagnostics in CLI                                                 | done    | `source/phx-diagnostics/src/format.rs`                 | Line + caret for parse/type errors via `CompileError::format_with_source` | `phx check bad_type.phx` shows caret |
| `phx compile` / write `.phx0` to disk                                            | done    | `source/phx/src/main.rs`                               | `phx compile -o` after verify                                        | `tests/cli/compile.sh` |
| Width-faithful primitives + slices/refs/byte strings                             | done    | `phx-bytecode`, `phx-vm`, `phx-compiler`               | See runtime snapshot above                                           | `primitives_width.phx`, `slice_from_array.phx`, etc. |


---

## Lexer & parser (`phx-syntax`)


| Item                                                       | Status           | Where                                            | Notes                                  | Acceptance                                      |
| ---------------------------------------------------------- | ---------------- | ------------------------------------------------ | -------------------------------------- | ----------------------------------------------- |
| EBNF-aligned token set                                     | done             | `source/phx-syntax/src/token.rs`, `lexer.rs`     | Keywords, literals, `#`/`@` directives | `source/phx-syntax/tests/lexer.rs` (~100 cases) |
| Program + top-level decls                                  | done             | `source/phx-syntax/src/parser/mod.rs`, `decl.rs` | struct/enum/trait/impl/fn/const/var    | Parser tests pass                               |
| Expressions, blocks, statements                            | done             | `parser/expr.rs`, `stmt.rs`                      | Includes `if`/`match`/`given` syntax   | Parser tests                                    |
| Types (primitives, tuples, arrays, slices, refs, fn types) | done             | `parser/types.rs`                                |                                        | Parse `&T`, `[T; N]`, `(T, U)`                  |
| Patterns (match arms)                                      | done             | `parser/pat.rs`                                  |                                        | Parse struct/tuple/enum patterns                |
| `#import` syntax                                           | done             | `parser/mod.rs`                                  | Parsed into `program.imports`          | Parse `#import std::foo::Bar`                   |
| **Parse-only / reject at parse** (`grammar-deferred.md`)   |                  |                                                  |                                        |                                                 |
| `#derive(...)`                                             | done (parse N/A) | `parser`                                         | Rejected as unsupported syntax         | `unsupported_hash_derive` test                  |
| `@spawn` / `@send` / `@receive` / `@reply`                 | done (reject)    | `parser/expr.rs`                                 |                                        | `unsupported_at_spawn` etc.                     |
| `for x in y`                                               | done (reject)    | `parser/stmt.rs`                                 |                                        | `unsupported_for_in` test                       |
| `0..n` / `0..=n`                                           | done (reject)    | `parser/expr.rs`                                 |                                        | Range tests                                     |
| Lambda `(…) => …`                                          | done (reject)    | `parser/expr.rs`                                 |                                        | Lambda tests                                    |
| Trait default bodies in trait decl                         | partial          | `parser/decl.rs`                                 | Parsed; no default body codegen        | Parse trait with method body (AST exists)       |


---

## Resolver (`phx-compiler/resolver`)


| Item                                | Status  | Where                           | Notes                       | Acceptance                                            |
| ----------------------------------- | ------- | ------------------------------- | --------------------------- | ----------------------------------------------------- |
| Single-file scopes + `DefId`        | done    | `resolver/scopes.rs`, `walk.rs` |                             | `resolve.rs` tests                                    |
| Top-level fn/const/var/type defs    | done    | `resolver/walk.rs`              |                             | Duplicate/unresolved tests                            |
| Block scopes, resolve exprs/pats    | done    | `resolver/walk.rs`              |                             |                                                       |
| Enforce `main` present              | done    | `resolver/walk.rs`              | `ResolveError::MissingMain` | `missing_main` fixture fails `phx check`              |
| Enforce `main :: () => …` signature | done    | `resolver/walk.rs`              | `InvalidMainSignature`      | resolve tests                                         |
| `#import`                           | done    | `modules/loader.rs`, `resolve_loaded_program.rs` | Bare `compile_source` / no `--module-src` → `ImportNotSupported` with module-root hint | `tests/cli/fixtures/modules/` + `run_modules.rs` |
| `pub` / cross-module visibility     | done    | `resolver/walk.rs`, `resolve_loaded_program.rs` | Export map + import preface | Private import fails; `pub` export works across files |


---

## Type checker (`phx-compiler/typeck`)


| Item                                       | Status   | Where                             | Notes                                                                         | Acceptance                                             |
| ------------------------------------------ | -------- | --------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------ |
| Literal defaults (`s32`, `u`→`u32`, `f32`) | done     | `typeck/builtins.rs`              |                                                                               | `const_inference_ok`                                   |
| No implicit numeric widening               | done     | `typeck/ops.rs`                   | Casts explicit only                                                           | Mismatch without `as`                                  |
| Explicit cast `expr as Type`               | done     | `typeck/ops.rs`, lower, VM `Cast`/`MakeStr`/`StrAsSlice` | Numeric cross-cast; array→slice; str↔bytes (compile-time UTF-8) | `cast_width.phx`, `string_literal.phx`, `byte_string_as_str.phx` |
| Function params explicit; default ret `()` | done     | `typeck/check.rs`                 |                                                                               | Tests                                                  |
| `if` branch unification                    | done     | `typeck/unify.rs`                 |                                                                               | `if_branch_mismatch`                                   |
| `while` / `loop` / `break` / `continue`    | done     | `typeck/check.rs`                 | `loop_depth`                                                                  | typeck tests + `control_flow.phx`                      |
| Calls, arity, return types                 | done     | `typeck/check.rs`                 |                                                                               | `call_*` tests                                         |
| `const` / `var` inference & assign         | done     | `typeck/check.rs`                 |                                                                               | assign tests                                           |
| Index `[T; N]` / slice                     | done     | `typeck/check.rs` + VM `Index`    | Array and slice index at runtime                                              | `array_index.phx`, `slice_from_array.phx`              |
| Struct literals + fields                   | done     | `typeck/check.rs`, `layout.rs`    | Missing/unknown field errors; layout tables                                   | Struct lit + field read in `struct_point.phx`          |
| Std ctors `Some`/`None`/`Ok`/`Err`         | deferred | —                                 | Lex as `TypeIdent`; resolve as unknown type until std prelude                 | `ok_ctor_unresolved_until_std`                         |
| `Option`/`Result` types                    | deferred | —                                 | Lex as `TypeIdent` + generics; no compiler builtin                            | `result_type_unresolved_until_std`                     |
| `?`                                        | deferred | `typeck/check.rs`                 | Postfix `?` rejected until std                                                | `question_mark_unsupported_in_mvp`                     |
| `match` expr arm unification               | done     | `typeck/check.rs`                 |                                                                               | Arm type unify                                         |
| `match` / `given` pattern checking         | done     | `typeck/check.rs`                 | Struct/tuple/unit enum patterns; enum exhaustiveness                      | `enum_match.phx`, `enum_match_non_exhaustive` test     |
| `&&` / `||` on `bool`                      | done     | `typeck/ops.rs`                   |                                                                               | Typeck accepts                                         |
| `%` `**` bitwise shifts                    | done     | `typeck/ops.rs`, lower, VM        |                                                                               | `mod_bitwise.phx`                                      |
| Method calls `x.foo()`                     | done     | `typeck/check.rs`, `lower/expr.rs` | Inherent + trait impl dispatch                                          | `struct_method.phx`, `trait_eq.phx`                    |
| Trait / impl static resolution             | done     | `typeck/check.rs`                  | `Type :: impl :: Trait`; ambiguous impls diagnosed                      | `trait_eq.phx`                                         |
| Borrow `&T` / `&mut T` in types            | partial  | `typeck/ops.rs`, `lower/expr.rs`  | Address-of locals + deref via `PtrLoad`; no borrow checker                    | `ref_local.phx`, `deref_ptr.phx`                       |
| Raw pointers `*T`                          | partial  | `typeck/ops.rs`, VM `PtrLoad`/`PtrStore` | Deref on primitives; full pointer surface TBD                          | `deref_ptr.phx`                                        |
| Generics on types                          | pass     | `typeck/mono.rs`, `typeck/check.rs` | V0-020/V0-021: explicit + inferred instantiation; dual-site mono in IR/bytecode; V0-022 generic enum match; V0-023 assoc types + `Self::Item` | `generic_fn.phx`, `generic_enum_infer.phx`, `generic_enum_match.phx`, `typeck.rs` dual-inst tests |
| Copyable inference                         | partial  | `typeck/builtins.rs`              | Primitives, tuples, arrays of Copyable                                        | User struct Copyable only when all fields Copyable     |
| Use-after-move (MVP ownership)             | done     | `typeck/ownership.rs`, `check.rs` | Non-Copyable moves                                                            | `use_after_move_error`                                 |
| Per-function layout / locals               | done     | `typeck/bindings.rs`              | For lowering                                                                  | `main_layout_slot_count`                               |


---

## Lowering & IR (`phx-compiler/lower`, `ir`)


| Item                                          | Status  | Where                     | Notes                                                                              | Acceptance                               |
| --------------------------------------------- | ------- | ------------------------- | ---------------------------------------------------------------------------------- | ---------------------------------------- |
| Literals, locals, const pool                  | done    | `lower/expr.rs`, `ctx.rs` |                                                                                    | IR const/load/store                      |
| Binary `+ - * / == <` (+ `>` via swapped `<`) | done    | `lower/expr.rs`           | `IrBinOp`                                                                          | Add in `add()` IR                        |
| `&&` `||`                                     | done    | `lower/expr.rs`           | Short-circuit via `JumpIf` + `Const` 0/1                                           | `logical.phx` |
| `%` `**` bitwise                              | done    | `lower/expr.rs`, VM         |                                                                                    | `mod_bitwise.phx`                        |
| `if` / else-if chain                          | done    | `lower/expr.rs`           | `JumpIf`, merge block                                                              | `JumpIf` in sample IR                    |
| `while` / `loop` / `break` / `continue`       | done    | `lower/stmt.rs`           |                                                                                    | `lower_control_flow_emits_loops`         |
| `return`                                      | done    | `lower/stmt.rs`           |                                                                                    |                                          |
| Function calls                                | done    | `lower/expr.rs`           | `IrInst::Call`                                                                     | Call in sample IR                        |
| `match`                                       | done    | `lower/expr.rs`           | Primitives + struct/enum via `MatchTag` / `GetField`                               | `enum_match.phx`, `match_int.phx`        |
| `given`                                       | done    | `lower/stmt.rs`           | Pattern dispatch via `emit_arm_condition` + `TrapGivenMismatch` on fail            | `given_struct.phx`, `given_enum_single_variant.phx`, `given_enum_non_exhaustive` check  |
| `?`                                           | deferred  | `lower/expr.rs`           | `PostfixOp::Try => {}`; post-MVP std only                                          | After std: early-return lowering         |
| Struct / enum value construction              | done    | `lower/expr.rs`           | `MakeStruct`, `MakeEnum`, `GetField`, `SetField`                                  | `struct_point.phx`, `struct_assign.phx`  |
| Casts                                         | done    | `lower/expr.rs`           | `IrInst::Cast` with `from_kind`/`to_kind`                                          | `cast_width.phx`                         |
| Field access `x.f`                            | done    | `lower/expr.rs`           | `GetField` / `SetField`                                                           | Aggregate fixtures                       |


---

## Codegen & bytecode (`phx-compiler/codegen`, `phx-bytecode`)


| Item                                                                                                               | Status  | Where                            | Notes                                                    | Acceptance                   |
| ------------------------------------------------------------------------------------------------------------------ | ------- | -------------------------------- | -------------------------------------------------------- | ---------------------------- |
| MVP opcode set (20 opcodes)                                                                                        | partial | `phx-bytecode/src/opcode.rs`     | Includes aggregate opcodes 15–19                           | Documented subset stable     |
| `CONST` / locals / arithmetic / compare / jumps / `CALL` / `RETURN`                                                | done    | `opcode.rs`, `emit.rs`           |                                                          | Verified sample module       |
| `POP`, `MOD`, `NEG`, bitwise, `MAKE_*`, `GET_FIELD`, `INDEX`, `MATCH_*`, `MAKE_SOME/OK/…`, `TRY`, `ALLOC`, ptr ops | partial | `opcode.rs`, `emit.rs`, VM       | Aggregate `MAKE_*`/`GET_FIELD`/`SET_FIELD`/`MATCH_TAG` done | Aggregate fixtures verify    |
| Constants: width-native tags (1–16 byte ints, f32/f64, bool, blob)                                                 | done    | `const_pool.rs`, VM `load_const` | `prim_kind` on `CONST`                                   | `primitives_float.phx`, `byte_string.phx` |
| Types section metadata                                                                                             | done    | `codegen/mod.rs`, `types.rs`     | Struct/enum aux from typeck layout tables                | Types round-trip in module   |
| Symbols / debug section                                                                                            | missing | spec § symbols                   | Optional in MVP                                          | —                            |
| Stack depth / `stack_max`                                                                                          | done    | `emit.rs`, `stack_effect.rs`     |                                                          | Verifier `StackExceedsMax`   |
| Entry = zero-arity `main`                                                                                          | done    | `codegen/mod.rs`                 |                                                          | `entry_function_id`, arity 0 |


---

## VM (`phx-vm`)


| Item                        | Status  | Where                        | Notes                                 | Acceptance                                |
| --------------------------- | ------- | ---------------------------- | ------------------------------------- | ----------------------------------------- |
| Stack machine + call frames | done    | `frame.rs`, `interpreter.rs` |                                       | Nested `CALL` works                       |
| `Value` model               | done    | `frame.rs`                   | `Scalar` (width-faithful) + `Agg` arena; slice aggregate | 31 CLI run fixtures                       |
| Opcode interpreter          | done    | `interpreter.rs`             | 43 opcodes; `prim_kind` on scalar ops | Unsupported opcode → clean error          |
| Deterministic run           | done    | `interpreter.rs`             | No I/O                                | Same bytecode → same result               |
| Division by zero            | done    | `interpreter.rs`             | `VmError::DivisionByZero`             | Test / fixture                            |
| Scope-end drop / RAII       | missing | —                            | Post-import memory model (see roadmap) | Explicit deallocation at scope end        |


---

## Expressions & operators (end-to-end)


| Item                                   | Status  | Where               | Notes                              | Acceptance                |
| -------------------------------------- | ------- | ------------------- | ---------------------------------- | ------------------------- |
| Integer `+ - * /`                      | done    | typeck → lower → VM |                                    | `sample.phx`              |
| Comparisons `== <` (and `>` via lower) | done    | same                | `Eq`, `Lt` opcodes                 | `if sum > 0` in sample    |
| `==` `!=` `<` `<=` `>` `>=`            | done    | typeck → lower → VM | `Ne`/`Le`/`Ge` + swapped `<` for `>`                             | `compare_unary.phx`       |
| Logical `&&` `||`                      | done    | typeck → lower → VM       | Short-circuit branch lowering                                      | `logical.phx`             |
| Unary `-` / `!`                        | done    | typeck → lower → VM | `Neg`/`Not` opcodes                                                | `compare_unary.phx`       |
| `bool` literals                        | partial | typeck + VM         |                                    | `const ok: bool = …` runs |


---

## Control flow (end-to-end)


| Item                          | Status  | Where                   | Notes | Acceptance                     |
| ----------------------------- | ------- | ----------------------- | ----- | ------------------------------ |
| `if` expression               | done    | full pipeline           |       | sample.phx                     |
| `while`                       | done    | full pipeline           |       | `control_flow.phx` + `phx run` |
| `loop` / `break`              | done    | full pipeline           |       | `control_flow.phx`             |
| `continue`                    | done    | full pipeline              |       | `continue_in_if.phx`                   |
| `match` (primitives + enum/struct) | done    | full pipeline              |       | `match_int.phx`, `enum_match_struct.phx` |
| `return`                      | done    | stmt lower + VM         |       | `function_return_stmt_ok`      |
| `given`                       | done    | typeck + lower          |       | `given_struct.phx`, `given_enum_single_variant.phx`, exhaustiveness check |


---

## Types & data


| Item                              | Status  | Where                        | Notes                               | Acceptance                |
| --------------------------------- | ------- | ---------------------------- | ----------------------------------- | ------------------------- |
| Numeric primitives (all keywords) | done    | typeck + VM (`ScalarValue` width enum) | Widening disallowed; width-faithful stack/locals/consts | `primitives_width.phx`, `primitives_i128.phx` |
| `bool`                            | done    | typeck + VM                  | 1-byte `Bool` cell, not integer alias | `match_bool.phx`          |
| `()` unit                         | done    | typeck                       |                                     | `main :: () =>`           |
| Tuples                            | done    | parse + typeck + VM          | `MakeTuple`                         | `tuple_lit.phx`           |
| Fixed arrays `[T; N]`             | done    | typeck + VM                  | `MakeArray`, index; `b"…"` lowers to `[u8; N]` | `array_index.phx`, `byte_string.phx` |
| Slices `[T]`                      | partial | typeck + VM                  | Explicit cast from array; stack-backed only (no heap slice) | `slice_from_array.phx` |
| Type aliases                      | done    | resolver + typeck + unify  | Expand aliases in unify; assignability tests | `type_alias_*` in `typeck.rs` |
| `struct` decl + literal           | done    | parse, typeck, lower, VM     | Arena struct aggregates             | `struct_point.phx`        |
| `enum` decl + ctors               | done    | parse, typeck, lower, VM     | Tag + payload in arena              | `enum_match.phx`          |


---

## Functions


| Item                             | Status  | Where          | Notes                        | Acceptance                         |
| -------------------------------- | ------- | -------------- | ---------------------------- | ---------------------------------- |
| Top-level `name :: (…) => T { }` | done    | full pipeline  |                              | `add` in sample                    |
| Trailing expr return             | done    | typeck         |                              | `function_trailing_expr_return_ok` |
| `return expr;`                   | done    | lower + VM     |                              |                                    |
| Recursion                        | done    | VM `CALL`      | Direct self-call              | `factorial.phx`, `factorial_computes_one_twenty` |
| Methods / receiver               | done    | typeck + lower | Synthetic receiver param; inherent + trait dispatch | `struct_method.phx`, `trait_eq.phx` |


---

## Errors: `Option` / `Result` / `?` (post-MVP std)


| Item                                   | Status  | Where             | Notes                      | Acceptance                   |
| -------------------------------------- | ------- | ----------------- | -------------------------- | ---------------------------- |
| Std `Option`/`Result` as generic enums | missing | std crate         | Not compiler builtins      | `#import` + prelude          |
| Surface syntax (`Option<T>`, ctors)    | done    | `phx-syntax`      | Same as user `TypeIdent` / enum patterns — no reserved keywords | Parser tests                 |
| Unknown until std prelude              | done    | `resolver/walk.rs`| `UnresolvedType` for undefined names | `*_unresolved_until_std` tests |
| `?` lowering + runtime                 | missing | lower + VM        | After std types exist      | Early return propagates      |
| `match` on std enums                   | missing | typeck + lower    | After std + aggregates     | Runtime unwrap arms          |


---

## Ownership (MVP subset)


| Item                                  | Status  | Where                | Notes                                                     | Acceptance                        |
| ------------------------------------- | ------- | -------------------- | --------------------------------------------------------- | --------------------------------- |
| Move on assign/call for non-Copyable  | done    | typeck               | Structs non-Copyable                                      | move tests                        |
| Use-after-move diagnostic + move site | done     | `typeck/ownership.rs`, `format.rs` | Secondary `note:` at move span                                            | `use_after_move.phx` + `use_after_move_error`          |
| Copyable primitives / tuples / arrays | done    | `typeck/builtins.rs` |                                                           | `const b = a` for `s32`           |
| Full borrow checker                   | missing | —                    | Post-MVP per [ownership.md](design/features/ownership.md) | —                                 |


---

## Traits (`traits.md` MVP: parse + static method resolution)


| Item                                  | Status  | Where    | Notes                                                    | Acceptance                          |
| ------------------------------------- | ------- | -------- | -------------------------------------------------------- | ----------------------------------- |
| Parse `trait` / `impl` / `impl ::`    | done    | parser   | Rejects legacy `impl for`                                 | Parser tests                        |
| Resolve trait/impl names              | done    | resolver |                                                          |                                     |
| Type-check impl methods               | done    | typeck   | Members as functions                                     |                                     |
| Static method resolution to impl      | done    | typeck   | Lookup via `inherent_methods` / `trait_methods`          | `trait_eq.phx`                      |
| Inherent vs trait impl disambiguation | partial | typeck   | Ambiguous trait impls diagnosed; inherent wins first     | Multiple trait impls same method  |
| `#derive` codegen                     | missing | deferred |                                                          | —                                   |


---

## Modules / `#import` (`modules.md`)


| Item                               | Status  | Where    | Notes                   | Acceptance                      |
| ---------------------------------- | ------- | -------- | ----------------------- | ------------------------------- |
| Parse `#import`                    | done    | parser   |                         |                                 |
| Load multiple files / module paths | done    | `modules/` | `--module-path`, `c.phx` / `index.phx` | `tests/cli/fixtures/modules/` |
| `pub` exports                      | done    | `resolve_loaded_program.rs` | Per-module export map | `import_private.phx` fails check |
| Path `std::…` → file mapping       | done    | `modules/path.rs` | Under `module_root` | `util::math` → `util/math.phx` |


### M2 — `phoenix.toml`, `build/`, incremental (modules.md)


| Item | Status | Where | Notes | Acceptance |
|------|--------|-------|-------|------------|
| `phoenix.toml` project root | done | `project/config.rs` | Marker file required | `tests/cli/fixtures/project/` |
| `build/` artifact layout | done | `project/layout.rs` | `pxi/`, `phx0/`, `bin/`, `manifest.json` | `tests/cli/build.sh` |
| `.pxi` v1 interfaces | done | `pxi/format.rs` | JSON exports + source hash | `build/pxi/util/math.pxi` |
| PHX0 linker | done | `link/mod.rs` | Merge per-module objects | `build/bin/main.phx0` runs |
| Incremental manifest | done | `build/manifest.rs` | Skip unchanged modules | Re-`phx build` fast path |
| Import cycle + `.pxi` escape | done | `modules/graph.rs` | Fresh `.pxi` on all SCC nodes | Design in modules.md |
| `phx build` / project `phx run` | done | `build/driver.rs`, `phx` CLI | `--no-build`, `--build` | `tests/integration/run_build.rs` |
| Separate compile via `.pxi` | done | `build/driver.rs`, `pxi/format.rs`, `typeck/mono.rs` | Path-dep link uses prebuilt `build/deps/*/phx0`; `function_id` on concrete exports; V0-024 mangled generic fn exports + consumer worklist rebuild | `run_dep_build.rs`, `pxi.rs` |
| Std package layout (V0-040) | done | `std/`, `std/README.md` | Repo-root `type = lib` package; `build/lib/std.phx0`; path-dep workflow | `run_build.rs`, `run_dep_build.rs`, `cli_e2e` |


---

## CLI & tooling


| Item                             | Status  | Where                    | Notes                       | Acceptance           |
| -------------------------------- | ------- | ------------------------ | --------------------------- | -------------------- |
| `phx help`                       | done    | `source/phx/src/main.rs` |                             | `tests/cli/help.sh`  |
| `phx check <file>`               | done    | same                     | Parse+resolve+typeck        | `tests/cli/check.sh` |
| `phx build <file>`               | done    | `build/driver.rs`        | Requires `phoenix.toml`     | `tests/cli/build.sh` |
| `phx run <file>`                 | done    | project build or M1 path | `build/bin` when project    | `tests/cli/build.sh` |
| `phx compile -o`                 | done    | same                     | M1 path; optional project   | `tests/cli/compile.sh` |
| Pretty diagnostics (span labels) | done    | `phx-diagnostics/format.rs` | Line + caret; move-site notes | `check.sh` caret + `use_after_move` note |


---

## Tests & fixtures


| Item                                      | Status  | Where                                   | Notes                                               | Acceptance                                           |
| ----------------------------------------- | ------- | --------------------------------------- | --------------------------------------------------- | ---------------------------------------------------- |
| Lexer unit tests                          | done    | `source/phx-syntax/tests/lexer.rs`      | Large table                                         |                                                      |
| Parser unit tests                         | done    | `source/phx-syntax/tests/parser.rs`     | Includes deferred=unsupported                       |                                                      |
| Resolver / typeck / lower / codegen tests | done    | `source/phx-compiler/tests/`            |                                                     |                                                      |
| Verifier tests                            | done    | `source/phx-bytecode/src/verify.rs`     | `#[cfg(test)]`                                      |                                                      |
| Verifier mutation tests                   | done    | `source/phx-bytecode/tests/verify_mutation.rs` | Encode/decode/mutate; verify rejects; VM no panic | `cargo test -p phx-bytecode --test verify_mutation`  |
| CLI shell tests                           | done    | `tests/cli/*.sh`                        | check, run, build, compile, help                  | CI: `rust` job + `cli` job (all shell scripts)       |
| `just test-lang`                          | done    | `Justfile`                              | check + run + build + help                        | `just pre-commit`; `just test-cli` adds `compile.sh` |
| Integration `run_sample`                  | done    | `tests/integration/tests/run_sample.rs` |                                                     |                                                      |
| Integration control_flow                  | done    | `tests/integration/tests/run_control_flow.rs` | compile → verify → run | `cargo test -p phx-integration-tests --test run_control_flow` |
| Integration semantics                       | done    | `tests/integration/tests/run_semantics.rs` | `VmRunCapture::main_local` slot-index assertions | See table below (28 tests) |
| Diagnostic golden tests                   | done    | `tests/integration/tests/diagnostics.rs` | `.stderr` sidecars in `tests/integration/diagnostics/` | `UPDATE_GOLDEN=1` to refresh |
| Corpus of `.phx` programs                 | done    | `tests/cli/fixtures/`                   | 31 run + modules + project + `mvp_acceptance` + app_dep | [tests/cli/README.md](../tests/cli/README.md)        |
| MVP acceptance project                    | done    | `tests/cli/fixtures/mvp_acceptance/`    | Struct + enum `match` + `#import` + `phoenix.toml` build | `build.sh`, `run_build.rs`                           |
| Unreachable `match` arm errors            | done    | `typeck/check.rs`, `match_unreachable_arm.phx` | Duplicate variant/literal/`_` arms rejected          | `check.sh`                                             |
| Negative diagnostics fixtures             | done    | `check.sh`                              | `bad_type`, `missing_main`, `use_after_move`, `mixed_width`, module errors | Substring + caret assertions                         |

### `run_semantics.rs` value assertions

`cargo test -p phx-integration-tests --test run_semantics`

| Test | Fixture | Expected local |
|------|---------|----------------|
| `sample_arithmetic_computes_sum` | `sample.phx` | `s32` 12, `bool` true |
| `enum_match_extracts_payload` | `enum_match.phx` | `s32` 42 |
| `struct_method_sums_fields` | `struct_method.phx` | `s32` 7 |
| `struct_point_sums_via_function` | `struct_point.phx` | `s32` 7 |
| `trait_eq_method_returns_true` | `trait_eq.phx` | `bool` true |
| `control_flow_loop_counter_reaches_ten` | `control_flow.phx` | `s32` 10 |
| `cast_width_sum_is_one_forty_two` | `cast_width.phx` | `s64` 142 |
| `match_int_selects_arm_value` | `match_int.phx` | `s32` 20 |
| `logical_short_circuit_ok_is_true` | `logical.phx` | `bool` true |
| `slice_from_array_index_byte` | `slice_from_array.phx` | `u8` `'Y'` |
| `modules_import_adds_imported_values` | `modules/main.phx` | `s32` 3 |
| `mvp_acceptance_along_plus_pick_is_four` | `mvp_acceptance/` project | `s32` 4 |
| `factorial_computes_one_twenty` | `factorial.phx` | `s32` 120 |
| `given_enum_single_variant_binds_payload` | `given_enum_single_variant.phx` | slot 1: `s32` 12 |
| `ref_local_derefs_to_ten` | `ref_local.phx` | slot 2: `s32` 10 |
| `deref_ptr_reads_seventy_seven` | `deref_ptr.phx` | slot 2: `u8` 77 |
| `byte_string_index_is_capital_b` | `byte_string.phx` | slot 1: `u8` `'B'` |
| `primitives_width_sums_to_two_fifty_five` | `primitives_width.phx` | slot 3: `s64` 255 |
| `primitives_float_mixed_width_sum` | `primitives_float.phx` | slot 6: `f64` 145.75 |
| `primitives_i128_truncates_to_s8` | `primitives_i128.phx` | slot 1: `s8` -24 |
| `compare_unary_and_relations_score` | `compare_unary.phx` | slot 11: `s32` -4 |
| `mod_bitwise_ops_sum` | `mod_bitwise.phx` | slot 5: `s32` -7 |
| `array_index_reads_middle_element` | `array_index.phx` | slot 1: `s32` 20 |
| `tuple_lit_first_element` | `tuple_lit.phx` | slot 1: `s32` 1 |
| `match_bool_true_arm` | `match_bool.phx` | slot 2: `s32` 1 |
| `match_ident_wildcard_arm` | `match_ident.phx` | slot 2: `s32` 20 |
| `struct_assign_updates_field` | `struct_assign.phx` | slot 1: `s32` 5 |
| `enum_match_struct_extracts_payload` | `enum_match_struct.phx` | slot 2: `s32` 42 |

`run_control_flow.rs` only smoke-runs `control_flow.phx` (overlap with row above); see [tests/integration/README.md](../tests/integration/README.md).

---

## Explicitly out of scope (do not implement for MVP)

Per [mvp.md](design/mvp.md) and [grammar-deferred.md](design/features/grammar-deferred.md):

- M:N scheduler, actors, mailboxes, supervision (`@spawn`, `@send`, …)
- Std I/O, networking, collections, formatting APIs
- Primitive owned `string` type (std owns growable `String` over bytes)
- Full borrow checker / lifetimes / exclusivity of `&mut`
- JIT, hot reload, `#derive` / `@derive` semantic codegen
- `for` loops, range literals, closures/lambdas (parse rejected today)
- Package manager / build system beyond `phx` CLI
- Post-MVP: runtime transparency, schedulable I/O ([runtime-transparency.md](design/features/runtime-transparency.md))
- Post-MVP: std `Option` / `Result` / `?` ([type-system.md](design/features/type-system.md#phased-option-and-result-language--std))

---

## Suggested implementation order

Aligned with `.cursor/rules/phoenix.mdc` (lexer → parser → AST → resolver → typeck → IR → codegen → verifier → VM → tests):

### Phase 1 — Credible demo (done)

1. ~~Fix `continue` in `if`~~ — done (loop exit placement + `var` assign `StoreLocal`).
2. ~~Logical ops~~ — done (`&&` / `||` short-circuit).
3. ~~Match lowering (primitives)~~ — done for literal / `_` / ident; enum/struct patterns Phase 2.
4. ~~E2E fixtures~~ — `run.sh` + integration tests.

### Phase 2 — User-defined aggregates (done)

1. ~~**VM value model:**~~ `Value::Scalar` / `Value::Agg` arena handles (freed at run end; not GC).
2. ~~`MAKE_STRUCT` / `MAKE_ENUM` / `GET_FIELD` / `SET_FIELD` / `MATCH_TAG`~~ (+ verifier stack rules).
3. ~~**Pattern typeck:**~~ struct/tuple/unit enum patterns against scrutinee type.
4. ~~**Method resolution:**~~ inherent impl `receiver.method` → `Call` with synthetic receiver param.

Fixtures: `struct_point.phx`, `struct_assign.phx`, `enum_match.phx`, `struct_method.phx`; integration `run_aggregates.rs`.

### Phase 3 — Bytecode completeness & tooling (done)

1. ~~**Remaining MVP opcodes:**~~ `Cast`, `Mod`, `Pow`, `Neg`, `Not`, bitwise, `Ne`/`Le`/`Ge`, `MakeTuple`, `MakeArray`, `Index`, `Trap`, ptr ops, `MakeSlice`, `AddressOfLocal`.
2. ~~**Float paths:**~~ `F32`/`F64` width-faithful scalars (`primitives_float.phx`).
3. ~~**Width-faithful integers:**~~ `s8`…`s128` / `u8`…`u128` (`primitives_width.phx`, `primitives_i128.phx`).
4. ~~**`phx compile -o`**~~ — `tests/cli/compile.sh`.
5. ~~**Source diagnostics**~~ — caret rendering in `phx check` / `phx run` (`tests/cli/check.sh`).
6. ~~**Language surface:**~~ explicit casts, tuple/array/slice runtime, `given`, trait impl dispatch, `b"…"`, frame refs.

Fixtures: see [Demo bar](#demo-bar-minimum-showcase-program); `run.sh` runs **31** programs + modules.

### Phase 4 — Polish (mostly done)

1. ~~Parser ergonomics (unclosed `(`)~~ — bounded fix in `parser/expr.rs`; broader grammar ambiguities may remain.
2. ~~Checklist/table hygiene and negative fixtures~~ — `mixed_width.phx`, fixture inventory in `tests/cli/README.md`.
3. ~~Use-after-move diagnostics cite source span at move site~~ — `format.rs` + `use_after_move.phx`.
4. ~~Enum struct-variant `match` arms + exhaustiveness~~ — `enum_match_struct.phx`, `NonExhaustiveMatch`.
5. ~~Type aliases~~ — `type_alias_*` tests.
6. ~~Trait `Type :: impl :: Trait` dispatch~~ — `trait_eq.phx`.
7. ~~Semantic integration tests~~ — `tests/integration/tests/run_semantics.rs` (value assertions, not just exit 0).

### Phase 5 — Modules (`#import`) (done)

1. ~~**Module graph (M1):**~~ load multiple files, `::` paths, `pub` visibility ([modules.md](design/features/modules.md)).
2. ~~**M2 build pipeline:**~~ `phoenix.toml`, `build/`, `.pxi`, linker, incremental manifest.
3. ~~**Std package layout (V0-040):**~~ repo-root `std/` lib package, path-dep workflow — see `std/README.md`.
4. **Std + prelude (minimal):** `Option`/`Result` as generic enums in library (V0-041+); then `?` — not compiler builtins.

### Phase 6 — Memory model & lifetimes (after modules; before scheduler/std I/O)

**Goal:** No GC — explicit rules for allocation, scope, and deallocation tied to the type system ([ownership.md](design/features/ownership.md)).

| Topic | Direction (design TBD) |
|-------|-------------------------|
| Stack vs heap | Most locals/aggregates in frame + arena today; heap via intrinsics/`Alloc` only internally until surface syntax is designed |
| Scope end | Block `{ … }` should imply drop of owned non-`Copyable` values (Rust-like); compiler inserts drop/dealloc opcodes at scope exit |
| Heap ownership | Likely explicit owning types (e.g. box/alloc handle) rather than implicit GC; syntax and `Copyable`/`Clone` interaction TBD |
| Borrow checker | Full `&` / `&mut` exclusivity and lifetimes — builds on address-of + `PtrLoad` already in MVP |
| Strings | Core **`str`** UTF-8 view; binary via `[u8]` / `b"…"`; std provides owned **`String`** |

**Deferred (post memory model + MVP acceptance):** M:N scheduler, actors (`@spawn`), mailboxes, std I/O, networking, `#derive` codegen, JIT.

---

## Roadmap beyond single-file MVP

Product order agreed for post-demo work (not all MVP-blocking):

1. **Single-file polish** — parser/diagnostic gaps above.
2. **`#import` + `pub`** — real modules; then minimal std prelude.
3. **Memory model** — scoped drop, heap ownership surface, borrow rules documented and enforced in typeck + codegen (no GC).
4. **Std library breadth** — string struct, collections, I/O (schedulable-I/O types when runtime exists).
5. **Runtime** — scheduler, actors, supervision ([runtime-transparency.md](design/features/runtime-transparency.md), [concurrency.md](design/features/concurrency.md)).

Do not implement scheduler/actors/std I/O until the memory model and module story are credible.

---

## Quick reference: pipeline stage map

```text
.phx source
  → phx-syntax (lex/parse)     [strong]
  → phx-compiler/resolver      [single-file or multi-file via --module-src / project build]
  → phx-compiler/typeck        [MVP rules + aggregate layouts/patterns/exhaustiveness]
  → phx-compiler/lower → ir    [control flow + struct/enum aggregates]
  → phx-compiler/codegen       [PHX0 + types section metadata]
  → phx-bytecode/verify        [scalar + aggregate opcode rules; local layouts]
  → phx-vm/interpreter         [width-faithful scalars + arena + slices; 43 opcodes]
```

---

## Checklist totals (implementation items in tables above)


| Status      | Count |
| ----------- | ----- |
| **done**    | ~82   |
| **partial** | ~35   |
| **missing** | ~18   |
| **deferred** (post-MVP std) | 4 |
| **Total**   | ~139  |


*(Counts are table rows in this file, not git issues. Includes `done (reject)` parser rows. **deferred** = intentionally not MVP.)*

---

## Related documents


| Document                                                   | Use                          |
| ---------------------------------------------------------- | ---------------------------- |
| [mvp.md](design/mvp.md)                                    | In/out of MVP                |
| [grammar.ebnf](design/grammar.ebnf)                        | Syntax                       |
| [grammar-deferred.md](design/features/grammar-deferred.md) | Parse-only vs must-implement |
| [vm-linear.md](design/features/vm-linear.md)               | PHX0 + opcode families       |
| [type-system.md](design/features/type-system.md)           | Typing rules                 |
| [error-handling.md](design/features/error-handling.md)     | `Result` / `?`               |
| [ownership.md](design/features/ownership.md)               | Moves, Copyable              |
| [traits.md](design/features/traits.md)                     | Traits / impl                |
| [modules.md](design/features/modules.md)                   | `#import`                    |


