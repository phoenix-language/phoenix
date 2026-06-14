//! Runtime value, aggregate, and frame types for the Phoenix VM.

use phx_bytecode::{PrimitiveKind, ScalarValue};

/// Runtime value: scalar primitive or handle into the aggregate arena.
///
/// MVP: aggregate handles are Copyable indices; arena is freed when the VM run ends.
/// See Phase 2 memory lifecycle docs — not the long-term ownership model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// Numeric / bool primitive.
    Scalar(ScalarValue),
    /// Index into the aggregate arena (`VmRuntime::aggregates`).
    Agg(u32),
}

impl Value {
    /// Returns the scalar payload or `None` for aggregates.
    #[must_use]
    pub const fn as_scalar(self) -> Option<ScalarValue> {
        match self {
            Self::Scalar(v) => Some(v),
            Self::Agg(_) => None,
        }
    }

    /// Returns the aggregate handle index or `None` for scalars.
    #[must_use]
    pub const fn as_agg(self) -> Option<u32> {
        match self {
            Self::Agg(i) => Some(i),
            Self::Scalar(_) => None,
        }
    }
}

/// Stored struct, enum, tuple, array, or slice payload in the MVP arena.
#[derive(Debug, Clone)]
pub enum Aggregate {
    /// User struct instance.
    Struct {
        /// Bytecode type table id (reserved for future layout checks).
        _type_id: u32,
        /// Field values in declaration order.
        fields: Vec<Value>,
    },
    /// User enum instance.
    Enum {
        /// Bytecode type table id (reserved for future layout checks).
        _type_id: u32,
        /// Variant discriminant.
        tag: u32,
        /// Tuple-variant payload slots (empty for unit variants).
        payload: Vec<Value>,
    },
    /// Tuple value `(T, U, …)`.
    Tuple {
        /// Element values in order.
        elems: Vec<Value>,
    },
    /// Fixed-size array `[T; N]`.
    Array {
        /// Element values in order.
        elems: Vec<Value>,
    },
    /// Slice view (`ptr`, `len`); `elem_kind` is wire [`PrimitiveKind`] or `0xFF` for aggregates.
    Slice {
        /// Element primitive wire kind, or aggregate tag `0xFF`.
        elem_kind: u8,
        /// Data pointer (heap offset, local tag, aggregate tag, or const-pool tag).
        ptr: u64,
        /// Element count.
        len: u64,
    },
    /// UTF-8 text view over module constant pool rodata.
    Str {
        /// Const-pool pointer tag + index (`PTR_CONST_TAG | index`).
        ptr: u64,
        /// Byte length (UTF-8).
        len: u64,
    },
}

/// One activation record.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Function id in the module.
    pub function_id: u32,
    /// Byte offset into the function's code slice.
    pub pc: u32,
    /// Local slots (parameters occupy `0..arity`).
    pub locals: Vec<Value>,
}

/// Reads primitive bytes from local `slot` for pointer loads.
///
/// # Errors
///
/// Returns [`crate::VmErrorKind::InvalidLocalSlot`] when `slot` is out of range or not a scalar.
pub fn local_scalar_bytes(
    frame: &Frame,
    slot: u32,
    kind: PrimitiveKind,
) -> Result<Vec<u8>, crate::VmErrorKind> {
    let idx = usize::try_from(slot).map_err(|_| crate::VmErrorKind::InvalidLocalSlot(slot))?;
    let local = frame
        .locals
        .get(idx)
        .ok_or(crate::VmErrorKind::InvalidLocalSlot(slot))?;
    let scalar = local
        .as_scalar()
        .ok_or(crate::VmErrorKind::ExpectedScalar)?;
    Ok(scalar.to_le_bytes(kind))
}

/// Writes primitive bytes into `frame` local `slot`.
pub fn store_local_scalar_bytes(
    frame: &mut Frame,
    slot: u32,
    kind: PrimitiveKind,
    bytes: &[u8],
) -> Result<(), crate::VmErrorKind> {
    let idx = usize::try_from(slot).map_err(|_| crate::VmErrorKind::InvalidLocalSlot(slot))?;
    let local = frame
        .locals
        .get_mut(idx)
        .ok_or(crate::VmErrorKind::InvalidLocalSlot(slot))?;
    let decoded =
        ScalarValue::from_le_bytes(kind, bytes).ok_or(crate::VmErrorKind::InvalidConstPayload)?;
    *local = Value::Scalar(decoded);
    Ok(())
}
