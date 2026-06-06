# CLI fixtures

On-disk fixtures for Phoenix end-to-end tests. The shell scripts in this directory were replaced by Rust tests:

- **CLI subprocess tests:** [`../integration/tests/cli_e2e.rs`](../integration/tests/cli_e2e.rs) via [`phx-test`](../phx-test/)
- **In-process smoke:** [`../integration/tests/run_smoke.rs`](../integration/tests/run_smoke.rs)

From the repo root:

```bash
just test-cli          # cli_e2e + run_smoke (CI parity)
just test-lang         # pre-commit subset
cargo test -p phx-integration-tests --test cli_e2e --test run_smoke -- --test-threads=1
```

CI runs `cargo test -p phx-integration-tests --test cli_e2e -- --test-threads=1` in the `cli` job.

## Module root (`#import`)

Phoenix resolves `#import` paths relative to a **module root** directory (the folder that mirrors `::` path segments). How you set that root depends on the workflow:

| Workflow | Module root | Entry |
|----------|-------------|-------|
| **Single-file** | Parent directory of the `.phx` file (default for `phx check` / `phx run` on one file) | `phx run tests/cli/fixtures/sample.phx` |
| **M1 multi-file** | Explicit via `--module-src` | `phx run --module-src tests/cli/fixtures/modules tests/cli/fixtures/modules/main.phx` |
| **M2 project** | `src/` (or `[package] module_src` in `phoenix.toml`) | `phx build` / `phx run` / `phx check` on a file under the project |

- **`phx check <file>`** and **`phx run <file>`**: when `phoenix.toml` is found by walking parents from the file path, use `[project] module_src` and path dependencies (same as build). With no project, use the file's parent as module root unless `--module-src` is set.
- **`compile_source(..., None)`** (in-process API, no path): **no** module root — `#import` fails with `ImportNotSupported`. Use [`check_file_with_module_path`](../../source/phx-compiler/src/compile.rs) or a project build instead.
- **M2:** `discover_project` + `build_project` load all modules under the configured `src` tree.

## Fixture inventory

See the previous README sections for positive/negative/project fixture tables — file names under [`fixtures/`](fixtures/) are unchanged.
