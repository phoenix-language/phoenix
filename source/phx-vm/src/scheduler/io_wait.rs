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
/// [`SingleThreadScheduler::resume`].
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
    /// Call after [`super::RunningGuard::park`] with [`ParkReason::AwaitIo`].
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
        scheduler: &SingleThreadScheduler,
    ) -> Result<(), IoWaitError> {
        if self.waits.contains_key(&handle.index()) {
            return Err(IoWaitError::HandleAlreadyRegistered(handle));
        }
        let state = scheduler.state_of(context).ok_or(IoWaitError::Scheduler(
            SchedulerError::ContextNotFound(context),
        ))?;
        if state != ContextState::Parked(ParkReason::AwaitIo) {
            return Err(IoWaitError::ContextNotAwaitingIo { context, state });
        }
        self.waits.insert(handle.index(), context);
        Ok(())
    }

    /// Signals readiness for `handle` and resumes the registered context on `scheduler`.
    ///
    /// On success the context transitions to [`ContextState::Runnable`] and is enqueued on
    /// the scheduler run queue (see [`SingleThreadScheduler::resume`]).
    ///
    /// # Errors
    ///
    /// Returns [`IoWaitError::HandleNotFound`] when no context is registered for `handle`, or
    /// [`IoWaitError::Scheduler`] when resume fails.
    pub fn signal_ready(
        &mut self,
        handle: IoHandle,
        scheduler: &mut SingleThreadScheduler,
    ) -> Result<ContextId, IoWaitError> {
        let context = self
            .waits
            .remove(&handle.index())
            .ok_or(IoWaitError::HandleNotFound(handle))?;
        scheduler.resume(context).map_err(IoWaitError::Scheduler)?;
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::scheduler::{ParkReason, StepOutcome};

    #[test]
    fn park_await_io_signal_ready_context_becomes_runnable() {
        let mut sched = SingleThreadScheduler::new();
        let mut registry = IoWaitRegistry::new();
        let handle = IoHandle::from_index(1);

        let ctx = sched.spawn(2);
        let guard = sched.run_next().unwrap().expect("dequeue spawned context");
        assert_eq!(guard.id(), ctx);
        guard.park(ParkReason::AwaitIo).expect("park for I/O");

        assert_eq!(
            sched.state_of(ctx),
            Some(ContextState::Parked(ParkReason::AwaitIo))
        );
        assert_eq!(sched.runnable_count(), 0);
        assert_eq!(sched.parked_count(), 1);

        registry
            .register(handle, ctx, &sched)
            .expect("register parked context");
        assert_eq!(registry.pending_count(), 1);
        assert_eq!(registry.context_for(handle), Some(ctx));

        let resumed = registry
            .signal_ready(handle, &mut sched)
            .expect("I/O readiness wakeup");
        assert_eq!(resumed, ctx);
        assert_eq!(registry.pending_count(), 0);
        assert!(!registry.is_registered(handle));

        assert_eq!(sched.state_of(ctx), Some(ContextState::Runnable));
        assert_eq!(sched.runnable_count(), 1);
        assert_eq!(sched.parked_count(), 0);
        assert!(sched.run_queue().contains(ctx));

        let guard = sched.run_next().unwrap().expect("run after I/O wakeup");
        assert_eq!(guard.id(), ctx);
        assert!(matches!(
            guard.step().expect("step after wakeup"),
            StepOutcome::Stepped { id, .. } if id == ctx
        ));
    }

    #[test]
    fn register_rejects_non_await_io_context() {
        let mut sched = SingleThreadScheduler::new();
        let mut registry = IoWaitRegistry::new();
        let ctx = sched.spawn(1);
        let handle = IoHandle::from_index(7);

        let err = registry
            .register(handle, ctx, &sched)
            .expect_err("runnable context cannot register");
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

        let guard = sched.run_next().unwrap().expect("run a");
        guard.park(ParkReason::AwaitIo).expect("park a");
        registry
            .register(handle, a, &sched)
            .expect("first register");

        let guard = sched.run_next().unwrap().expect("run b");
        guard.park(ParkReason::AwaitIo).expect("park b");

        let err = registry
            .register(handle, b, &sched)
            .expect_err("duplicate handle");
        assert_eq!(err, IoWaitError::HandleAlreadyRegistered(handle));
    }

    #[test]
    fn signal_ready_unknown_handle_returns_not_found() {
        let mut sched = SingleThreadScheduler::new();
        let mut registry = IoWaitRegistry::new();
        let handle = IoHandle::from_index(99);

        let err = registry
            .signal_ready(handle, &mut sched)
            .expect_err("unknown handle");
        assert_eq!(err, IoWaitError::HandleNotFound(handle));
    }
}
