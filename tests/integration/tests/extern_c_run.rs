//! V0-053: extern "C" run with VM foreign stub registration.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_bytecode::{BytecodeModule, ScalarValue, verify};
use phx_compiler::{BuildOptions, ProjectConfig, build_project};
use phx_test::{
    ExpectedLocal, assert_main_locals, fixture_fs_lock, load_built_binary, require_cli_project,
};
use phx_vm::{
    ForeignStubFn, Machine, Value, VmErrorKind, clear_foreign_stubs, register_foreign_stub,
};

fn c_add_stub(machine: &mut Machine, _module: &BytecodeModule) -> Result<(), VmErrorKind> {
    let b = machine.stack().pop().ok_or(VmErrorKind::StackUnderflow)?;
    let a = machine.stack().pop().ok_or(VmErrorKind::StackUnderflow)?;
    let Value::Scalar(ScalarValue::I32(x)) = a else {
        return Err(VmErrorKind::ExpectedScalar);
    };
    let Value::Scalar(ScalarValue::I32(y)) = b else {
        return Err(VmErrorKind::ExpectedScalar);
    };
    machine
        .stack()
        .push(Value::Scalar(ScalarValue::I32(x.saturating_add(y))));
    Ok(())
}

#[test]
fn extern_c_fixture_runs_with_registered_stub() {
    let _lock = fixture_fs_lock();
    clear_foreign_stubs();
    let root = require_cli_project("extern_c");
    let _stub_id: u32 = register_foreign_stub("c_add", c_add_stub as ForeignStubFn);
    let config = ProjectConfig::load(&root).expect("load extern_c");
    build_project(&config, None, BuildOptions::force(true)).expect("build extern_c");
    let module = load_built_binary(&config).expect("load binary");
    verify(&module).expect("verify extern_c");
    assert_main_locals(&module, &[(0, ExpectedLocal::S32(12))]);
}
