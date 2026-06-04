//! Workspace integration smoke tests.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[test]
fn workspace_links() {
    phx_compiler::compile_source("main :: () => { };", None).expect("compile pipeline should link");
}
