//! Workspace integration smoke tests.

#[test]
fn workspace_links() {
    assert_eq!(phx_compiler::WORKSPACE, ());
    assert_eq!(phx_vm::WORKSPACE, ());
    assert_eq!(phx_bytecode::WORKSPACE, ());
}
