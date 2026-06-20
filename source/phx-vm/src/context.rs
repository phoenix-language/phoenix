//! Per-context execution state and shared VM runtime resources.
//!
//! The interpreter splits **schedulable context** ([`ExecutionContext`]: operand stack and call
//! frames) from **run-wide resources** ([`VmRuntime`]: linear heap, aggregate arena, allocation
//! ledger). [`Machine`] combines both for foreign stubs and integration tests.
//!
//! MVP runs one [`ExecutionContext`] on one [`VmRuntime`] per [`crate::run`] call. Post-MVP the
//! scheduler owns a pool of contexts sharing runtime services; see
//! `docs/design/features/vm-linear.md`.
//!
//! ## Layout
//!
//! | Type | Owns |
//! | --- | --- |
//! | [`ExecutionContext`] | Evaluation stack and call stack ([`crate::frame::Frame`] chain) |
//! | [`VmRuntime`] | Aggregate arena, byte heap, live-allocation ledger, heap cap |
//! | [`Machine`] | Facade exposing `ctx` + `runtime` to embedders and test harnesses |
//!
//! ## Heap ledger
//!
//! [`VmRuntime::alloc_bytes`] appends zeroed bytes and registers `(ptr, size)` in
//! [`VmRuntime::live_heap_blocks`]. [`VmRuntime::free_bytes`] removes the entry and zero-fills the
//! range. When [`VmRuntime::heap_check_enabled`] is true, pointer loads/stores validate against
//! live blocks via [`VmRuntime::validate_live_heap_access`].

use std::collections::BTreeMap;

use phx_bytecode::{
    LocalLayoutTable, PTR_AGG_TAG, PTR_CONST_TAG, PTR_FN_TAG, PTR_LOCAL_TAG, PrimitiveKind,
    ScalarValue,
};

use crate::VmErrorKind;
use crate::frame::{Aggregate, Frame, Value};

/// Default byte cap for the MVP VM linear heap (`64` MiB).
pub const DEFAULT_HEAP_CAP_BYTES: usize = 64 * 1024 * 1024;

/// Per schedulable unit: operand stack and call stack.
///
/// Holds transient evaluation state for one logical thread of execution. The innermost
/// [`Frame`](crate::frame::Frame) in [`Self::frames`] is the active function; [`Self::stack`]
/// holds pending operands for the current instruction stream.
///
/// MVP: one context per [`crate::run`] invocation.
#[derive(Debug, Default)]
pub struct ExecutionContext {
    /// Evaluation stack (operands for the current frame).
    pub stack: Vec<Value>,
    /// Call stack; the last element is the innermost (currently executing) frame.
    pub frames: Vec<Frame>,
}

impl ExecutionContext {
    /// Pushes a new frame with `local_count` zero-initialized locals.
    ///
    /// When `layouts` contains an entry for `function_id`, each slot is initialized from
    /// [`phx_bytecode::LocalLayoutTable`] metadata (aggregate handles, fn pointers, or zeroed
    /// primitives). Missing layout entries default remaining slots to zeroed `S32`.
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

    /// Pops and returns the innermost frame, or `None` when the call stack is empty.
    pub fn pop_frame(&mut self) -> Option<Frame> {
        self.frames.pop()
    }
}

/// Shared resources for one VM run.
///
/// Owns memory that outlives individual frames: the aggregate arena, the linear byte heap, and
/// the allocation ledger used for use-after-free detection. Dropped when the enclosing
/// [`Machine`] or interpreter run completes.
#[derive(Debug)]
pub struct VmRuntime {
    /// MVP arena: all aggregates; reclaimed when the runtime is dropped.
    pub aggregates: Vec<Aggregate>,
    /// Byte heap for `Alloc` / pointer loads (MVP linear allocator; not GC).
    pub heap: Vec<u8>,
    /// Live `(ptr, size)` blocks registered by [`Self::alloc_bytes`] and removed by [`Self::free_bytes`].
    pub live_heap_blocks: BTreeMap<u64, u32>,
    /// Maximum `heap.len()` after any successful [`Self::alloc_bytes`].
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

    /// Appends an aggregate to the arena and returns its handle as [`Value::Agg`].
    pub fn push_aggregate(&mut self, agg: Aggregate) -> Value {
        let index = u32::try_from(self.aggregates.len()).unwrap_or(u32::MAX);
        self.aggregates.push(agg);
        Value::Agg(index)
    }

    /// Borrows an aggregate by handle, or `None` when the index is out of range.
    #[must_use]
    pub fn aggregate(&self, handle: u32) -> Option<&Aggregate> {
        self.aggregates.get(handle as usize)
    }

    /// Mutably borrows an aggregate by handle, or `None` when the index is out of range.
    pub fn aggregate_mut(&mut self, handle: u32) -> Option<&mut Aggregate> {
        self.aggregates.get_mut(handle as usize)
    }

    /// Allocates `size` zeroed bytes on the heap; returns the start offset.
    ///
    /// Registers the block in [`Self::live_heap_blocks`] when `size` fits in `u32`.
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
    /// Removes the ledger entry and zero-fills the byte range. Tagged pointers (local, aggregate,
    /// const-pool, fn) are rejected.
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
/// [`Self::ctx`] and [`Self::runtime`] directly. Convenience methods delegate heap and aggregate
/// operations to [`Self::runtime`].
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

    /// Mutable reference to the operand stack (convenience for foreign stubs).
    pub fn stack(&mut self) -> &mut Vec<Value> {
        &mut self.ctx.stack
    }

    /// Pushes a new frame; see [`ExecutionContext::push_frame`].
    pub fn push_frame(&mut self, function_id: u32, local_count: u16, layouts: &LocalLayoutTable) {
        self.ctx.push_frame(function_id, local_count, layouts);
    }

    /// Pops the current frame; see [`ExecutionContext::pop_frame`].
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

    /// Appends an aggregate and returns its handle; see [`VmRuntime::push_aggregate`].
    pub fn push_aggregate(&mut self, agg: Aggregate) -> Value {
        self.runtime.push_aggregate(agg)
    }

    /// Borrows an aggregate by handle; see [`VmRuntime::aggregate`].
    #[must_use]
    pub fn aggregate(&self, handle: u32) -> Option<&Aggregate> {
        self.runtime.aggregate(handle)
    }

    /// Mutably borrows an aggregate by handle; see [`VmRuntime::aggregate_mut`].
    pub fn aggregate_mut(&mut self, handle: u32) -> Option<&mut Aggregate> {
        self.runtime.aggregate_mut(handle)
    }

    /// Allocates `size` zeroed bytes on the heap; see [`VmRuntime::alloc_bytes`].
    ///
    /// # Errors
    ///
    /// Returns [`VmErrorKind::OutOfMemory`] when the cap would be exceeded.
    pub fn alloc_bytes(&mut self, size: usize) -> Result<u64, VmErrorKind> {
        self.runtime.alloc_bytes(size)
    }

    /// Returns the number of live heap blocks; see [`VmRuntime::live_heap_block_count`].
    #[must_use]
    pub fn live_heap_block_count(&self) -> usize {
        self.runtime.live_heap_block_count()
    }

    /// Frees a heap block; see [`VmRuntime::free_bytes`].
    ///
    /// # Errors
    ///
    /// Returns [`VmErrorKind::DoubleFree`] or [`VmErrorKind::InvalidFree`] on misuse.
    pub fn free_bytes(&mut self, ptr: u64, size: u32) -> Result<(), VmErrorKind> {
        self.runtime.free_bytes(ptr, size)
    }
}

#[cfg(test)]
mod tests {
    use super::Machine;
    use crate::VmErrorKind;

    fn alloc_ok(machine: &mut Machine, size: usize) -> u64 {
        match machine.alloc_bytes(size) {
            Ok(ptr) => ptr,
            Err(kind) => panic!("alloc_bytes({size}): {kind:?}"),
        }
    }

    fn free_ok(machine: &mut Machine, ptr: u64, size: u32) {
        if let Err(kind) = machine.free_bytes(ptr, size) {
            panic!("free_bytes({ptr}, {size}): {kind:?}");
        }
    }

    fn ptr_usize(ptr: u64) -> usize {
        match usize::try_from(ptr) {
            Ok(addr) => addr,
            Err(err) => panic!("ptr {ptr} does not fit usize: {err}"),
        }
    }

    #[test]
    fn alloc_then_free_clears_ledger() {
        let mut machine = Machine::default();
        let ptr = alloc_ok(&mut machine, 8);
        assert_eq!(machine.live_heap_block_count(), 1);
        assert!(machine.free_bytes(ptr, 8).is_ok());
        assert_eq!(machine.live_heap_block_count(), 0);
    }

    #[test]
    fn double_free_returns_error() {
        let mut machine = Machine::default();
        let ptr = alloc_ok(&mut machine, 4);
        assert!(machine.free_bytes(ptr, 4).is_ok());
        let err = machine.free_bytes(ptr, 4);
        assert_eq!(err, Err(VmErrorKind::DoubleFree));
    }

    #[test]
    fn wrong_size_free_returns_invalid_free() {
        let mut machine = Machine::default();
        let ptr = alloc_ok(&mut machine, 4);
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
        assert_eq!(alloc_ok(&mut machine, 8), 0);
        assert_eq!(alloc_ok(&mut machine, 8), 8);
        assert_eq!(machine.alloc_bytes(1), Err(VmErrorKind::OutOfMemory));
        assert_eq!(machine.runtime.heap.len(), 16);
    }

    #[test]
    fn live_heap_access_allowed() {
        let mut machine = Machine::default();
        let ptr = alloc_ok(&mut machine, 4);
        let addr = ptr_usize(ptr);
        assert!(machine.validate_live_heap_access(addr, 1).is_ok());
        assert!(machine.validate_live_heap_access(addr, 4).is_ok());
    }

    #[test]
    fn freed_heap_access_returns_use_after_free() {
        let mut machine = Machine::default();
        let ptr = alloc_ok(&mut machine, 4);
        let addr = ptr_usize(ptr);
        free_ok(&mut machine, ptr, 4);
        assert_eq!(
            machine.validate_live_heap_access(addr, 1),
            Err(VmErrorKind::UseAfterFree)
        );
    }

    #[test]
    fn partial_past_block_end_returns_use_after_free() {
        let mut machine = Machine::default();
        let ptr = alloc_ok(&mut machine, 4);
        let _second = alloc_ok(&mut machine, 4);
        let addr = ptr_usize(ptr);
        assert_eq!(
            machine.validate_live_heap_access(addr, 5),
            Err(VmErrorKind::UseAfterFree)
        );
    }

    #[test]
    fn heap_check_disabled_allows_freed_access() {
        let mut machine = Machine::with_heap_checking(false);
        let ptr = alloc_ok(&mut machine, 4);
        let addr = ptr_usize(ptr);
        free_ok(&mut machine, ptr, 4);
        assert!(machine.validate_live_heap_access(addr, 1).is_ok());
    }
}
