# phx-integration-tests

End-to-end integration tests across workspace crates (compile, run, diagnostics, and CLI).

Shared helpers live in [`../phx-test/`](../phx-test/).

## Test binaries

| Binary | Purpose |
|--------|---------|
| [`run_smoke.rs`](tests/run_smoke.rs) | In-process compile → verify → run on positive CLI fixtures |
| [`run_semantics.rs`](tests/run_semantics.rs) | Assert computed `main` local slots via `VmRunCapture` |
| [`cli_e2e.rs`](tests/cli_e2e.rs) | Subprocess `phx` CLI tests (replaces `tests/cli/*.sh`) |
| [`diagnostics.rs`](tests/diagnostics.rs) | Golden formatted diagnostics |
| Other `run_*` / `incremental_*` | Project build, path deps, modules |

**Diagnostic goldens:** compare formatted output to [`diagnostics/*.stderr`](diagnostics/). Regenerate with:

```bash
UPDATE_GOLDEN=1 cargo test -p phx-integration-tests --test diagnostics
```

## Running

```bash
cargo test -p phx-integration-tests
just test-lang    # cli_e2e + run_smoke (pre-commit)
just test-cli     # same as test-lang
```
