//! Assert computed values in `main` local slots after VM execution.

use phx_bytecode::{BytecodeModule, ScalarValue};
use phx_vm::{Value, run_captured};

/// Expected scalar value at a `main` local slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExpectedLocal {
    /// Signed 32-bit integer (`s32`).
    S32(i32),
    /// Signed 8-bit integer (`s8`).
    S8(i8),
    /// Boolean.
    Bool(bool),
    /// Unsigned 8-bit integer (`u8`).
    U8(u8),
    /// Signed 64-bit integer (`s64`).
    S64(i64),
    /// 64-bit float (`f64`, compared with `f64::EPSILON`).
    F64(f64),
}

/// Assert a single `main` local slot matches `expected`.
pub fn assert_main_local(module: &BytecodeModule, slot: usize, expected: ExpectedLocal) {
    assert_main_locals(module, &[(slot, expected)]);
}

/// Assert multiple `main` local slots match `expected` values.
pub fn assert_main_locals(module: &BytecodeModule, slots: &[(usize, ExpectedLocal)]) {
    let capture = run_captured(module).unwrap_or_else(|e| panic!("run: {e}"));
    for &(slot, expected) in slots {
        match expected {
            ExpectedLocal::S32(v) => {
                let actual = capture
                    .main_local(slot)
                    .and_then(scalar_i32)
                    .unwrap_or_else(|| panic!("slot {slot} not s32: {:?}", capture.main_locals));
                assert_eq!(actual, v, "slot {slot}");
            }
            ExpectedLocal::S8(v) => {
                let actual = capture
                    .main_local(slot)
                    .and_then(scalar_i8)
                    .unwrap_or_else(|| panic!("slot {slot} not s8: {:?}", capture.main_locals));
                assert_eq!(actual, v, "slot {slot}");
            }
            ExpectedLocal::Bool(v) => {
                let actual = capture
                    .main_local(slot)
                    .and_then(scalar_bool)
                    .unwrap_or_else(|| panic!("slot {slot} not bool: {:?}", capture.main_locals));
                assert_eq!(actual, v, "slot {slot}");
            }
            ExpectedLocal::U8(v) => {
                let actual = capture
                    .main_local(slot)
                    .and_then(scalar_u8)
                    .unwrap_or_else(|| panic!("slot {slot} not u8: {:?}", capture.main_locals));
                assert_eq!(actual, v, "slot {slot}");
            }
            ExpectedLocal::S64(v) => {
                let actual = capture
                    .main_local(slot)
                    .and_then(scalar_i64)
                    .unwrap_or_else(|| panic!("slot {slot} not s64: {:?}", capture.main_locals));
                assert_eq!(actual, v, "slot {slot}");
            }
            ExpectedLocal::F64(v) => {
                let actual = capture
                    .main_local(slot)
                    .and_then(scalar_f64)
                    .unwrap_or_else(|| panic!("slot {slot} not f64: {:?}", capture.main_locals));
                assert!(
                    (actual - v).abs() < f64::EPSILON,
                    "slot {slot}: expected {v}, got {actual}"
                );
            }
        }
    }
}

fn scalar_i32(value: Value) -> Option<i32> {
    match value {
        Value::Scalar(ScalarValue::I32(v)) => Some(v),
        _ => None,
    }
}

fn scalar_i8(value: Value) -> Option<i8> {
    match value {
        Value::Scalar(ScalarValue::I8(v)) => Some(v),
        _ => None,
    }
}

fn scalar_bool(value: Value) -> Option<bool> {
    match value {
        Value::Scalar(ScalarValue::Bool(v)) => Some(v),
        _ => None,
    }
}

fn scalar_u8(value: Value) -> Option<u8> {
    match value {
        Value::Scalar(ScalarValue::U8(v)) => Some(v),
        _ => None,
    }
}

fn scalar_i64(value: Value) -> Option<i64> {
    match value {
        Value::Scalar(ScalarValue::I64(v)) => Some(v),
        _ => None,
    }
}

fn scalar_f64(value: Value) -> Option<f64> {
    match value {
        Value::Scalar(ScalarValue::F64(v)) => Some(v),
        _ => None,
    }
}
