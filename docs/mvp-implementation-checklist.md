# Phoenix MVP implementation checklist

**Purpose:** Single reference for humans and coding agents: what the [MVP spec](design/mvp.md) requires, what is already implemented under `source/`, and what remains for a **credible demo** (working control flow, arithmetic, functions, types — not post-MVP runtime).

**How to use with agents:** Attach this file to prompts. Work top-down in [Suggested implementation order](#suggested-implementation-order). For each row, read **Status**, implement in **Where** until **Acceptance** passes. Do not invent semantics — [design docs](design/README.md) are authoritative.

**Last surveyed:** working tree on `trunk` (last commit `1ea58a0`; **uncommitted** changes include control flow, Option/Result policy, this checklist). **`cargo test --workspace`:** 188 tests passing.

### Landed since `1ea58a0` (verify in working tree, not necessarily committed)

| Change | Evidence |
|--------|----------|
| Control flow E2E + `continue` in `if` | `control_flow.phx`, `continue_in_if.phx`, deferred loop exit blocks, `var` assign `StoreLocal` |
| Short-circuit `&&` / `\|\|` | `lower_short_circuit_bool`, `logical.phx` |
| `match` on literals / wildcard | `match_int.phx`, `match_bool.phx`, `run_match.rs` |
| Option/Result not MVP | No lexer keywords; parse as `TypeIdent`; resolve fails until std prelude |
| CLI `run.sh` | Runs 10 fixtures including struct/enum aggregates |

### Known gaps (do not assume done)

| Issue | Impact |
|-------|--------|
| `i < n` / `c \|\| d {` before `{` | Parser ambiguity; parenthesize (`n > (i)`, `(c \|\| d)`) |
| `match ident {` scrutinee | Use `match (ident) {` not `match ident {` (struct-literal parse) |
| `given pat = e { … }` before `{` body | Use `given pat = (e) { … }` when scrutinee is followed by `{` (struct-literal parse) |
| Enum/struct `match` patterns | done for struct/tuple/unit enum arms; enum struct variants deferred |
| Float VM | Int-only scalar model; `f32`/`f64` literals type-check but no float arithmetic opcodes yet |
| VM value model | `Value::Scalar` / `Value::Agg` (struct/enum/tuple/array); 38 opcodes (0–37) |

---

## Demo bar (minimum showcase program)

A credible MVP demo `.phx` should be able to:

- [x] Define `main :: () => { … }` and fail compile without it
- [x] Declare top-level functions and call them with typed parameters
- [x] Use `const` / `var`, assignment, and `s32` arithmetic (`+`, `-`, `*`, `/`, comparisons)
- [x] Use `if` / `else` as expressions with unified branch types
- [x] Use `while`, `loop`, `break`, `continue`, `return`
- [x] Use `match` on `s32` / `bool` literals and `_` (with `match (expr)` syntax)
- [x] Use short-circuit `&&` / `||` on `bool`
- [x] Construct and use **user** `struct` / `enum` values with field/tag access at runtime
- [x] Explicit `expr as Type` casts where types differ (per [type-system.md](design/features/type-system.md))
- [x] Run via `phx run file.phx` after bytecode verify (no panic on valid programs)
- [ ] *(Post-MVP std)* `Option` / `Result` / `?` — generic enums in library, not compiler builtins

**Reference fixtures today:** `sample.phx`, `control_flow.phx`, `continue_in_if.phx`, `logical.phx`, `match_int.phx`, `match_bool.phx`, `struct_point.phx`, `struct_assign.phx`, `enum_match.phx`, `struct_method.phx`, `cast_width.phx`, `mod_bitwise.phx`, `array_index.phx`, `tuple_lit.phx`, `given_struct.phx`, `trait_eq.phx`.

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
| PHX0 encode/decode                                                               | done    | `source/phx-bytecode/src/module.rs`                    | Magic `PHX0`, 4 sections                                             | Round-trip test in `codegen.rs`                          |
| Bytecode verifier                                                                | partial | `source/phx-bytecode/src/verify.rs`                    | Jump targets, stack depth, locals — for **implemented** opcodes only | `verify(&module)` on `sample.phx` output                 |
| VM interpret verified module                                                     | partial | `source/phx-vm/src/interpreter.rs`                     | 38 opcodes (0–37); int-only scalars + tuple/array/struct/enum aggs   | `tests/cli/run.sh` (16 fixtures) |
| Span-preserving AST                                                              | done    | `source/phx-syntax/src/ast/node.rs`, `phx-diagnostics` | Spans on nodes/tokens                                                | Errors include `Span` fields                             |
| Interned identifiers                                                             | done    | `source/phx-syntax/src/intern.rs`                      | `Symbol` in AST                                                      | No raw `String` names in AST                             |
| Source-backed diagnostics in CLI                                                 | done    | `source/phx-diagnostics/src/format.rs`                 | Line + caret for parse/type errors via `CompileError::format_with_source` | `phx check bad_type.phx` shows caret |
| `phx compile` / write `.phx0` to disk                                            | done    | `source/phx/src/main.rs`                               | `phx compile -o` after verify                                        | `tests/cli/compile.sh` |


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
| `#import`                           | missing | `resolver/walk.rs`              | Always `ImportNotSupported` | `#import` loads second file and resolves symbols      |
| `pub` / cross-module visibility     | missing | —                               | No module graph             | Private import fails; `pub` export works across files |


---

## Type checker (`phx-compiler/typeck`)


| Item                                       | Status   | Where                             | Notes                                                                         | Acceptance                                             |
| ------------------------------------------ | -------- | --------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------------------------ |
| Literal defaults (`s32`, `u`→`u32`, `f32`) | done     | `typeck/builtins.rs`              |                                                                               | `const_inference_ok`                                   |
| No implicit numeric widening               | done     | `typeck/ops.rs`                   | Casts explicit only                                                           | Mismatch without `as`                                  |
| Explicit cast `expr as Type`               | partial  | `typeck/ops.rs`                   | MVP: **same primitive keyword only** (`primitive_cast_allowed`)               | `1 as s64` allowed when designed; today same-kind only |
| Function params explicit; default ret `()` | done     | `typeck/check.rs`                 |                                                                               | Tests                                                  |
| `if` branch unification                    | done     | `typeck/unify.rs`                 |                                                                               | `if_branch_mismatch`                                   |
| `while` / `loop` / `break` / `continue`    | done     | `typeck/check.rs`                 | `loop_depth`                                                                  | typeck tests + `control_flow.phx`                      |
| Calls, arity, return types                 | done     | `typeck/check.rs`                 |                                                                               | `call_*` tests                                         |
| `const` / `var` inference & assign         | done     | `typeck/check.rs`                 |                                                                               | assign tests                                           |
| Index `[T; N]` / slice                     | partial  | `typeck/check.rs`                 | Typing only; no runtime slice value                                           | `index_array_ok`                                       |
| Struct literals + fields                   | done     | `typeck/check.rs`, `layout.rs`    | Missing/unknown field errors; layout tables                                   | Struct lit + field read in `struct_point.phx`          |
| Std ctors `Some`/`None`/`Ok`/`Err`         | deferred | —                                 | Lex as `TypeIdent`; resolve as unknown type until std prelude                 | `ok_ctor_unresolved_until_std`                         |
| `Option`/`Result` types                    | deferred | —                                 | Lex as `TypeIdent` + generics; no compiler builtin                            | `result_type_unresolved_until_std`                     |
| `?`                                        | deferred | `typeck/check.rs`                 | Postfix `?` rejected until std                                                | `question_mark_unsupported_in_mvp`                     |
| `match` expr arm unification               | done     | `typeck/check.rs`                 |                                                                               | Arm type unify                                         |
| `match` / `given` pattern checking         | done     | `typeck/check.rs`                 | Struct/tuple/unit enum patterns                                               | `enum_match.phx`                                       |
| `&&` / `||` on `bool`                      | done     | `typeck/ops.rs`                   |                                                                               | Typeck accepts                                         |
| `%` `**` bitwise shifts                    | partial  | `typeck/ops.rs`                   | Typed on numerics; **no codegen**                                             | Runtime test when VM supports                          |
| Method calls `x.foo()`                     | partial  | `typeck/check.rs`, `lower/expr.rs` | Inherent impl dispatch; synthetic receiver param; `self.` in impl body parse gap | `struct_method.phx`                                    |
| Trait / impl static resolution             | missing  | —                                 | Impl bodies type-checked; no trait constraint dispatch                        | `Point :: impl for Eq` call resolves to impl           |
| Borrow `&T` / `&mut T` in types            | partial  | `typeck/lower_ty.rs`              | In type AST; no borrow checker                                                | Signatures parse+type; exclusivity post-MVP            |
| Raw pointers `*T`                          | partial  | `typeck/lower_ty.rs`              | Types only                                                                    | No VM `PTR_LOAD`                                       |
| Generics on types                          | partial  | `typeck/lower_ty.rs`              | Named types + args scaffold                                                   | User generic fn typeck                                 |
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
| `%` `**` bitwise                              | missing | `lower/expr.rs`           | Operands lowered, no op                                                            | VM tests                                 |
| `if` / else-if chain                          | done    | `lower/expr.rs`           | `JumpIf`, merge block                                                              | `JumpIf` in sample IR                    |
| `while` / `loop` / `break` / `continue`       | done    | `lower/stmt.rs`           |                                                                                    | `lower_control_flow_emits_loops`         |
| `return`                                      | done    | `lower/stmt.rs`           |                                                                                    |                                          |
| Function calls                                | done    | `lower/expr.rs`           | `IrInst::Call`                                                                     | Call in sample IR                        |
| `match`                                       | done    | `lower/expr.rs`           | Primitives + struct/enum via `MatchTag` / `GetField`                               | `enum_match.phx`, `match_int.phx`        |
| `given`                                       | partial | `lower/stmt.rs`           | Scrutinee + body; no pattern dispatch                                              | Runtime `given` test                     |
| `?`                                           | deferred  | `lower/expr.rs`           | `PostfixOp::Try => {}`; post-MVP std only                                          | After std: early-return lowering         |
| Struct / enum value construction              | done    | `lower/expr.rs`           | `MakeStruct`, `MakeEnum`, `GetField`, `SetField`                                  | `struct_point.phx`, `struct_assign.phx`  |
| Casts                                         | partial | `lower/expr.rs`           | Value passed through unchanged                                                     | Cast changes representation when needed  |
| Field access `x.f`                            | done    | `lower/expr.rs`           | `GetField` / `SetField`                                                           | Aggregate fixtures                       |


---

## Codegen & bytecode (`phx-compiler/codegen`, `phx-bytecode`)


| Item                                                                                                               | Status  | Where                            | Notes                                                    | Acceptance                   |
| ------------------------------------------------------------------------------------------------------------------ | ------- | -------------------------------- | -------------------------------------------------------- | ---------------------------- |
| MVP opcode set (20 opcodes)                                                                                        | partial | `phx-bytecode/src/opcode.rs`     | Includes aggregate opcodes 15–19                           | Documented subset stable     |
| `CONST` / locals / arithmetic / compare / jumps / `CALL` / `RETURN`                                                | done    | `opcode.rs`, `emit.rs`           |                                                          | Verified sample module       |
| `POP`, `MOD`, `NEG`, bitwise, `MAKE_*`, `GET_FIELD`, `INDEX`, `MATCH_*`, `MAKE_SOME/OK/…`, `TRY`, `ALLOC`, ptr ops | partial | `opcode.rs`, `emit.rs`, VM       | Aggregate `MAKE_*`/`GET_FIELD`/`SET_FIELD`/`MATCH_TAG` done | Aggregate fixtures verify    |
| Constants: `s64` / `u64` / `bool` tags                                                                             | partial | `const_pool.rs`, VM `load_const` | Float/blob tags exist in spec; VM rejects most           | `f32` literal runs           |
| Types section metadata                                                                                             | done    | `codegen/mod.rs`, `types.rs`     | Struct/enum aux from typeck layout tables                | Types round-trip in module   |
| Symbols / debug section                                                                                            | missing | spec § symbols                   | Optional in MVP                                          | —                            |
| Stack depth / `stack_max`                                                                                          | done    | `emit.rs`, `stack_effect.rs`     |                                                          | Verifier `StackExceedsMax`   |
| Entry = zero-arity `main`                                                                                          | done    | `codegen/mod.rs`                 |                                                          | `entry_function_id`, arity 0 |


---

## VM (`phx-vm`)


| Item                        | Status  | Where                        | Notes                                 | Acceptance                                |
| --------------------------- | ------- | ---------------------------- | ------------------------------------- | ----------------------------------------- |
| Stack machine + call frames | done    | `frame.rs`, `interpreter.rs` |                                       | Nested `CALL` works                       |
| `Value` model               | partial | `frame.rs`                   | **All stack/locals are `s64` scalars**; `bool` as 0/1 | Struct/enum need tagged values or handles |
| Opcode interpreter          | partial | `interpreter.rs`             | Matches MVP opcode enum only          | Unsupported opcode → clean error          |
| Deterministic run           | done    | `interpreter.rs`             | No I/O                                | Same bytecode → same result               |
| Division by zero            | done    | `interpreter.rs`             | `VmError::DivisionByZero`             | Test / fixture                            |


---

## Expressions & operators (end-to-end)


| Item                                   | Status  | Where               | Notes                              | Acceptance                |
| -------------------------------------- | ------- | ------------------- | ---------------------------------- | ------------------------- |
| Integer `+ - * /`                      | done    | typeck → lower → VM |                                    | `sample.phx`              |
| Comparisons `== <` (and `>` via lower) | done    | same                | `Eq`, `Lt` opcodes                 | `if sum > 0` in sample    |
| `==` chained with bool                 | partial | lower               | `Ne`/`Le`/… partial in lower       | Full comparison set in VM |
| Logical `&&` `||`                      | done    | typeck → lower → VM       | Short-circuit branch lowering                                      | `logical.phx`             |
| Unary `-` / `!`                        | partial | typeck              | Lowering drops unary in some paths | Tests                     |
| `bool` literals                        | partial | typeck + VM         |                                    | `const ok: bool = …` runs |


---

## Control flow (end-to-end)


| Item                          | Status  | Where                   | Notes | Acceptance                     |
| ----------------------------- | ------- | ----------------------- | ----- | ------------------------------ |
| `if` expression               | done    | full pipeline           |       | sample.phx                     |
| `while`                       | done    | full pipeline           |       | `control_flow.phx` + `phx run` |
| `loop` / `break`              | done    | full pipeline           |       | `control_flow.phx`             |
| `continue`                    | done    | full pipeline              |       | `continue_in_if.phx`                   |
| `match` (primitive arms)      | partial | full pipeline              |       | `match_int.phx`, `match_bool.phx`      |
| `return`                      | done    | stmt lower + VM         |       | `function_return_stmt_ok`      |
| `match`                       | partial | typeck done; lower stub |       | Runtime selects arm            |
| `given`                       | partial | typeck; lower stub      |       | Runtime `given`                |


---

## Types & data


| Item                              | Status  | Where                        | Notes                               | Acceptance                |
| --------------------------------- | ------- | ---------------------------- | ----------------------------------- | ------------------------- |
| Numeric primitives (all keywords) | partial | typeck                       | Widening disallowed; MVP VM uses `s64` slots for all numeric ops | `u32` value correct in VM when implemented |
| `bool`                            | partial | typeck + VM                  |                                     |                           |
| `()` unit                         | done    | typeck                       |                                     | `main :: () =>`           |
| Tuples                            | partial | parse + typeck               | No `MAKE_TUPLE` VM                  | Tuple value in VM         |
| Fixed arrays `[T; N]`             | partial | typeck                       | Literal typing; no heap array VM    | Index returns element     |
| Slices `[T]`                      | partial | types only                   |                                     | Post-MVP runtime          |
| Type aliases                      | partial | resolver + typeck            |                                     | Alias resolves            |
| `struct` decl + literal           | partial | parse, typeck, partial lower | No field storage in VM              | Read `p.x` after lit      |
| `enum` decl + ctors               | partial | parse, typeck                | No tag payload in VM                | `match` on enum tag       |


---

## Functions


| Item                             | Status  | Where          | Notes                        | Acceptance                         |
| -------------------------------- | ------- | -------------- | ---------------------------- | ---------------------------------- |
| Top-level `name :: (…) => T { }` | done    | full pipeline  |                              | `add` in sample                    |
| Trailing expr return             | done    | typeck         |                              | `function_trailing_expr_return_ok` |
| `return expr;`                   | done    | lower + VM     |                              |                                    |
| Recursion                        | partial | VM `CALL`      | Should work if typeck passes | Recursive factorial .phx           |
| Methods / receiver               | missing | typeck partial | No `self` lowering           | `Point :: impl { fn …(self) }`     |


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
| Use-after-move diagnostic + move site | partial | typeck               | Tracker has span; diagnostic uses symbol index            | Message cites move span in source |
| Copyable primitives / tuples / arrays | done    | `typeck/builtins.rs` |                                                           | `const b = a` for `s32`           |
| Full borrow checker                   | missing | —                    | Post-MVP per [ownership.md](design/features/ownership.md) | —                                 |


---

## Traits (`traits.md` MVP: parse + static method resolution)


| Item                                  | Status  | Where    | Notes                                                    | Acceptance                          |
| ------------------------------------- | ------- | -------- | -------------------------------------------------------- | ----------------------------------- |
| Parse `trait` / `impl` / `impl for`   | done    | parser   |                                                          | Parser tests                        |
| Resolve trait/impl names              | done    | resolver |                                                          |                                     |
| Type-check impl methods               | done    | typeck   | Members as functions                                     |                                     |
| Static method resolution to impl      | missing | typeck   | No trait vtable; methods not looked up via receiver type | Call trait method on typed receiver |
| Inherent vs trait impl disambiguation | missing | —        |                                                          |                                     |
| `#derive` codegen                     | missing | deferred |                                                          | —                                   |


---

## Modules / `#import` (`modules.md`)


| Item                               | Status  | Where    | Notes                   | Acceptance                      |
| ---------------------------------- | ------- | -------- | ----------------------- | ------------------------------- |
| Parse `#import`                    | done    | parser   |                         |                                 |
| Load multiple files / module paths | missing | —        | Single compilation unit | Two-file project with `#import` |
| `pub` exports                      | missing | resolver |                         |                                 |
| Path `std::…` → file mapping       | missing | —        |                         |                                 |


---

## CLI & tooling


| Item                             | Status  | Where                    | Notes                       | Acceptance           |
| -------------------------------- | ------- | ------------------------ | --------------------------- | -------------------- |
| `phx help`                       | done    | `source/phx/src/main.rs` |                             | `tests/cli/help.sh`  |
| `phx check <file>`               | done    | same                     | Parse+resolve+typeck        | `tests/cli/check.sh` |
| `phx run <file>`                 | done    | compile+verify+VM        |                             | `tests/cli/run.sh`   |
| `phx compile -o`                 | missing | —                        |                             |                      |
| Pretty diagnostics (span labels) | missing | `phx-diagnostics`        | Comment: "later formatting" |                      |


---

## Tests & fixtures


| Item                                      | Status  | Where                                   | Notes                                               | Acceptance                                           |
| ----------------------------------------- | ------- | --------------------------------------- | --------------------------------------------------- | ---------------------------------------------------- |
| Lexer unit tests                          | done    | `source/phx-syntax/tests/lexer.rs`      | Large table                                         |                                                      |
| Parser unit tests                         | done    | `source/phx-syntax/tests/parser.rs`     | Includes deferred=unsupported                       |                                                      |
| Resolver / typeck / lower / codegen tests | done    | `source/phx-compiler/tests/`            |                                                     |                                                      |
| Verifier tests                            | done    | `source/phx-bytecode/src/verify.rs`     | `#[cfg(test)]`                                      |                                                      |
| CLI shell tests                           | done    | `tests/cli/*.sh`                        | sample, bad_type, missing_main, control_flow        | CI runs scripts                                      |
| Integration `run_sample`                  | done    | `tests/integration/tests/run_sample.rs` |                                                     |                                                      |
| Integration control_flow                  | done    | `tests/integration/tests/run_control_flow.rs` | compile → verify → run | `cargo test -p phx-integration-tests --test run_control_flow` |
| Corpus of `.phx` programs                 | partial | `tests/cli/fixtures/`                   | **8** files (6 run via `run.sh`)                    | Expand per feature |
| Negative diagnostics fixtures             | partial | `bad_type.phx`, `missing_main.phx`      |                                                     | Expected-error sidecars                              |


---

## Explicitly out of scope (do not implement for MVP)

Per [mvp.md](design/mvp.md) and [grammar-deferred.md](design/features/grammar-deferred.md):

- M:N scheduler, actors, mailboxes, supervision (`@spawn`, `@send`, …)
- Std I/O, networking, collections, formatting APIs
- Primitive `string` type
- Full borrow checker / lifetimes / exclusivity of `&mut`
- JIT, hot reload, `#derive` / `@derive` semantic codegen
- `for` loops, range literals, closures/lambdas (parse rejected today)
- Package manager / build system beyond `phx` CLI
- Post-MVP: runtime transparency, schedulable I/O ([runtime-transparency.md](design/features/runtime-transparency.md))
- Post-MVP: std `Option` / `Result` / `?` ([type-system.md](design/features/type-system.md#phased-option-and-result-language--std))

---

## Suggested implementation order

Aligned with `.cursor/rules/phoenix.mdc` (lexer → parser → AST → resolver → typeck → IR → codegen → verifier → VM → tests):

### Phase 1 — Credible demo on current VM (`s64` scalar stack)

1. ~~Fix `continue` in `if`~~ — done (loop exit placement + `var` assign `StoreLocal`).
2. ~~Logical ops~~ — done (`&&` / `||` short-circuit).
3. ~~Match lowering (primitives)~~ — done for literal / `_` / ident; enum/struct patterns Phase 2.
4. ~~E2E fixtures~~ — `run.sh` runs 6 programs; `run_match.rs` integration tests.

### Phase 2 — User-defined aggregates (done)

1. ~~**VM value model:**~~ `Value::Scalar` / `Value::Agg` arena handles (freed at run end; not GC).
2. ~~`MAKE_STRUCT` / `MAKE_ENUM` / `GET_FIELD` / `SET_FIELD` / `MATCH_TAG`~~ (+ verifier stack rules).
3. ~~**Pattern typeck:**~~ struct/tuple/unit enum patterns against scrutinee type.
4. ~~**Method resolution:**~~ inherent impl `receiver.method` → `Call` with synthetic receiver param.

Fixtures: `struct_point.phx`, `struct_assign.phx`, `enum_match.phx`, `struct_method.phx`; integration `run_aggregates.rs`.

### Phase 3 — Bytecode completeness & tooling (done)

1. ~~**Remaining MVP opcodes:**~~ `Cast`, `Mod`, `Pow`, `Neg`, `Not`, bitwise, `Ne`/`Le`/`Ge`, `MakeTuple`, `MakeArray`, `Index`, `Trap`.
2. ~~**Float paths:**~~ deferred — MVP VM uses `i64` scalars; int/bool casts only (`cast_width.phx`).
3. ~~**`phx compile -o`**~~ — `tests/cli/compile.sh`.
4. ~~**Source diagnostics**~~ — caret rendering in `phx check` / `phx run` (`tests/cli/check.sh`).
5. ~~**Language surface:**~~ explicit casts, tuple/array runtime, `given`, trait dispatch, `self` in impl bodies.

Fixtures: `cast_width.phx`, `mod_bitwise.phx`, `array_index.phx`, `tuple_lit.phx`, `given_struct.phx`, `trait_eq.phx`; `run.sh` runs 16 programs.

### Phase 4 — Multi-file

1. `**#import`:** module graph, file path → `::` path, `pub` visibility.
2. **Std + prelude:** define `Option`/`Result` as generic enums; then `?` sugar and discard rules.

---

## Quick reference: pipeline stage map

```text
.phx source
  → phx-syntax (lex/parse)     [strong]
  → phx-compiler/resolver      [single-file; import blocked]
  → phx-compiler/typeck        [MVP rules + aggregate layouts/patterns]
  → phx-compiler/lower → ir    [control flow + struct/enum aggregates]
  → phx-compiler/codegen       [PHX0 + types section metadata]
  → phx-bytecode/verify        [scalar + aggregate opcode rules]
  → phx-vm/interpreter         [scalar + arena aggregate handles; 20 opcodes]
```

---

## Checklist totals (implementation items in tables above)


| Status      | Count |
| ----------- | ----- |
| **done**    | 74    |
| **partial** | 43    |
| **missing** | 22    |
| **deferred** (post-MVP std) | 4 |
| **Total**   | 143   |


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


