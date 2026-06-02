//! Workspace integration smoke tests.

#[test]
fn workspace_links() {
    phx_compiler::compile_source("main :: () => { };", None).expect("compile pipeline should link");
}
