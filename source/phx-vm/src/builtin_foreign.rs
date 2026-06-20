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

/// Writes a zero-terminated `[u8; 32]` display buffer to host stdout.
pub const PHOENIX_WRITE_DISPLAY_BUF: &str = "phoenix_write_display_buf";

/// Registers built-in foreign stubs used by `phx run` and I/O bridge tests.
///
/// Re-registering is idempotent per symbol name ([`register_foreign_stub`]).
pub fn register_builtin_foreign_stubs() {
    let _ = register_foreign_stub(PHOENIX_WRITE_STDOUT, phoenix_write_stdout_stub);
    let _ = register_foreign_stub(PHOENIX_WRITE_DISPLAY_BUF, phoenix_write_display_buf_stub);
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

fn phoenix_write_display_buf_stub(
    machine: &mut Machine,
    _module: &BytecodeModule,
) -> Result<(), VmErrorKind> {
    let arg = machine.stack().pop().ok_or(VmErrorKind::StackUnderflow)?;
    let bytes = display_buf_bytes(machine, arg)?;
    let written = match io::stdout().write_all(&bytes) {
        Ok(()) => i64::try_from(bytes.len()).unwrap_or(i64::MAX),
        Err(_) => -1,
    };
    machine
        .stack()
        .push(Value::Scalar(ScalarValue::I64(written)));
    Ok(())
}

fn display_buf_bytes(machine: &Machine, value: Value) -> Result<Vec<u8>, VmErrorKind> {
    let handle = value.as_agg().ok_or(VmErrorKind::InvalidAggregate)?;
    let Aggregate::Array { elems } = machine
        .aggregate(handle)
        .ok_or(VmErrorKind::InvalidAggregate)?
    else {
        return Err(VmErrorKind::InvalidAggregate);
    };
    let mut out = Vec::new();
    for elem in elems {
        let b = match elem {
            Value::Scalar(ScalarValue::U8(b)) => *b,
            _ => return Err(VmErrorKind::InvalidAggregate),
        };
        if b == 0 {
            break;
        }
        out.push(b);
    }
    Ok(out)
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
    use phx_bytecode::{PcSpanTable, ScalarValue};

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
            pc_spans: PcSpanTable::default(),
        };
        machine.stack().push(Value::Scalar(ScalarValue::I32(1)));
        let err = phoenix_write_stdout_stub(&mut machine, &module).expect_err("not str");
        assert_eq!(err, VmErrorKind::InvalidAggregate);
    }

    #[test]
    fn phoenix_write_display_buf_stub_writes_zero_terminated_bytes() {
        use crate::frame::Aggregate;
        let mut machine = Machine::default();
        let module = phx_bytecode::BytecodeModule {
            header: phx_bytecode::FileHeader::new(5, 0),
            constants: phx_bytecode::ConstPool::default(),
            types: phx_bytecode::TypeTable::default(),
            functions: phx_bytecode::FunctionTable::default(),
            code: Vec::new(),
            local_layouts: phx_bytecode::LocalLayoutTable::default(),
            pc_spans: PcSpanTable::default(),
        };
        let elems: Vec<Value> = b"42\0"
            .iter()
            .copied()
            .map(|b| Value::Scalar(ScalarValue::U8(b)))
            .chain(std::iter::repeat_n(Value::Scalar(ScalarValue::U8(0)), 29))
            .collect();
        let buf = machine.push_aggregate(Aggregate::Array { elems });
        machine.stack().push(buf);
        phoenix_write_display_buf_stub(&mut machine, &module).expect("write display buf");
        let ScalarValue::I64(written) = machine.stack().pop().unwrap().as_scalar().unwrap() else {
            panic!("expected ssize");
        };
        assert_eq!(written, 2);
    }
}
