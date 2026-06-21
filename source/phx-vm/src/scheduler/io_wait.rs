//! I/O wait registry for contexts parked with [`ParkReason::AwaitIo`].
//!
//! When schedulable I/O would block a worker thread, the VM parks the current context and
//! registers the pending operation here. Host readiness (epoll / kqueue / IOCP or equivalent)
//! later calls [`IoWaitRegistry::signal_ready`], which resumes the context on the scheduler.
//!
//! Design reference: `docs/design/features/runtime-transparency.md` (schedulable I/O contract).

use std::collections::HashMap;

use super::context::{ContextId, ContextState};
use super::harness::{SchedulerError, SingleThreadScheduler};
use super::park::ParkReason;
use super::worker_pool::WorkerPool;

/// Scheduler surface for [`IoWaitRegistry::register`] state checks.
pub trait IoWaitState {
    /// Returns the lifecycle state of `id`, if it exists.
    fn io_wait_state_of(&self, id: ContextId) -> Option<ContextState>;
}

/// Scheduler surface for [`IoWaitRegistry::signal_ready`] resume.
pub trait IoWaitWake {
    /// Moves a parked context back to the runnable queue.
    fn io_wait_resume(&mut self, id: ContextId) -> Result<(), SchedulerError>;
}

impl IoWaitState for SingleThreadScheduler {
    fn io_wait_state_of(&self, id: ContextId) -> Option<ContextState> {
        self.state_of(id)
    }
}

impl IoWaitWake for SingleThreadScheduler {
    fn io_wait_resume(&mut self, id: ContextId) -> Result<(), SchedulerError> {
        self.resume(id)
    }
}

impl IoWaitState for WorkerPool {
    fn io_wait_state_of(&self, id: ContextId) -> Option<ContextState> {
        self.state_of(id)
    }
}

impl IoWaitWake for WorkerPool {
    fn io_wait_resume(&mut self, id: ContextId) -> Result<(), SchedulerError> {
        self.resume(id)
    }
}

/// Opaque handle for a pending schedulable I/O operation.
///
/// Post-MVP this will correspond to a kernel file descriptor, socket, timer id, or async
/// submission token. PHX-sched-2 uses a harness-only numeric id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IoHandle(u64);

impl IoHandle {
    /// Creates a handle from a caller-assigned numeric id (harness and stub I/O only).
    #[must_use]
    pub const fn from_index(index: u64) -> Self {
        Self(index)
    }

    /// Returns the raw index assigned at construction.
    #[must_use]
    pub const fn index(self) -> u64 {
        self.0
    }
}

/// Failure registering or signaling an I/O wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoWaitError {
    /// `handle` is already registered to a parked context.
    HandleAlreadyRegistered(IoHandle),
    /// No parked context is registered for `handle`.
    HandleNotFound(IoHandle),
    /// `context` is not parked with [`ParkReason::AwaitIo`].
    ContextNotAwaitingIo {
        /// Context that rejected registration.
        context: ContextId,
        /// Observed lifecycle state.
        state: ContextState,
    },
    /// The scheduler rejected resume for the registered context.
    Scheduler(SchedulerError),
}

/// Maps pending I/O operations to contexts parked with [`ParkReason::AwaitIo`].
///
/// Invariant: every registered handle points at a context in
/// [`ContextState::Parked`] with reason [`ParkReason::AwaitIo`]. Successful
/// [`Self::signal_ready`] removes the entry and enqueues the context via
/// [`IoWaitWake::io_wait_resume`] on the scheduler ([`SingleThreadScheduler`] or [`WorkerPool`]).
#[derive(Debug, Default)]
pub struct IoWaitRegistry {
    waits: HashMap<u64, ContextId>,
}

impl IoWaitRegistry {
    /// Returns an empty registry with no pending I/O waits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `context` is parked awaiting I/O identified by `handle`.
    ///
    /// Call after [`super::RunningGuard::park`] with [`ParkReason::AwaitIo`], or after a
    /// worker pool parks a context via [`WorkerPool::park_on_next_run`].
    ///
    /// # Errors
    ///
    /// Returns [`IoWaitError::HandleAlreadyRegistered`] when `handle` is already registered,
    /// [`IoWaitError::ContextNotAwaitingIo`] when `context` is missing or not parked for I/O,
    /// or [`IoWaitError::Scheduler`] when the context id is unknown to `scheduler`.
    pub fn register(
        &mut self,
        handle: IoHandle,
        context: ContextId,
        scheduler: &impl IoWaitState,
    ) -> Result<(), IoWaitError> {
        if self.waits.contains_key(&handle.index()) {
            return Err(IoWaitError::HandleAlreadyRegistered(handle));
        }
        let state = scheduler
            .io_wait_state_of(context)
            .ok_or(IoWaitError::Scheduler(SchedulerError::ContextNotFound(
                context,
            )))?;
        if state != ContextState::Parked(ParkReason::AwaitIo) {
            return Err(IoWaitError::ContextNotAwaitingIo { context, state });
        }
        self.waits.insert(handle.index(), context);
        Ok(())
    }

    /// Signals readiness for `handle` and resumes the registered context on `scheduler`.
    ///
    /// On success the context transitions to [`ContextState::Runnable`] and is enqueued on
    /// the scheduler run queue (see [`IoWaitWake::io_wait_resume`]).
    ///
    /// # Errors
    ///
    /// Returns [`IoWaitError::HandleNotFound`] when no context is registered for `handle`, or
    /// [`IoWaitError::Scheduler`] when resume fails.
    pub fn signal_ready(
        &mut self,
        handle: IoHandle,
        scheduler: &mut impl IoWaitWake,
    ) -> Result<ContextId, IoWaitError> {
        let context = self
            .waits
            .remove(&handle.index())
            .ok_or(IoWaitError::HandleNotFound(handle))?;
        scheduler
            .io_wait_resume(context)
            .map_err(IoWaitError::Scheduler)?;
        Ok(context)
    }

    /// Returns whether `handle` has a registered parked context.
    #[must_use]
    pub fn is_registered(&self, handle: IoHandle) -> bool {
        self.waits.contains_key(&handle.index())
    }

    /// Returns the context registered for `handle`, if any.
    #[must_use]
    pub fn context_for(&self, handle: IoHandle) -> Option<ContextId> {
        self.waits.get(&handle.index()).copied()
    }

    /// Returns how many I/O waits are registered and not yet signaled.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.waits.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::harness::RunningGuard;
    use crate::scheduler::{ParkReason, StepOutcome, WorkerPool};

    fn run_next_guard<'a>(sched: &'a mut SingleThreadScheduler, label: &str) -> RunningGuard<'a> {
        match sched.run_next() {
            Ok(Some(guard)) => guard,
            Ok(None) => panic!("{label}: expected runnable context"),
            Err(err) => panic!("{label}: {err:?}"),
        }
    }

    fn step_ok(guard: RunningGuard<'_>, label: &str) -> StepOutcome {
        match guard.step() {
            Ok(outcome) => outcome,
            Err(err) => panic!("{label}: {err:?}"),
        }
    }

    fn park_ok(guard: RunningGuard<'_>, reason: ParkReason, label: &str) {
        if let Err(err) = guard.park(reason) {
            panic!("{label}: {err:?}");
        }
    }

    fn register_ok(
        registry: &mut IoWaitRegistry,
        handle: IoHandle,
        context: ContextId,
        sched: &impl IoWaitState,
        label: &str,
    ) {
        if let Err(err) = registry.register(handle, context, sched) {
            panic!("{label}: {err:?}");
        }
    }

    fn register_err(
        registry: &mut IoWaitRegistry,
        handle: IoHandle,
        context: ContextId,
        sched: &impl IoWaitState,
        label: &str,
    ) -> IoWaitError {
        match registry.register(handle, context, sched) {
            Err(err) => err,
            Ok(()) => panic!("{label}: expected register error"),
        }
    }

    fn signal_ready_ok(
        registry: &mut IoWaitRegistry,
        handle: IoHandle,
        sched: &mut impl IoWaitWake,
        label: &str,
    ) -> ContextId {
        match registry.signal_ready(handle, sched) {
            Ok(context) => context,
            Err(err) => panic!("{label}: {err:?}"),
        }
    }

    fn signal_ready_err(
        registry: &mut IoWaitRegistry,
        handle: IoHandle,
        sched: &mut impl IoWaitWake,
        label: &str,
    ) -> IoWaitError {
        match registry.signal_ready(handle, sched) {
            Err(err) => err,
            Ok(_) => panic!("{label}: expected signal_ready error"),
        }
    }

    #[test]
    fn park_await_io_signal_ready_context_becomes_runnable() {
        let mut sched = SingleThreadScheduler::new();
        let mut registry = IoWaitRegistry::new();
        let handle = IoHandle::from_index(1);

        let ctx = sched.spawn(2);
        let guard = run_next_guard(&mut sched, "dequeue spawned context");
        assert_eq!(guard.id(), ctx);
        park_ok(guard, ParkReason::AwaitIo, "park for I/O");

        assert_eq!(
            sched.state_of(ctx),
            Some(ContextState::Parked(ParkReason::AwaitIo))
        );
        assert_eq!(sched.runnable_count(), 0);
        assert_eq!(sched.parked_count(), 1);

        register_ok(
            &mut registry,
            handle,
            ctx,
            &sched,
            "register parked context",
        );
        assert_eq!(registry.pending_count(), 1);
        assert_eq!(registry.context_for(handle), Some(ctx));

        let resumed = signal_ready_ok(&mut registry, handle, &mut sched, "I/O readiness wakeup");
        assert_eq!(resumed, ctx);
        assert_eq!(registry.pending_count(), 0);
        assert!(!registry.is_registered(handle));

        assert_eq!(sched.state_of(ctx), Some(ContextState::Runnable));
        assert_eq!(sched.runnable_count(), 1);
        assert_eq!(sched.parked_count(), 0);
        assert!(sched.run_queue().contains(ctx));

        let guard = run_next_guard(&mut sched, "run after I/O wakeup");
        assert_eq!(guard.id(), ctx);
        assert!(matches!(
            step_ok(guard, "step after wakeup"),
            StepOutcome::Stepped { id, .. } if id == ctx
        ));
    }

    #[test]
    fn register_rejects_non_await_io_context() {
        let mut sched = SingleThreadScheduler::new();
        let mut registry = IoWaitRegistry::new();
        let ctx = sched.spawn(1);
        let handle = IoHandle::from_index(7);

        let err = register_err(
            &mut registry,
            handle,
            ctx,
            &sched,
            "runnable context cannot register",
        );
        assert!(matches!(
            err,
            IoWaitError::ContextNotAwaitingIo {
                context,
                state: ContextState::Runnable,
            } if context == ctx
        ));
    }

    #[test]
    fn register_rejects_duplicate_handle() {
        let mut sched = SingleThreadScheduler::new();
        let mut registry = IoWaitRegistry::new();
        let a = sched.spawn(1);
        let b = sched.spawn(1);
        let handle = IoHandle::from_index(3);

        let guard = run_next_guard(&mut sched, "run a");
        park_ok(guard, ParkReason::AwaitIo, "park a");
        register_ok(&mut registry, handle, a, &sched, "first register");

        let guard = run_next_guard(&mut sched, "run b");
        park_ok(guard, ParkReason::AwaitIo, "park b");

        let err = register_err(&mut registry, handle, b, &sched, "duplicate handle");
        assert_eq!(err, IoWaitError::HandleAlreadyRegistered(handle));
    }

    #[test]
    fn signal_ready_unknown_handle_returns_not_found() {
        let mut sched = SingleThreadScheduler::new();
        let mut registry = IoWaitRegistry::new();
        let handle = IoHandle::from_index(99);

        let err = signal_ready_err(&mut registry, handle, &mut sched, "unknown handle");
        assert_eq!(err, IoWaitError::HandleNotFound(handle));
    }

    #[test]
    fn worker_pool_park_await_io_signal_ready_completes_all_contexts() {
        let mut pool = WorkerPool::new(3);
        let mut registry = IoWaitRegistry::new();
        let handle = IoHandle::from_index(42);

        let io_blocked = pool.spawn_parking_on_first_run(3, ParkReason::AwaitIo);
        let fast_a = pool.spawn(1);
        let fast_b = pool.spawn(2);

        pool.wait_until(|status| status.done_count >= 2 && status.parked_count >= 1);

        assert_eq!(pool.state_of(fast_a), Some(ContextState::Done));
        assert_eq!(pool.state_of(fast_b), Some(ContextState::Done));
        assert_eq!(
            pool.state_of(io_blocked),
            Some(ContextState::Parked(ParkReason::AwaitIo))
        );
        assert_eq!(pool.done_count(), 2);
        assert_eq!(pool.parked_count(), 1);
        assert_eq!(pool.runnable_count(), 0);

        register_ok(
            &mut registry,
            handle,
            io_blocked,
            &pool,
            "register worker-pool I/O wait",
        );
        assert_eq!(registry.pending_count(), 1);

        let resumed = signal_ready_ok(&mut registry, handle, &mut pool, "worker pool I/O wakeup");
        assert_eq!(resumed, io_blocked);
        assert_eq!(registry.pending_count(), 0);
        // Workers may dequeue and finish the context before this thread observes Runnable.
        assert_ne!(
            pool.state_of(io_blocked),
            Some(ContextState::Parked(ParkReason::AwaitIo))
        );

        pool.wait_all_done();

        assert_eq!(pool.done_count(), 3);
        assert_eq!(pool.parked_count(), 0);
        assert_eq!(pool.runnable_count(), 0);
        assert_eq!(pool.state_of(io_blocked), Some(ContextState::Done));
        pool.shutdown();
    }

    #[test]
    fn worker_pool_io_wait_register_rejects_runnable_context() {
        let pool = WorkerPool::new(2);
        let mut registry = IoWaitRegistry::new();
        let ctx = pool.spawn(2);
        let handle = IoHandle::from_index(5);

        let err = register_err(
            &mut registry,
            handle,
            ctx,
            &pool,
            "runnable worker-pool context",
        );
        assert!(matches!(
            err,
            IoWaitError::ContextNotAwaitingIo {
                context,
                state: ContextState::Runnable,
            } if context == ctx
        ));
        pool.shutdown();
    }
}
