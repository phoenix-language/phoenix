# CLI tests

End-to-end tests for the `phx` binary. From the repo root:

```bash
just test-cli          # all CLI scripts (CI parity)
just test-lang         # check.sh + run.sh + build.sh + help.sh (pre-commit subset)
tests/cli/check.sh     # negative + module diagnostics
tests/cli/run.sh       # compile + run positive fixtures
tests/cli/build.sh     # phoenix.toml project build/run
tests/cli/compile.sh   # phx compile -o
```

| Script | What it does |
|--------|----------------|
| [`check.sh`](check.sh) | `phx check` on valid and invalid fixtures; module import/cycle/private cases |
| [`run.sh`](run.sh) | `phx run` on positive `.phx` fixtures + multi-file modules |
| [`build.sh`](build.sh) | `phx build` / `phx run --no-build` on [`fixtures/project/`](fixtures/project/) |
| [`compile.sh`](compile.sh) | `phx compile -o` writes verifiable `.phx0` |
| [`help.sh`](help.sh) | Smoke test for `phx help` usage text |

CI (`.github/workflows/ci.yml`) `cli` job runs all five scripts (`check.sh`, `run.sh`, `build.sh`, `compile.sh`, `help.sh`). Locally, `just test-cli` matches that job; `just pre-commit` runs `just test-lang` (check, run, build, help) plus fmt/clippy/doc — not `cargo test --workspace` (use `just test` for full Rust tests).

## Module root (`#import`)

Phoenix resolves `#import` paths relative to a **module root** directory (the folder that mirrors `::` path segments). How you set that root depends on the workflow:

| Workflow | Module root | Entry |
|----------|-------------|-------|
| **Single-file** | Parent directory of the `.phx` file (default for `phx check` / `phx run` on one file) | `phx run tests/cli/fixtures/sample.phx` |
| **M1 multi-file** | Explicit via `--module-src` | `phx run --module-src tests/cli/fixtures/modules tests/cli/fixtures/modules/main.phx` |
| **M2 project** | `src/` (or `[package] module_src` in `phoenix.toml`) | `phx build` / `phx run` / `phx check` on a file under the project |

- **`phx check <file>`** and **`phx run <file>`**: when `phoenix.toml` is found by walking parents from the file path, use `[project] module_src` and path dependencies (same as build). With no project, use the file’s parent as module root unless `--module-src` is set.
- **`compile_source(..., None)`** (in-process API, no path): **no** module root — `#import` fails with `ImportNotSupported`. Use [`check_file_with_module_path`](../../source/phx-compiler/src/compile.rs) or a project build instead.
- **M2:** `discover_project` + `build_project` load all modules under the configured `src` tree.

## Fixture inventory

### Positive — compile + run (`run.sh`)

| Fixture | Kind | Used by | Expected outcome |
|---------|------|---------|------------------|
| [`sample.phx`](fixtures/sample.phx) | positive | `run.sh`, `check.sh`, `compile.sh` | Exit 0; arithmetic + `if` |
| [`control_flow.phx`](fixtures/control_flow.phx) | positive | `run.sh` | Exit 0; `while` / `loop` / `break` |
| [`continue_in_if.phx`](fixtures/continue_in_if.phx) | positive | `run.sh` | Exit 0; `continue` inside `if` |
| [`logical.phx`](fixtures/logical.phx) | positive | `run.sh` | Exit 0; short-circuit `&&` / `\|\|` |
| [`match_int.phx`](fixtures/match_int.phx) | positive | `run.sh` | Exit 0; `match` on `s32` |
| [`match_ident.phx`](fixtures/match_ident.phx) | positive | `run.sh` | Exit 0; `match` with binding |
| [`match_bool.phx`](fixtures/match_bool.phx) | positive | `run.sh` | Exit 0; `match` on `bool` |
| [`struct_point.phx`](fixtures/struct_point.phx) | positive | `run.sh` | Exit 0; struct literal + fields |
| [`struct_assign.phx`](fixtures/struct_assign.phx) | positive | `run.sh` | Exit 0; struct field assign |
| [`enum_match.phx`](fixtures/enum_match.phx) | positive | `run.sh` | Exit 0; unit enum `match` |
| [`enum_match_struct.phx`](fixtures/enum_match_struct.phx) | positive | `run.sh` | Exit 0; struct-payload enum `match` |
| [`struct_method.phx`](fixtures/struct_method.phx) | positive | `run.sh` | Exit 0; inherent impl method call |
| [`cast_width.phx`](fixtures/cast_width.phx) | positive | `run.sh` | Exit 0; explicit `as` cast |
| [`compare_unary.phx`](fixtures/compare_unary.phx) | positive | `run.sh` | Exit 0; unary `-`/`!` and full comparisons |
| [`mod_bitwise.phx`](fixtures/mod_bitwise.phx) | positive | `run.sh` | Exit 0; `%`, shifts, bitwise ops |
| [`array_index.phx`](fixtures/array_index.phx) | positive | `run.sh` | Exit 0; fixed array index |
| [`tuple_lit.phx`](fixtures/tuple_lit.phx) | positive | `run.sh` | Exit 0; tuple literal |
| [`given_struct.phx`](fixtures/given_struct.phx) | positive | `run.sh` | Exit 0; `given` on struct pattern |
| [`trait_eq.phx`](fixtures/trait_eq.phx) | positive | `run.sh` | Exit 0; `Type :: impl :: Trait` dispatch |
| [`primitives_float.phx`](fixtures/primitives_float.phx) | positive | `run.sh` | Exit 0; `f32` / `f64` |
| [`primitives_width.phx`](fixtures/primitives_width.phx) | positive | `run.sh` | Exit 0; width-faithful integers |
| [`primitives_i128.phx`](fixtures/primitives_i128.phx) | positive | `run.sh` | Exit 0; `s128` / `u128` |
| [`byte_string.phx`](fixtures/byte_string.phx) | positive | `run.sh` | Exit 0; `b"…"` → `[u8; N]` |
| [`string_literal.phx`](fixtures/string_literal.phx) | positive | `run.sh` | Exit 0; `"…"` → `str`, `str as [u8]` |
| [`byte_string_as_str.phx`](fixtures/byte_string_as_str.phx) | positive | `run.sh` | Exit 0; `const` `b"…"` → `str` (const-fold) |
| [`ref_local.phx`](fixtures/ref_local.phx) | positive | `run.sh` | Exit 0; address-of local |
| [`deref_ptr.phx`](fixtures/deref_ptr.phx) | positive | `run.sh` | Exit 0; pointer deref |
| [`slice_from_array.phx`](fixtures/slice_from_array.phx) | positive | `run.sh` | Exit 0; array → slice cast |
| [`factorial.phx`](fixtures/factorial.phx) | positive | `run.sh`, `run_semantics.rs` | Exit 0; recursive `fac(5)` → 120 |
| [`given_enum_single_variant.phx`](fixtures/given_enum_single_variant.phx) | positive | `run.sh`, `run_semantics.rs` | Exit 0; exhaustive `given` on single-variant enum |
| [`modules/main.phx`](fixtures/modules/main.phx) | positive (multi-file) | `run.sh`, `check.sh` | Exit 0 with `--module-src fixtures/modules` |

### Negative — check fails (`check.sh`)

| Fixture | Kind | Used by | Expected outcome |
|---------|------|---------|------------------|
| [`bad_type.phx`](fixtures/bad_type.phx) | negative | `check.sh` | Non-zero; type mismatch with line + caret |
| [`missing_main.phx`](fixtures/missing_main.phx) | negative | `check.sh` | Non-zero; missing `main` |
| [`use_after_move.phx`](fixtures/use_after_move.phx) | negative | `check.sh` | Non-zero; use-after-move + move-site `note:` |
| [`mixed_width.phx`](fixtures/mixed_width.phx) | negative | `check.sh` | Non-zero; implicit mixed-width arithmetic |
| [`invalid_utf8_byte_as_str.phx`](fixtures/invalid_utf8_byte_as_str.phx) | negative | `check.sh` | Non-zero; invalid UTF-8 `b"…" as str` |
| [`given_enum_non_exhaustive.phx`](fixtures/given_enum_non_exhaustive.phx) | negative | `check.sh` | Non-zero; non-exhaustive `given` on enum |
| [`match_unreachable_arm.phx`](fixtures/match_unreachable_arm.phx) | negative | `check.sh` | Non-zero; unreachable `match` arm |
| [`modules/import_private.phx`](fixtures/modules/import_private.phx) | negative | `check.sh` | Non-zero; import of non-`pub` symbol |
| [`modules/cycle_a.phx`](fixtures/modules/cycle_a.phx) | negative | `check.sh` | Non-zero; circular `#import` |

### Supporting module files (not run directly)

| Fixture | Kind | Used by | Expected outcome |
|---------|------|---------|------------------|
| [`modules/util/math.phx`](fixtures/modules/util/math.phx) | module dep | `modules/main.phx` | Imported via `#import util::math::add` |
| [`modules/util/secret.phx`](fixtures/modules/util/secret.phx) | module dep | `import_private.phx` | Private symbol; triggers export error |
| [`modules/cycle_b.phx`](fixtures/modules/cycle_b.phx) | module dep | `cycle_a.phx` | Completes import cycle |

### Project / build fixtures

| Fixture | Kind | Used by | Expected outcome |
|---------|------|---------|------------------|
| [`project/`](fixtures/project/) | project (M2) | `build.sh`, `run_build.rs` | `phx build` → `build/bin/cli_project_test.phx0`; multi-module via `phoenix.toml` |
| [`project/src/main.phx`](fixtures/project/src/main.phx) | project entry | `build.sh` | Imports `util::math::add` from sibling module |
| [`project/src/util/math.phx`](fixtures/project/src/util/math.phx) | project module | `build.sh` | `pub` export consumed by `main.phx` |
| [`mvp_acceptance/`](fixtures/mvp_acceptance/) | MVP smoke project | `build.sh`, `run_build.rs` | Struct + enum `match` + `#import` + `phx build` / `run` |
| [`mvp_acceptance/src/main.phx`](fixtures/mvp_acceptance/src/main.phx) | MVP entry | `build.sh` | Imports `shapes::math::pick`; struct + `Result` match |
| [`app_dep/`](fixtures/app_dep/) | path dependency | `run_dep_build.rs` | Builds app + `build/deps/math/` artifacts |
| [`app_dep/src/main.phx`](fixtures/app_dep/src/main.phx) | app entry | `run_dep_build.rs` | `#import math::add` from path dep |
| [`math_lib/`](fixtures/math_lib/) | library package | `build.sh`, `run_build.rs`, `app_dep` | Standalone `phx build` → `build/lib/math.phx0`; path dep for `app_dep` |
| [`lib_with_main/`](fixtures/lib_with_main/) | negative lib | `check.sh` | `main` in lib package → E1014 |
| [`bad_dep_key/`](fixtures/bad_dep_key/) | negative project | `build.sh` | Dependency key ≠ `project.name` |
| [`bin_missing_main/`](fixtures/bin_missing_main/) | negative project | `build.sh` | `type = bin` without `main.phx` |
