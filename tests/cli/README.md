# CLI tests

End-to-end tests for the `phx` binary.

| Script | What it does |
|--------|----------------|
| [`check.sh`](check.sh) | Builds `phx`, runs `phx check` on success and failure fixtures |
| [`run.sh`](run.sh) | Builds `phx`, runs `phx run` on `fixtures/sample.phx` |
| [`help.sh`](help.sh) | Smoke test for `phx help` usage text |

| Fixture | Expected |
|---------|----------|
| [`fixtures/sample.phx`](fixtures/sample.phx) | Exit 0 |
| [`fixtures/bad_type.phx`](fixtures/bad_type.phx) | Non-zero (type mismatch) |

From the repo root:

```bash
just run tests/cli/fixtures/sample.phx
tests/cli/check.sh
# or
just test-cli
```
