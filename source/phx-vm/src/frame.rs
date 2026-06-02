//! Call frames, operand stack, and aggregate storage.

use phx_bytecode::{LocalLayoutTable, PrimitiveKind, ScalarValue};

/// Runtime value: scalar primitive or handle into the aggregate arena.
///
/// MVP: aggregate handles are Copyable indices; arena is freed when the VM run ends.
/// See Phase 2 memory lifecycle docs — not the long-term ownership model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// Numeric / bool primitive.
    Scalar(ScalarValue),
    /// Index into [`Machine::aggregates`].
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
        /// Bytecode type table id.
        type_id: u32,
        /// Field values in declaration order.
        fields: Vec<Value>,
    },
    /// User enum instance.
    Enum {
        /// Bytecode type table id.
        type_id: u32,
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
        /// Data pointer (heap offset, local tag, or aggregate tag).
        ptr: u64,
        /// Element count.
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

/// Operand stack + call stack + aggregate arena + linear heap for pointers.
#[derive(Debug, Default)]
pub struct Machine {
    /// Evaluation stack.
    pub stack: Vec<Value>,
    /// Innermost frame is the current function.
    pub frames: Vec<Frame>,
    /// MVP arena: all aggregates; reclaimed when `Machine` is dropped.
    pub aggregates: Vec<Aggregate>,
    /// Byte heap for `Alloc` / pointer loads (MVP; not GC).
    pub heap: Vec<u8>,
}

impl Machine {
    /// Pushes a new frame with `local_count` zero-initialized locals per layout metadata.
    pub fn push_frame(
        &mut self,
        function_id: u32,
        local_count: u16,
        layouts: &LocalLayoutTable,
    ) {
        let n = usize::from(local_count);
        let mut locals = Vec::with_capacity(n);
        if let Some(layout) = layouts.for_function(function_id) {
            for slot_kind in layout.slots.iter().take(n) {
                if slot_kind.is_aggregate() {
                    locals.push(Value::Agg(0));
                } else if let Some(kind) = slot_kind.primitive_kind() {
                    locals.push(Value::Scalar(ScalarValue::zero(kind)));
                } else {
                    locals.push(Value::Scalar(ScalarValue::zero(PrimitiveKind::S32)));
                }
            }
        }
        while locals.len() < n {
            locals.push(Value::Scalar(ScalarValue::zero(PrimitiveKind::S32)));
        }
        self.frames.push(Frame {
            function_id,
            pc: 0,
            locals,
        });
    }

    /// Pops the current frame.
    pub fn pop_frame(&mut self) -> Option<Frame> {
        self.frames.pop()
    }

    /// Appends an aggregate and returns its handle.
    pub fn push_aggregate(&mut self, agg: Aggregate) -> Value {
        let index = u32::try_from(self.aggregates.len()).unwrap_or(u32::MAX);
        self.aggregates.push(agg);
        Value::Agg(index)
    }

    /// Borrows an aggregate by handle.
    pub fn aggregate(&self, handle: u32) -> Option<&Aggregate> {
        self.aggregates.get(handle as usize)
    }

    /// Mutably borrows an aggregate by handle.
    pub fn aggregate_mut(&mut self, handle: u32) -> Option<&mut Aggregate> {
        self.aggregates.get_mut(handle as usize)
    }

    /// Allocates `size` zeroed bytes on the heap; returns the start offset.
    pub fn alloc_bytes(&mut self, size: usize) -> u64 {
        let start = self.heap.len();
        self.heap.resize(start.saturating_add(size), 0);
        u64::try_from(start).unwrap_or(u64::MAX)
    }

    /// Reads primitive bytes from local `slot` for pointer loads.
    ///
    /// # Errors
    ///
    /// Returns [`crate::VmError::InvalidLocalSlot`] when `slot` is out of range or not a scalar.
    pub fn local_scalar_bytes(
        &self,
        frame: &Frame,
        slot: u32,
        kind: PrimitiveKind,
    ) -> Result<Vec<u8>, crate::VmError> {
        let idx = usize::try_from(slot).map_err(|_| crate::VmError::InvalidLocalSlot(slot))?;
        let local = frame
            .locals
            .get(idx)
            .ok_or(crate::VmError::InvalidLocalSlot(slot))?;
        let scalar = local.as_scalar().ok_or(crate::VmError::ExpectedScalar)?;
        Ok(scalar.to_le_bytes(kind))
    }

}

/// Writes primitive bytes into `frame` local `slot`.
pub fn store_local_scalar_bytes(
    frame: &mut Frame,
    slot: u32,
    kind: PrimitiveKind,
    bytes: &[u8],
) -> Result<(), crate::VmError> {
    let idx = usize::try_from(slot).map_err(|_| crate::VmError::InvalidLocalSlot(slot))?;
    let local = frame
        .locals
        .get_mut(idx)
        .ok_or(crate::VmError::InvalidLocalSlot(slot))?;
    let decoded = ScalarValue::from_le_bytes(kind, bytes)
        .ok_or(crate::VmError::InvalidConstPayload)?;
    *local = Value::Scalar(decoded);
    Ok(())
}
