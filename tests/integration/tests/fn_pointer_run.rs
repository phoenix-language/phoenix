//! V0-053: function pointer indirect call smoke.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_test::{ExpectedLocal, assert_main_locals, compile_fixture};

#[test]
fn fn_pointer_fixture_runs() {
    let module = compile_fixture("fn_pointer.phx");
    assert_main_locals(&module, &[(0, ExpectedLocal::Bool(true))]);
}
