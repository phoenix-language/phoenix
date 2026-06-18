# CLI integration tests

On-disk fixtures under `tests/cli/fixtures/` were removed. Phoenix test sources now live in [`../phx-programs/`](../phx-programs/) and are written to temp workspaces by [`../phx-test/`](../phx-test/).

- **CLI subprocess tests:** [`../integration/tests/cli_e2e.rs`](../integration/tests/cli_e2e.rs)
- **In-process smoke:** [`../integration/tests/run_smoke.rs`](../integration/tests/run_smoke.rs)

From the repo root:

```bash
just test-cli          # cli_e2e + run_smoke
just test-lang         # broader integration subset
cargo test -p phx-integration-tests --test cli_e2e
```

## Module root (`#import`)

Phoenix resolves `#import` paths relative to a **module root** directory (the folder that mirrors `::` path segments).

| Workflow | Module root | Entry |
|----------|-------------|-------|
| **Single-file** | Parent directory of the `.phx` file (default for `phx check` / `phx run` on one file) | materialized `sample.phx` via `phx-test` |
| **M1 multi-file** | Explicit via `--module-src` | `phx run --module-src <root> <entry>` |
| **M2 project** | `src/` (or `[project] module_src` in `phoenix.toml`) | `phx build` / `phx run` / `phx check` on a project file |

- **`phx check <file>`** and **`phx run <file>`**: when `phoenix.toml` is found by walking parents from the file path, use `[project] module_src` and path dependencies (same as build). With no project, use the file's parent as module root unless `--module-src` is set.
- **`compile_source(..., None)`** (in-process API, no path): **no** module root — `#import` fails with `ImportNotSupported`. Use `check_file_with_module_path` or a project build instead.
- **M2:** `discover_project` + `build_project` load all modules under the configured `src` tree.
