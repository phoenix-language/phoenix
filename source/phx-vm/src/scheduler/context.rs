//! Schedulable context identity, lifecycle state, and harness metadata.

use super::park::ParkReason;

/// Opaque handle for a scheduler-managed execution context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContextId(u64);

impl ContextId {
    /// Creates a handle from a spawn-time index (harness-internal).
    pub(crate) const fn from_index(index: u64) -> Self {
        Self(index)
    }

    /// Returns the raw index assigned at spawn time.
    #[must_use]
    pub const fn index(self) -> u64 {
        self.0
    }
}

/// Lifecycle state of a scheduler-managed context.
///
/// Invariant: at most one context is [`ContextState::Running`] on a given worker thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContextState {
    /// Enqueued or eligible for the runnable queue.
    Runnable,
    /// Currently executing on a worker (not re-queued until step or park completes).
    Running,
    /// Removed from the runnable queue until an external wakeup resumes it.
    Parked(ParkReason),
    /// Finished; resources may be reclaimed.
    Done,
}

/// Metadata for one schedulable unit under scheduler control.
///
/// PHX-sched-0 uses [`Self::steps_remaining`] as a harness-only work budget (each
/// [`super::RunningGuard::step`] decrements it). Post-MVP this struct will hold or
/// reference a live [`crate::ExecutionContext`] plus bytecode continuation state.
#[derive(Debug, Clone)]
pub struct RunnableContext {
    id: ContextId,
    state: ContextState,
    steps_remaining: u32,
}

impl RunnableContext {
    /// Creates a new runnable context with `steps_remaining` harness work units.
    #[must_use]
    pub fn new(id: ContextId, steps_remaining: u32) -> Self {
        Self {
            id,
            state: ContextState::Runnable,
            steps_remaining,
        }
    }

    /// Returns this context's stable id.
    #[must_use]
    pub fn id(&self) -> ContextId {
        self.id
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub fn state(&self) -> ContextState {
        self.state
    }

    /// Returns remaining harness steps before [`ContextState::Done`].
    #[must_use]
    pub fn steps_remaining(&self) -> u32 {
        self.steps_remaining
    }

    pub(crate) fn set_state(&mut self, state: ContextState) {
        self.state = state;
    }

    pub(crate) fn dec_step(&mut self) -> u32 {
        self.steps_remaining = self.steps_remaining.saturating_sub(1);
        self.steps_remaining
    }
}

/// Result of executing one harness step on the currently running context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// Context still has work; re-enqueued as [`ContextState::Runnable`].
    Stepped {
        /// Context that ran.
        id: ContextId,
        /// Remaining harness steps.
        steps_remaining: u32,
    },
    /// Context exhausted its step budget and is [`ContextState::Done`].
    Completed(ContextId),
}
