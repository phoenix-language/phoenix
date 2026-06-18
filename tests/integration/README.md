# phx-integration-tests

End-to-end integration tests across workspace crates (compile, run, diagnostics, and CLI).

Shared helpers live in [`../phx-test/`](../phx-test/). Phoenix test sources are embedded in [`../phx-programs/`](../phx-programs/) and materialized to temp directories at runtime.

## Test binaries

| Binary | Purpose |
|--------|---------|
| [`run_smoke.rs`](tests/run_smoke.rs) | In-process compile → verify → run on embedded smoke programs |
| [`run_semantics.rs`](tests/run_semantics.rs) | Assert computed `main` local slots via `VmRunCapture` |
| [`cli_e2e.rs`](tests/cli_e2e.rs) | Subprocess `phx` CLI tests (flags, projects, path deps, explain) |
| [`diagnostics.rs`](tests/diagnostics.rs) | Golden formatted diagnostics (embedded expected strings) |
| Other `run_*` / `incremental_*` | Project build, path deps, modules |

**Diagnostic goldens:** compare formatted output to embedded `expected` strings in `phx-programs`. Regenerate with:

```bash
UPDATE_GOLDEN=1 cargo test -p phx-integration-tests --test diagnostics
```

## Running

```bash
cargo test -p phx-integration-tests
just test-lang    # integration subset
just test-cli     # cli_e2e + run_smoke
```
