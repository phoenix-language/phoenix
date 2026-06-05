# phx-integration-tests

End-to-end integration tests across workspace crates (compile, run, and diagnostics fixtures).

**Semantics vs smoke:** [`tests/run_semantics.rs`](tests/run_semantics.rs) asserts computed `main` local slots via `VmRunCapture::main_local` (arithmetic, match, traits, modules, recursion, `given`, primitives, pointers, etc.). [`tests/run_control_flow.rs`](tests/run_control_flow.rs) only checks compile → verify → run succeeds for `control_flow.phx` — overlap with `run_semantics::control_flow_loop_counter_reaches_ten`; keep the former as a minimal pipeline smoke test unless you want one less binary target.

**Diagnostic goldens:** [`tests/diagnostics.rs`](tests/diagnostics.rs) compares formatted compiler output to [`diagnostics/*.stderr`](diagnostics/). Regenerate with `UPDATE_GOLDEN=1 cargo test -p phx-integration-tests --test diagnostics`.
