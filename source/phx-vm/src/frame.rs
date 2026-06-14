//! Operand stack, call stack, aggregate arena, and linear heap for pointers.

use std::collections::BTreeMap;

use phx_bytecode::{
    LocalLayoutTable, PTR_AGG_TAG, PTR_CONST_TAG, PTR_FN_TAG, PTR_LOCAL_TAG, PrimitiveKind,
    ScalarValue,
};

use crate::VmError;

/// Runtime value: scalar primitive or handle into the aggregate arena.
///
/// MVP: aggregate handles are Copyable indices; arena is freed when the VM run ends.
/// See Phase 2 memory lifecycle docs — not the long-term ownership model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// Numeric / bool primitive.
    Scalar(ScalarValue),
    /// Index into the aggregate arena (`Machine::aggregates`).
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

/// Default byte cap for the MVP VM linear heap (`64` MiB).
pub const DEFAULT_HEAP_CAP_BYTES: usize = 64 * 1024 * 1024;

/// Operand stack + call stack + aggregate arena + linear heap for pointers.
#[derive(Debug)]
pub struct Machine {
    /// Evaluation stack.
    pub stack: Vec<Value>,
    /// Innermost frame is the current function.
    pub frames: Vec<Frame>,
    /// MVP arena: all aggregates; reclaimed when `Machine` is dropped.
    pub aggregates: Vec<Aggregate>,
    /// Byte heap for `Alloc` / pointer loads (MVP; not GC).
    pub heap: Vec<u8>,
    /// Live `(ptr, size)` blocks registered by `Alloc` and removed by `Free`.
    pub live_heap_blocks: BTreeMap<u64, u32>,
    /// Maximum `heap.len()` after any successful `alloc_bytes`.
    pub heap_cap: usize,
    /// When true, heap loads/stores validate against [`Self::live_heap_blocks`].
    pub heap_check_enabled: bool,
}

impl Default for Machine {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            frames: Vec::new(),
            aggregates: Vec::new(),
            heap: Vec::new(),
            live_heap_blocks: BTreeMap::new(),
            heap_cap: DEFAULT_HEAP_CAP_BYTES,
            heap_check_enabled: true,
        }
    }
}

impl Machine {
    /// Builds a machine with a custom heap byte cap (for tests and future CLI wiring).
    #[must_use]
    pub fn with_heap_cap(heap_cap: usize) -> Self {
        Self {
            heap_cap,
            ..Self::default()
        }
    }

    /// Builds a machine with heap use-after-free checking enabled or disabled.
    #[must_use]
    pub fn with_heap_checking(heap_check_enabled: bool) -> Self {
        Self {
            heap_check_enabled,
            ..Self::default()
        }
    }

    /// Returns `Ok(())` when `addr..addr+len` lies fully inside a live ledger block.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::HeapOutOfBounds`] when the range extends past the physical heap.
    /// Returns [`VmError::UseAfterFree`] when checking is enabled and the range is not live.
    pub fn validate_live_heap_access(&self, addr: usize, len: usize) -> Result<(), VmError> {
        let end = addr.checked_add(len).ok_or(VmError::HeapOutOfBounds)?;
        if end > self.heap.len() {
            return Err(VmError::HeapOutOfBounds);
        }
        if !self.heap_check_enabled {
            return Ok(());
        }
        let ptr_key = u64::try_from(addr).map_err(|_| VmError::UseAfterFree)?;
        if let Some((&ptr, &size)) = self.live_heap_blocks.range(..=ptr_key).next_back() {
            let start = usize::try_from(ptr).map_err(|_| VmError::UseAfterFree)?;
            let block_end = start
                .checked_add(usize::try_from(size).map_err(|_| VmError::UseAfterFree)?)
                .ok_or(VmError::UseAfterFree)?;
            if addr >= start && end <= block_end {
                return Ok(());
            }
        }
        Err(VmError::UseAfterFree)
    }

    /// Pushes a new frame with `local_count` zero-initialized locals per layout metadata.
    pub fn push_frame(&mut self, function_id: u32, local_count: u16, layouts: &LocalLayoutTable) {
        let n = usize::from(local_count);
        let mut locals = Vec::with_capacity(n);
        if let Some(layout) = layouts.for_function(function_id) {
            for slot_kind in layout.slots.iter().take(n) {
                if slot_kind.is_aggregate() {
                    locals.push(Value::Agg(0));
                } else if slot_kind.is_fn_ptr() {
                    locals.push(Value::Scalar(ScalarValue::Ptr(0)));
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
    #[must_use]
    pub fn aggregate(&self, handle: u32) -> Option<&Aggregate> {
        self.aggregates.get(handle as usize)
    }

    /// Mutably borrows an aggregate by handle.
    pub fn aggregate_mut(&mut self, handle: u32) -> Option<&mut Aggregate> {
        self.aggregates.get_mut(handle as usize)
    }

    /// Allocates `size` zeroed bytes on the heap; returns the start offset.
    ///
    /// # Errors
    ///
    /// Returns [`VmError::OutOfMemory`] when `size` or cumulative heap growth would exceed
    /// [`Self::heap_cap`], or when the start offset does not fit in `u64`.
    pub fn alloc_bytes(&mut self, size: usize) -> Result<u64, VmError> {
        let start = self.heap.len();
        let new_len = start.checked_add(size).ok_or(VmError::OutOfMemory)?;
        if new_len > self.heap_cap {
            return Err(VmError::OutOfMemory);
        }
        self.heap.resize(new_len, 0);
        let ptr = u64::try_from(start).map_err(|_| VmError::OutOfMemory)?;
        if let Ok(reg_size) = u32::try_from(size) {
            self.live_heap_blocks.insert(ptr, reg_size);
        }
        Ok(ptr)
    }

    /// Returns the number of live heap blocks tracked by the allocation ledger.
    #[must_use]
    pub fn live_heap_block_count(&self) -> usize {
        self.live_heap_blocks.len()
    }

    /// Frees a heap block previously returned by [`Self::alloc_bytes`].
    ///
    /// # Errors
    ///
    /// Returns [`VmError::DoubleFree`] when `(ptr, size)` is not in the ledger.
    /// Returns [`VmError::InvalidFree`] for tagged pointers, out-of-bounds ranges, or size mismatch.
    pub fn free_bytes(&mut self, ptr: u64, size: u32) -> Result<(), VmError> {
        if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG
            || ptr & PTR_AGG_TAG == PTR_AGG_TAG
            || ptr & PTR_CONST_TAG == PTR_CONST_TAG
            || ptr & PTR_FN_TAG == PTR_FN_TAG
        {
            return Err(VmError::InvalidFree);
        }
        let addr = usize::try_from(ptr).map_err(|_| VmError::InvalidFree)?;
        let byte_len = usize::try_from(size).map_err(|_| VmError::InvalidFree)?;
        let end = addr.checked_add(byte_len).ok_or(VmError::InvalidFree)?;
        if end > self.heap.len() {
            return Err(VmError::InvalidFree);
        }
        match self.live_heap_blocks.remove(&ptr) {
            Some(registered) if registered == size => {}
            Some(_) => return Err(VmError::InvalidFree),
            None => return Err(VmError::DoubleFree),
        }
        self.heap[addr..end].fill(0);
        Ok(())
    }
}

/// Reads primitive bytes from local `slot` for pointer loads.
///
/// # Errors
///
/// Returns [`crate::VmError::InvalidLocalSlot`] when `slot` is out of range or not a scalar.
pub fn local_scalar_bytes(
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
    let decoded =
        ScalarValue::from_le_bytes(kind, bytes).ok_or(crate::VmError::InvalidConstPayload)?;
    *local = Value::Scalar(decoded);
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::Machine;
    use crate::VmError;

    #[test]
    fn alloc_then_free_clears_ledger() {
        let mut machine = Machine::default();
        let ptr = machine.alloc_bytes(8).expect("alloc");
        assert_eq!(machine.live_heap_block_count(), 1);
        assert!(machine.free_bytes(ptr, 8).is_ok());
        assert_eq!(machine.live_heap_block_count(), 0);
    }

    #[test]
    fn double_free_returns_error() {
        let mut machine = Machine::default();
        let ptr = machine.alloc_bytes(4).expect("alloc");
        assert!(machine.free_bytes(ptr, 4).is_ok());
        let err = machine.free_bytes(ptr, 4);
        assert_eq!(err, Err(VmError::DoubleFree));
    }

    #[test]
    fn wrong_size_free_returns_invalid_free() {
        let mut machine = Machine::default();
        let ptr = machine.alloc_bytes(4).expect("alloc");
        let err = machine.free_bytes(ptr, 8);
        assert_eq!(err, Err(VmError::InvalidFree));
    }

    #[test]
    fn alloc_exceeding_cap_returns_out_of_memory() {
        let mut machine = Machine::with_heap_cap(8);
        assert_eq!(machine.alloc_bytes(16), Err(VmError::OutOfMemory));
        assert_eq!(machine.heap.len(), 0);
    }

    #[test]
    fn cumulative_alloc_hits_cap() {
        let mut machine = Machine::with_heap_cap(16);
        assert_eq!(machine.alloc_bytes(8).expect("first"), 0);
        assert_eq!(machine.alloc_bytes(8).expect("second"), 8);
        assert_eq!(machine.alloc_bytes(1), Err(VmError::OutOfMemory));
        assert_eq!(machine.heap.len(), 16);
    }

    #[test]
    fn live_heap_access_allowed() {
        let mut machine = Machine::default();
        let ptr = machine.alloc_bytes(4).expect("alloc");
        let addr = usize::try_from(ptr).expect("ptr fits usize");
        assert!(machine.validate_live_heap_access(addr, 1).is_ok());
        assert!(machine.validate_live_heap_access(addr, 4).is_ok());
    }

    #[test]
    fn freed_heap_access_returns_use_after_free() {
        let mut machine = Machine::default();
        let ptr = machine.alloc_bytes(4).expect("alloc");
        let addr = usize::try_from(ptr).expect("ptr fits usize");
        machine.free_bytes(ptr, 4).expect("free");
        assert_eq!(
            machine.validate_live_heap_access(addr, 1),
            Err(VmError::UseAfterFree)
        );
    }

    #[test]
    fn partial_past_block_end_returns_use_after_free() {
        let mut machine = Machine::default();
        let ptr = machine.alloc_bytes(4).expect("first alloc");
        let _second = machine.alloc_bytes(4).expect("second alloc");
        let addr = usize::try_from(ptr).expect("ptr fits usize");
        assert_eq!(
            machine.validate_live_heap_access(addr, 5),
            Err(VmError::UseAfterFree)
        );
    }

    #[test]
    fn heap_check_disabled_allows_freed_access() {
        let mut machine = Machine::with_heap_checking(false);
        let ptr = machine.alloc_bytes(4).expect("alloc");
        let addr = usize::try_from(ptr).expect("ptr fits usize");
        machine.free_bytes(ptr, 4).expect("free");
        assert!(machine.validate_live_heap_access(addr, 1).is_ok());
    }
}
