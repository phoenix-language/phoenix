//! Runtime values, aggregate payloads, and call frames for the stack interpreter.
//!
//! This module defines the **operand representation** the VM moves on the evaluation stack and in
//! local slots. Scalars are immediate [`phx_bytecode::ScalarValue`] payloads; non-Copyable Phoenix
//! values are [`Value::Agg`] handles into [`crate::VmRuntime::aggregates`].
//!
//! ## MVP memory model
//!
//! Aggregates live in a per-run arena owned by [`crate::VmRuntime`]. Handles are plain `u32`
//! indices; the arena is reclaimed when the runtime is dropped — not a tracing GC and not the
//! long-term ownership model (see `docs/design/features/vm-linear.md`).
//!
//! ## Types
//!
//! | Type | Role |
//! | --- | --- |
//! | [`Value`] | Stack/local cell: scalar primitive or aggregate handle |
//! | [`Aggregate`] | Struct, enum, tuple, array, slice view, or string view stored in the arena |
//! | [`Frame`] | One activation record: function id, program counter, and local slots |
//!
//! [`local_scalar_bytes`] and [`store_local_scalar_bytes`] bridge local slots to raw bytes for
//! pointer load/store opcodes in the interpreter.

use phx_bytecode::{PrimitiveKind, ScalarValue};

/// Runtime value: scalar primitive or handle into the aggregate arena.
///
/// Locals and stack slots use this enum uniformly. Copyable primitives are stored inline;
/// structs, enums, tuples, arrays, and other aggregate shapes use [`Self::Agg`] indices into
/// [`crate::VmRuntime::aggregates`].
///
/// MVP: aggregate handles are Copyable indices; the arena is freed when the VM run ends.
/// See Phase 2 memory lifecycle docs — not the long-term ownership model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// Numeric / bool primitive ([`phx_bytecode::ScalarValue`]).
    Scalar(ScalarValue),
    /// Index into the aggregate arena ([`crate::VmRuntime::aggregates`]).
    Agg(u32),
}

impl Value {
    /// Returns the scalar payload or `None` for aggregate handles.
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

/// Stored struct, enum, tuple, array, slice, or string payload in the MVP aggregate arena.
///
/// Constructed by aggregate opcodes (`MakeStruct`, `MakeEnum`, `MakeTuple`, …) and accessed
/// through [`Value::Agg`] handles. Slice and string variants hold tagged pointer/length pairs
/// compatible with codegen pointer conventions ([`phx_bytecode::PTR_*_TAG`]).
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

/// One activation record on the call stack.
///
/// The innermost frame in [`crate::ExecutionContext::frames`] is the currently executing
/// function. Parameters occupy local slots `0..arity`; remaining slots hold temporaries and
/// spill values lowered from the register IR.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Function id in the module function table.
    pub function_id: u32,
    /// Byte offset into the function's code slice (next instruction to decode).
    pub pc: u32,
    /// Local slots; parameters occupy indices `0..arity` per the function signature.
    pub locals: Vec<Value>,
}

/// Reads primitive bytes from local `slot` for pointer loads.
///
/// Used when a pointer opcode materializes bytes from a local holding a scalar primitive.
///
/// # Errors
///
/// Returns [`crate::VmErrorKind::InvalidLocalSlot`] when `slot` is out of range.
/// Returns [`crate::VmErrorKind::ExpectedScalar`] when the slot holds an aggregate handle.
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
///
/// Decodes `bytes` with `kind` and replaces the slot with [`Value::Scalar`].
///
/// # Errors
///
/// Returns [`crate::VmErrorKind::InvalidLocalSlot`] when `slot` is out of range.
/// Returns [`crate::VmErrorKind::InvalidConstPayload`] when `bytes` cannot be decoded for `kind`.
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
