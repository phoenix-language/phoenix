//! Per-context execution state and shared VM runtime resources.
//!
//! MVP runs one [`ExecutionContext`] on one [`VmRuntime`] per `run()` call.
//! Post-MVP the scheduler owns a pool of contexts; see `docs/design/features/vm-linear.md`.

use std::collections::BTreeMap;

use phx_bytecode::{
    LocalLayoutTable, PTR_AGG_TAG, PTR_CONST_TAG, PTR_FN_TAG, PTR_LOCAL_TAG, PrimitiveKind,
    ScalarValue,
};

use crate::VmErrorKind;
use crate::frame::{Aggregate, Frame, Value};

/// Default byte cap for the MVP VM linear heap (`64` MiB).
pub const DEFAULT_HEAP_CAP_BYTES: usize = 64 * 1024 * 1024;

/// Per schedulable unit: operand stack + call stack (MVP: one per `run()`).
#[derive(Debug, Default)]
pub struct ExecutionContext {
    /// Evaluation stack.
    pub stack: Vec<Value>,
    /// Innermost frame is the current function.
    pub frames: Vec<Frame>,
}

impl ExecutionContext {
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
}

/// Shared resources for one VM run (MVP: owned alongside a single context).
#[derive(Debug)]
pub struct VmRuntime {
    /// MVP arena: all aggregates; reclaimed when the runtime is dropped.
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

impl Default for VmRuntime {
    fn default() -> Self {
        Self {
            aggregates: Vec::new(),
            heap: Vec::new(),
            live_heap_blocks: BTreeMap::new(),
            heap_cap: DEFAULT_HEAP_CAP_BYTES,
            heap_check_enabled: true,
        }
    }
}

impl VmRuntime {
    /// Builds a runtime with a custom heap byte cap (for tests and future CLI wiring).
    #[must_use]
    pub fn with_heap_cap(heap_cap: usize) -> Self {
        Self {
            heap_cap,
            ..Self::default()
        }
    }

    /// Builds a runtime with heap use-after-free checking enabled or disabled.
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
    /// Returns [`VmErrorKind::HeapOutOfBounds`] when the range extends past the physical heap.
    /// Returns [`VmErrorKind::UseAfterFree`] when checking is enabled and the range is not live.
    pub fn validate_live_heap_access(&self, addr: usize, len: usize) -> Result<(), VmErrorKind> {
        let end = addr.checked_add(len).ok_or(VmErrorKind::HeapOutOfBounds)?;
        if end > self.heap.len() {
            return Err(VmErrorKind::HeapOutOfBounds);
        }
        if !self.heap_check_enabled {
            return Ok(());
        }
        let ptr_key = u64::try_from(addr).map_err(|_| VmErrorKind::UseAfterFree)?;
        if let Some((&ptr, &size)) = self.live_heap_blocks.range(..=ptr_key).next_back() {
            let start = usize::try_from(ptr).map_err(|_| VmErrorKind::UseAfterFree)?;
            let block_end = start
                .checked_add(usize::try_from(size).map_err(|_| VmErrorKind::UseAfterFree)?)
                .ok_or(VmErrorKind::UseAfterFree)?;
            if addr >= start && end <= block_end {
                return Ok(());
            }
        }
        Err(VmErrorKind::UseAfterFree)
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
    /// Returns [`VmErrorKind::OutOfMemory`] when `size` or cumulative heap growth would exceed
    /// [`Self::heap_cap`], or when the start offset does not fit in `u64`.
    pub fn alloc_bytes(&mut self, size: usize) -> Result<u64, VmErrorKind> {
        let start = self.heap.len();
        let new_len = start.checked_add(size).ok_or(VmErrorKind::OutOfMemory)?;
        if new_len > self.heap_cap {
            return Err(VmErrorKind::OutOfMemory);
        }
        self.heap.resize(new_len, 0);
        let ptr = u64::try_from(start).map_err(|_| VmErrorKind::OutOfMemory)?;
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
    /// Returns [`VmErrorKind::DoubleFree`] when `(ptr, size)` is not in the ledger.
    /// Returns [`VmErrorKind::InvalidFree`] for tagged pointers, out-of-bounds ranges, or size mismatch.
    pub fn free_bytes(&mut self, ptr: u64, size: u32) -> Result<(), VmErrorKind> {
        if ptr & PTR_LOCAL_TAG == PTR_LOCAL_TAG
            || ptr & PTR_AGG_TAG == PTR_AGG_TAG
            || ptr & PTR_CONST_TAG == PTR_CONST_TAG
            || ptr & PTR_FN_TAG == PTR_FN_TAG
        {
            return Err(VmErrorKind::InvalidFree);
        }
        let addr = usize::try_from(ptr).map_err(|_| VmErrorKind::InvalidFree)?;
        let byte_len = usize::try_from(size).map_err(|_| VmErrorKind::InvalidFree)?;
        let end = addr.checked_add(byte_len).ok_or(VmErrorKind::InvalidFree)?;
        if end > self.heap.len() {
            return Err(VmErrorKind::InvalidFree);
        }
        match self.live_heap_blocks.remove(&ptr) {
            Some(registered) if registered == size => {}
            Some(_) => return Err(VmErrorKind::InvalidFree),
            None => return Err(VmErrorKind::DoubleFree),
        }
        self.heap[addr..end].fill(0);
        Ok(())
    }
}

/// Facade combining one [`ExecutionContext`] and one [`VmRuntime`] for MVP execution.
///
/// Foreign stubs and integration tests use this type; the interpreter accesses
/// [`Self::ctx`] and [`Self::runtime`] directly.
#[derive(Debug, Default)]
pub struct Machine {
    /// Per-context stack and call frames.
    pub ctx: ExecutionContext,
    /// Shared heap, aggregate arena, and allocation ledger.
    pub runtime: VmRuntime,
}

impl Machine {
    /// Builds a machine with a custom heap byte cap (for tests and future CLI wiring).
    #[must_use]
    pub fn with_heap_cap(heap_cap: usize) -> Self {
        Self {
            runtime: VmRuntime::with_heap_cap(heap_cap),
            ..Self::default()
        }
    }

    /// Builds a machine with heap use-after-free checking enabled or disabled.
    #[must_use]
    pub fn with_heap_checking(heap_check_enabled: bool) -> Self {
        Self {
            runtime: VmRuntime::with_heap_checking(heap_check_enabled),
            ..Self::default()
        }
    }

    /// Operand stack (convenience for foreign stubs).
    pub fn stack(&mut self) -> &mut Vec<Value> {
        &mut self.ctx.stack
    }

    /// Pushes a new frame with `local_count` zero-initialized locals per layout metadata.
    pub fn push_frame(&mut self, function_id: u32, local_count: u16, layouts: &LocalLayoutTable) {
        self.ctx.push_frame(function_id, local_count, layouts);
    }

    /// Pops the current frame.
    pub fn pop_frame(&mut self) -> Option<Frame> {
        self.ctx.pop_frame()
    }

    /// Returns `Ok(())` when `addr..addr+len` lies fully inside a live ledger block.
    ///
    /// # Errors
    ///
    /// Returns [`VmErrorKind::HeapOutOfBounds`] or [`VmErrorKind::UseAfterFree`] on invalid access.
    pub fn validate_live_heap_access(&self, addr: usize, len: usize) -> Result<(), VmErrorKind> {
        self.runtime.validate_live_heap_access(addr, len)
    }

    /// Appends an aggregate and returns its handle.
    pub fn push_aggregate(&mut self, agg: Aggregate) -> Value {
        self.runtime.push_aggregate(agg)
    }

    /// Borrows an aggregate by handle.
    #[must_use]
    pub fn aggregate(&self, handle: u32) -> Option<&Aggregate> {
        self.runtime.aggregate(handle)
    }

    /// Mutably borrows an aggregate by handle.
    pub fn aggregate_mut(&mut self, handle: u32) -> Option<&mut Aggregate> {
        self.runtime.aggregate_mut(handle)
    }

    /// Allocates `size` zeroed bytes on the heap; returns the start offset.
    ///
    /// # Errors
    ///
    /// Returns [`VmErrorKind::OutOfMemory`] when the cap would be exceeded.
    pub fn alloc_bytes(&mut self, size: usize) -> Result<u64, VmErrorKind> {
        self.runtime.alloc_bytes(size)
    }

    /// Returns the number of live heap blocks tracked by the allocation ledger.
    #[must_use]
    pub fn live_heap_block_count(&self) -> usize {
        self.runtime.live_heap_block_count()
    }

    /// Frees a heap block previously returned by [`Self::alloc_bytes`].
    ///
    /// # Errors
    ///
    /// Returns [`VmErrorKind::DoubleFree`] or [`VmErrorKind::InvalidFree`] on misuse.
    pub fn free_bytes(&mut self, ptr: u64, size: u32) -> Result<(), VmErrorKind> {
        self.runtime.free_bytes(ptr, size)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::Machine;
    use crate::VmErrorKind;

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
        assert_eq!(err, Err(VmErrorKind::DoubleFree));
    }

    #[test]
    fn wrong_size_free_returns_invalid_free() {
        let mut machine = Machine::default();
        let ptr = machine.alloc_bytes(4).expect("alloc");
        let err = machine.free_bytes(ptr, 8);
        assert_eq!(err, Err(VmErrorKind::InvalidFree));
    }

    #[test]
    fn alloc_exceeding_cap_returns_out_of_memory() {
        let mut machine = Machine::with_heap_cap(8);
        assert_eq!(machine.alloc_bytes(16), Err(VmErrorKind::OutOfMemory));
        assert_eq!(machine.runtime.heap.len(), 0);
    }

    #[test]
    fn cumulative_alloc_hits_cap() {
        let mut machine = Machine::with_heap_cap(16);
        assert_eq!(machine.alloc_bytes(8).expect("first"), 0);
        assert_eq!(machine.alloc_bytes(8).expect("second"), 8);
        assert_eq!(machine.alloc_bytes(1), Err(VmErrorKind::OutOfMemory));
        assert_eq!(machine.runtime.heap.len(), 16);
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
            Err(VmErrorKind::UseAfterFree)
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
            Err(VmErrorKind::UseAfterFree)
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
