//! Built-in VM foreign stubs for the pre-scheduler I/O bridge.
//!
//! See `docs/design/features/io-bridge.md`.

use std::io::{self, Write as _};

use phx_bytecode::{BytecodeModule, ConstTag, PTR_CONST_TAG, ScalarValue};

use crate::VmErrorKind;
use crate::context::Machine;
use crate::foreign::register_foreign_stub;
use crate::frame::{Aggregate, Value};

/// Well-known foreign symbol for stdout bridge (Phase A).
pub const PHOENIX_WRITE_STDOUT: &str = "phoenix_write_stdout";

/// Registers built-in foreign stubs used by `phx run` and I/O bridge tests.
///
/// Re-registering is idempotent per symbol name ([`register_foreign_stub`]).
pub fn register_builtin_foreign_stubs() {
    let _ = register_foreign_stub(PHOENIX_WRITE_STDOUT, phoenix_write_stdout_stub);
}

fn phoenix_write_stdout_stub(
    machine: &mut Machine,
    module: &BytecodeModule,
) -> Result<(), VmErrorKind> {
    let arg = machine.stack().pop().ok_or(VmErrorKind::StackUnderflow)?;
    let bytes = str_literal_bytes(machine, module, arg)?;
    let written = match io::stdout().write_all(bytes) {
        Ok(()) => i64::try_from(bytes.len()).unwrap_or(i64::MAX),
        Err(_) => -1,
    };
    machine
        .stack()
        .push(Value::Scalar(ScalarValue::I64(written)));
    Ok(())
}

fn str_literal_bytes<'a>(
    machine: &Machine,
    module: &'a BytecodeModule,
    value: Value,
) -> Result<&'a [u8], VmErrorKind> {
    let handle = value.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
    let Aggregate::Str { ptr, len } = machine
        .aggregate(handle)
        .ok_or(VmErrorKind::InvalidAggregate)?
    else {
        return Err(VmErrorKind::InvalidAggregate);
    };
    if *ptr & PTR_CONST_TAG != PTR_CONST_TAG {
        return Err(VmErrorKind::InvalidAggregate);
    }
    let idx = u32::try_from(*ptr & !PTR_CONST_TAG).map_err(|_| VmErrorKind::InvalidConstPayload)?;
    let entry = module
        .constants
        .entries
        .get(idx as usize)
        .ok_or(VmErrorKind::InvalidConstIndex(idx))?;
    if entry.tag != ConstTag::Bytes {
        return Err(VmErrorKind::InvalidConstPayload);
    }
    let end = usize::try_from(*len).unwrap_or(entry.payload.len());
    let end = end.min(entry.payload.len());
    Ok(&entry.payload[..end])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use phx_bytecode::ScalarValue;

    use super::*;
    use crate::{Machine, Value};

    #[test]
    fn phoenix_write_stdout_stub_rejects_non_str() {
        let mut machine = Machine::default();
        let module = phx_bytecode::BytecodeModule {
            header: phx_bytecode::FileHeader::new(5, 0),
            constants: phx_bytecode::ConstPool::default(),
            types: phx_bytecode::TypeTable::default(),
            functions: phx_bytecode::FunctionTable::default(),
            code: Vec::new(),
            local_layouts: phx_bytecode::LocalLayoutTable::default(),
        };
        machine.stack().push(Value::Scalar(ScalarValue::I32(1)));
        let err = phoenix_write_stdout_stub(&mut machine, &module).expect_err("not str");
        assert_eq!(err, VmErrorKind::InvalidAggregate);
    }
}
