//! Synthetic std I/O park contract harness for the M:N worker pool.
//!
//! PHX-sched-7 models the runtime-transparency schedulable-I/O path without Phoenix
//! syntax or real host std I/O: a synthetic context parks on [`ParkReason::AwaitIo`],
//! registers its pending operation in [`IoWaitRegistry`], and completes on a worker thread
//! after an external readiness wakeup.
//!
//! This is a thin wrapper over [`dispatch_await_io_harness`] and
//! [`WorkerPool::spawn_await_io_on_first_run`], documenting the contract future
//! `std::io` APIs will use when the pre-scheduler bridge is retired.
//!
//! Design references: `docs/design/features/runtime-transparency.md` (schedulable I/O
//! contract), `docs/design/features/io-bridge.md` (migration target).

use std::sync::{Arc, Mutex};

use super::await_io::{AwaitIoHarnessError, AwaitIoOperands, dispatch_await_io_harness};
use super::context::{ContextId, ContextState};
use super::io_wait::{IoHandle, IoWaitError, IoWaitRegistry};
use super::park::ParkReason;
use super::worker_pool::WorkerPool;

/// Synthetic schedulable std I/O discriminant for harness stubs.
///
/// Maps to the `io_kind` operand of [`Opcode::AwaitIo`](phx_bytecode::Opcode::AwaitIo).
/// Post-MVP typed std I/O will lower to these kinds when a call may park the context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StdIoKind {
    /// Future schedulable `std::io::write_stdout` (replaces the Phase-A bridge stub).
    WriteStdout = 1,
    /// Future schedulable stdin read.
    ReadStdin = 2,
    /// Future schedulable file read/write lowering.
    File = 3,
}

impl StdIoKind {
    /// Returns the `io_kind` operand for [`AwaitIoOperands`].
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Pending synthetic std I/O operation submitted by a schedulable context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StdIoParkRequest {
    /// Schedulable std I/O discriminant.
    pub kind: StdIoKind,
    /// Index into the VM/host I/O table for this suspend site.
    pub request_id: u32,
}

impl StdIoParkRequest {
    /// Creates a harness request for `kind` with caller-assigned `request_id`.
    #[must_use]
    pub const fn new(kind: StdIoKind, request_id: u32) -> Self {
        Self { kind, request_id }
    }

    /// Returns the [`IoHandle`] derived from [`Self::request_id`].
    #[must_use]
    pub fn io_handle(self) -> IoHandle {
        IoHandle::from_index(u64::from(self.request_id))
    }

    /// Converts this request into [`AwaitIoOperands`] for harness dispatch.
    #[must_use]
    pub fn await_io_operands(self) -> AwaitIoOperands {
        AwaitIoOperands {
            io_kind: self.kind.as_u32(),
            request_id: self.request_id,
        }
    }
}

/// In-tree harness wiring [`WorkerPool`] and [`IoWaitRegistry`] for the std I/O park contract.
///
/// Use in unit tests to exercise park → register → wake → complete without real host I/O.
#[derive(Debug)]
pub struct StdIoParkHarness {
    registry: Arc<Mutex<IoWaitRegistry>>,
    pool: WorkerPool,
}

impl StdIoParkHarness {
    /// Creates a harness with `worker_count` OS threads and a fresh I/O wait registry.
    #[must_use]
    pub fn new(worker_count: usize) -> Self {
        let registry = Arc::new(Mutex::new(IoWaitRegistry::new()));
        let pool = WorkerPool::with_io_registry(worker_count, Arc::clone(&registry));
        Self { registry, pool }
    }

    /// Spawns a context that executes a synthetic schedulable std I/O op on first dequeue.
    ///
    /// The context parks with [`ParkReason::AwaitIo`] and registers `request` in the registry.
    #[must_use]
    pub fn spawn_schedulable(&self, steps: u32, request: StdIoParkRequest) -> ContextId {
        self.pool
            .spawn_await_io_on_first_run(steps, request.await_io_operands())
    }

    /// Blocks until `context` is parked for I/O.
    pub fn wait_until_parked(&self, context: ContextId) {
        self.pool.wait_until(|status| {
            status.parked_count >= 1
                && self.pool.state_of(context) == Some(ContextState::Parked(ParkReason::AwaitIo))
        });
    }

    /// Returns the lifecycle state of `context`, if it exists.
    #[must_use]
    pub fn context_state(&self, context: ContextId) -> Option<ContextState> {
        self.pool.state_of(context)
    }

    /// Returns whether `request` is registered to a parked context.
    #[must_use]
    pub fn is_registered(&self, request: StdIoParkRequest) -> bool {
        self.lock_registry().is_registered(request.io_handle())
    }

    /// Returns the context registered for `request`, if any.
    #[must_use]
    pub fn registered_context(&self, request: StdIoParkRequest) -> Option<ContextId> {
        self.lock_registry().context_for(request.io_handle())
    }

    /// Models host I/O readiness: resumes the parked context and unregisters the handle.
    ///
    /// # Errors
    ///
    /// Returns [`IoWaitError`] when the handle is unknown or resume fails.
    pub fn signal_host_ready(
        &mut self,
        request: StdIoParkRequest,
    ) -> Result<ContextId, IoWaitError> {
        let registry = Arc::clone(&self.registry);
        let handle = request.io_handle();
        let mut reg = registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reg.signal_ready(handle, &mut self.pool)
    }

    /// Blocks until every spawned context has reached [`ContextState::Done`].
    pub fn wait_all_done(&self) {
        self.pool.wait_all_done();
    }

    /// Returns the number of pending I/O registrations.
    #[must_use]
    pub fn pending_io_count(&self) -> usize {
        self.lock_registry().pending_count()
    }

    /// Shuts down worker threads. Call after tests finish waiting on contexts.
    pub fn shutdown(self) {
        self.pool.shutdown();
    }

    fn lock_registry(&self) -> std::sync::MutexGuard<'_, IoWaitRegistry> {
        self.registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Park a running context for synthetic std I/O and register the pending handle.
///
/// Harness-only entry point mirroring VM dispatch when bytecode hits
/// [`Opcode::AwaitIo`](phx_bytecode::Opcode::AwaitIo) for a schedulable std call.
///
/// # Errors
///
/// Returns [`AwaitIoHarnessError`] when park or registry registration fails.
pub fn park_std_io_synthetic(
    pool: &WorkerPool,
    registry: &mut IoWaitRegistry,
    context: ContextId,
    request: StdIoParkRequest,
) -> Result<(), AwaitIoHarnessError> {
    dispatch_await_io_harness(pool, registry, context, request.await_io_operands())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn std_io_park_register_wake_complete_on_worker() {
        let request = StdIoParkRequest::new(StdIoKind::WriteStdout, 7);
        let mut harness = StdIoParkHarness::new(2);
        let context = harness.spawn_schedulable(3, request);

        harness.wait_until_parked(context);

        assert_eq!(
            harness.context_state(context),
            Some(ContextState::Parked(ParkReason::AwaitIo))
        );
        assert_eq!(harness.pending_io_count(), 1);
        assert_eq!(harness.registered_context(request), Some(context));
        assert!(harness.is_registered(request));

        let resumed = harness
            .signal_host_ready(request)
            .expect("host readiness wakeup");
        assert_eq!(resumed, context);
        assert_eq!(harness.pending_io_count(), 0);
        assert!(!harness.is_registered(request));

        harness.wait_all_done();

        assert_eq!(harness.context_state(context), Some(ContextState::Done));
        harness.shutdown();
    }

    #[test]
    fn std_io_request_maps_to_await_io_operands() {
        let request = StdIoParkRequest::new(StdIoKind::ReadStdin, 42);
        let ops = request.await_io_operands();
        assert_eq!(ops.io_kind, StdIoKind::ReadStdin.as_u32());
        assert_eq!(ops.request_id, 42);
        assert_eq!(ops.io_handle(), request.io_handle());
    }
}
