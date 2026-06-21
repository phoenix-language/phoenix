//! V0-053: extern "C" run with VM foreign stub registration.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use phx_bytecode::{BytecodeModule, ScalarValue, verify};
use phx_test::{ExpectedLocal, assert_main_locals, ensure_built_project, require_cli_project};
use phx_vm::{
    ForeignStubFn, Machine, Value, VmErrorKind, clear_foreign_stubs,
    register_builtin_foreign_stubs, register_foreign_stub,
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
    clear_foreign_stubs();
    register_builtin_foreign_stubs();
    require_cli_project("extern_c");
    let _stub_id: u32 = register_foreign_stub("c_add", c_add_stub as ForeignStubFn);
    let built = ensure_built_project("extern_c");
    verify(&built.module).expect("verify extern_c");
    assert_main_locals(&built.module, &[(0, ExpectedLocal::S32(12))]);
}
