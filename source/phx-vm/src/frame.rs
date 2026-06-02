//! Call frames, operand stack, and aggregate storage.

use phx_bytecode::ScalarValue;

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

/// Stored struct, enum, tuple, or array payload in the MVP arena.
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
    /// Pushes a new frame with `local_count` zeroed locals.
    pub fn push_frame(&mut self, function_id: u32, local_count: u16) {
        let n = usize::from(local_count);
        self.frames.push(Frame {
            function_id,
            pc: 0,
            locals: vec![Value::Scalar(ScalarValue::zero_int()); n],
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

    /// Allocates `size` zeroed bytes on the heap; returns the start offset as `i64`.
    pub fn alloc_bytes(&mut self, size: usize) -> i64 {
        let start = self.heap.len();
        self.heap.resize(start.saturating_add(size), 0);
        i64::try_from(start).unwrap_or(i64::MAX)
    }
}
