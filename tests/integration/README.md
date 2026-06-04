# phx-integration-tests

End-to-end integration tests across workspace crates (compile, run, and diagnostics fixtures).

**Semantics vs smoke:** [`tests/run_semantics.rs`](tests/run_semantics.rs) asserts computed `main` locals after `run_captured` (arithmetic, match, traits, modules, recursion, `given`, etc.). [`tests/run_control_flow.rs`](tests/run_control_flow.rs) only checks compile → verify → run succeeds for `control_flow.phx` — overlap with `run_semantics::control_flow_loop_counter_reaches_ten`; keep the former as a minimal pipeline smoke test unless you want one less binary target.
