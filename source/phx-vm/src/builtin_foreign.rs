//! Built-in VM foreign stubs for the pre-scheduler I/O bridge.
//!
//! Registers the well-known stdout symbols used by `std::io::write_stdout` and
//! `std::text::fmt::print` before the M:N scheduler ships. Each stub decodes Phoenix `str` or
//! display-buffer aggregates on the host and performs a blocking `std::io::stdout().write_all`.
//!
//! See `docs/design/features/io-bridge.md` and [`crate::foreign`] for Phase A dispatch.
//!
//! ## Stub contract
//!
//! | Symbol | Phoenix signature | Payload |
//! | --- | --- | --- |
//! | [`PHOENIX_WRITE_STDOUT`] | `(s: str) => c_ssize` | Const-pool `str` literals only |
//! | [`PHOENIX_WRITE_DISPLAY_BUF`] | `(buf: [u8; 32]) => c_ssize` | Zero-terminated `[u8; 32]` display buffer |
//!
//! Return value is bytes written as `c_ssize`, or `-1` on host I/O failure. `phx run` calls
//! [`register_builtin_foreign_stubs`] before execution.

use std::io::{self, Write as _};

use phx_bytecode::{BytecodeModule, ConstTag, PTR_CONST_TAG, ScalarValue};

use crate::VmErrorKind;
use crate::context::Machine;
use crate::foreign::register_foreign_stub;
use crate::frame::{Aggregate, Value};

/// Well-known foreign symbol for the stdout string bridge (Phase A).
///
/// Lowered from `extern "C" phoenix_write_stdout :: (s: str) => c_ssize`. The stub accepts only
/// const-pool-backed `str` aggregates; other provenance returns [`VmErrorKind::InvalidAggregate`].
pub const PHOENIX_WRITE_STDOUT: &str = "phoenix_write_stdout";

/// Well-known foreign symbol for the display-buffer stdout bridge (Phase A).
///
/// Lowered from `extern "C" phoenix_write_display_buf :: (buf: [u8; 32]) => c_ssize`. Used by
/// `std::text::fmt::print`; reads bytes until the first `0` in the fixed-size array.
pub const PHOENIX_WRITE_DISPLAY_BUF: &str = "phoenix_write_display_buf";

/// Registers built-in foreign stubs used by `phx run` and I/O bridge tests.
///
/// Idempotent per symbol name via [`register_foreign_stub`]. Registration order is
/// stdout first, then display buffer — must match bridge program `extern` declaration order.
pub fn register_builtin_foreign_stubs() {
    let _ = register_foreign_stub(PHOENIX_WRITE_STDOUT, phoenix_write_stdout_stub);
    let _ = register_foreign_stub(PHOENIX_WRITE_DISPLAY_BUF, phoenix_write_display_buf_stub);
}

/// Stub for [`PHOENIX_WRITE_STDOUT`]: pops one `str` aggregate, writes UTF-8 bytes to host stdout.
///
/// # Errors
///
/// Returns [`VmErrorKind::StackUnderflow`] when the stack is empty. Returns
/// [`VmErrorKind::InvalidAggregate`] or [`VmErrorKind::InvalidConstIndex`] /
/// [`VmErrorKind::InvalidConstPayload`] when the argument is not a const-pool `str`.
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

/// Stub for [`PHOENIX_WRITE_DISPLAY_BUF`]: pops one `[u8; 32]` aggregate, writes zero-terminated bytes.
///
/// # Errors
///
/// Returns [`VmErrorKind::StackUnderflow`] when the stack is empty. Returns
/// [`VmErrorKind::InvalidAggregate`] when the argument is not a byte array aggregate.
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

/// Extracts zero-terminated bytes from a Phoenix `[u8; 32]` display buffer aggregate.
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

/// Resolves const-pool UTF-8 bytes from a Phoenix `str` aggregate (`PTR_CONST_TAG | pool_index`).
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
