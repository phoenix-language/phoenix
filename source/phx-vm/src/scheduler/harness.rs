//! Single-threaded cooperative scheduler harness for unit tests.

use super::context::{ContextId, ContextState, RunnableContext, StepOutcome};
use super::park::ParkReason;
use super::queue::RunQueue;

/// Invalid park, resume, or lookup on a scheduler-managed context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerError {
    /// No context exists for the given id.
    ContextNotFound(ContextId),
    /// The requested transition is illegal for the context's current state.
    InvalidTransition {
        /// Context that rejected the operation.
        id: ContextId,
        /// State observed at the time of the call.
        state: ContextState,
        /// Short label for the attempted operation (for diagnostics).
        operation: &'static str,
    },
}

/// Cooperative scheduler harness executing on one OS thread.
///
/// Spawns [`RunnableContext`] values, dequeues them through [`Self::run_next`], and
/// supports park/resume without worker pools or I/O. Each spawned context runs a fixed
/// number of harness "steps" before reaching [`ContextState::Done`].
#[derive(Debug, Default)]
pub struct SingleThreadScheduler {
    contexts: Vec<RunnableContext>,
    run_queue: RunQueue,
    next_id: u64,
    running: Option<ContextId>,
}

impl SingleThreadScheduler {
    /// Returns an empty scheduler with no contexts.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns a new context with `steps` harness work units and enqueues it.
    ///
    /// # Panics
    ///
    /// Never panics; `steps` may be zero (the context completes on the first step).
    pub fn spawn(&mut self, steps: u32) -> ContextId {
        let id = ContextId::from_index(self.next_id);
        self.next_id += 1;
        self.contexts.push(RunnableContext::new(id, steps));
        self.run_queue.push(id);
        id
    }

    /// Dequeues the next runnable context and marks it [`ContextState::Running`].
    ///
    /// Returns `Ok(None)` when the run queue is empty or a context is already running.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::ContextNotFound`] when the run queue references a
    /// context id that is not present in the scheduler table.
    pub fn run_next(&mut self) -> Result<Option<RunningGuard<'_>>, SchedulerError> {
        if self.running.is_some() {
            return Ok(None);
        }
        let Some(id) = self.run_queue.pop() else {
            return Ok(None);
        };
        let ctx = self
            .context_mut(id)
            .ok_or(SchedulerError::ContextNotFound(id))?;
        ctx.set_state(ContextState::Running);
        self.running = Some(id);
        Ok(Some(RunningGuard {
            scheduler: self,
            id,
        }))
    }

    /// Moves a parked context back to the runnable queue.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::ContextNotFound`] when `id` is unknown, or
    /// [`SchedulerError::InvalidTransition`] when the context is not [`ContextState::Parked`].
    pub fn resume(&mut self, id: ContextId) -> Result<(), SchedulerError> {
        let state = self
            .context(id)
            .ok_or(SchedulerError::ContextNotFound(id))?
            .state();
        if !matches!(state, ContextState::Parked(_)) {
            return Err(SchedulerError::InvalidTransition {
                id,
                state,
                operation: "resume",
            });
        }
        let ctx = self
            .context_mut(id)
            .ok_or(SchedulerError::ContextNotFound(id))?;
        ctx.set_state(ContextState::Runnable);
        self.run_queue.push(id);
        Ok(())
    }

    /// Returns the lifecycle state of `id`, if it exists.
    #[must_use]
    pub fn state_of(&self, id: ContextId) -> Option<ContextState> {
        self.context(id).map(RunnableContext::state)
    }

    /// Returns how many contexts are waiting in the run queue.
    #[must_use]
    pub fn runnable_count(&self) -> usize {
        self.run_queue.len()
    }

    /// Returns how many contexts are parked (any [`ParkReason`]).
    #[must_use]
    pub fn parked_count(&self) -> usize {
        self.contexts
            .iter()
            .filter(|ctx| matches!(ctx.state(), ContextState::Parked(_)))
            .count()
    }

    /// Returns how many contexts have reached [`ContextState::Done`].
    #[must_use]
    pub fn done_count(&self) -> usize {
        self.contexts
            .iter()
            .filter(|ctx| ctx.state() == ContextState::Done)
            .count()
    }

    /// Returns the total number of spawned contexts (all lifecycle states).
    #[must_use]
    pub fn context_count(&self) -> usize {
        self.contexts.len()
    }

    /// Returns a shared view of the runnable queue (for assertions in tests).
    #[must_use]
    pub fn run_queue(&self) -> &RunQueue {
        &self.run_queue
    }

    fn context(&self, id: ContextId) -> Option<&RunnableContext> {
        self.contexts.iter().find(|ctx| ctx.id() == id)
    }

    fn context_mut(&mut self, id: ContextId) -> Option<&mut RunnableContext> {
        self.contexts.iter_mut().find(|ctx| ctx.id() == id)
    }

    fn clear_running(&mut self, id: ContextId) {
        if self.running == Some(id) {
            self.running = None;
        }
    }
}

/// Proof that `id` is the currently running context on this scheduler.
///
/// Obtain via [`SingleThreadScheduler::run_next`]; consume with [`Self::step`] or
/// [`Self::park`].
#[derive(Debug)]
#[must_use = "running context must be stepped or parked"]
pub struct RunningGuard<'a> {
    scheduler: &'a mut SingleThreadScheduler,
    id: ContextId,
}

impl RunningGuard<'_> {
    /// Returns the running context id.
    #[must_use]
    pub fn id(&self) -> ContextId {
        self.id
    }

    /// Executes one harness step on the running context.
    ///
    /// Re-enqueues the context when steps remain; otherwise marks it [`ContextState::Done`].
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::ContextNotFound`] if the running context was removed
    /// from the scheduler (should not occur while the guard is live).
    pub fn step(self) -> Result<StepOutcome, SchedulerError> {
        let id = self.id;
        let outcome = {
            let ctx = self
                .scheduler
                .context_mut(id)
                .ok_or(SchedulerError::ContextNotFound(id))?;
            let remaining = ctx.dec_step();
            if remaining == 0 {
                ctx.set_state(ContextState::Done);
                StepOutcome::Completed(id)
            } else {
                ctx.set_state(ContextState::Runnable);
                self.scheduler.run_queue.push(id);
                StepOutcome::Stepped {
                    id,
                    steps_remaining: remaining,
                }
            }
        };
        self.scheduler.clear_running(id);
        Ok(outcome)
    }

    /// Parks the running context with `reason` without re-enqueueing it.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::ContextNotFound`] when the context id is unknown, or
    /// [`SchedulerError::InvalidTransition`] if the context is no longer running
    /// (should not occur while the guard is live).
    pub fn park(self, reason: ParkReason) -> Result<(), SchedulerError> {
        let id = self.id;
        let state = self
            .scheduler
            .context(id)
            .ok_or(SchedulerError::ContextNotFound(id))?
            .state();
        if state != ContextState::Running {
            return Err(SchedulerError::InvalidTransition {
                id,
                state,
                operation: "park",
            });
        }
        let ctx = self
            .scheduler
            .context_mut(id)
            .ok_or(SchedulerError::ContextNotFound(id))?;
        ctx.set_state(ContextState::Parked(reason));
        self.scheduler.clear_running(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::{ParkReason, StepOutcome};

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

    fn resume_ok(sched: &mut SingleThreadScheduler, id: ContextId, label: &str) {
        if let Err(err) = sched.resume(id) {
            panic!("{label}: {err:?}");
        }
    }

    fn resume_err(sched: &mut SingleThreadScheduler, id: ContextId, label: &str) -> SchedulerError {
        match sched.resume(id) {
            Err(err) => err,
            Ok(()) => panic!("{label}: expected resume error"),
        }
    }

    fn park_err(guard: RunningGuard<'_>, reason: ParkReason, label: &str) -> SchedulerError {
        match guard.park(reason) {
            Err(err) => err,
            Ok(()) => panic!("{label}: expected park error"),
        }
    }

    fn run_next_none(sched: &mut SingleThreadScheduler, label: &str) {
        match sched.run_next() {
            Ok(None) => {}
            Ok(Some(_)) => panic!("{label}: expected empty run queue"),
            Err(err) => panic!("{label}: {err:?}"),
        }
    }

    #[test]
    fn spawn_n_contexts_park_one_resume_complete_on_one_thread() {
        let mut sched = SingleThreadScheduler::new();
        let a = sched.spawn(3);
        let b = sched.spawn(2);
        let c = sched.spawn(1);

        assert_eq!(sched.context_count(), 3);
        assert_eq!(sched.runnable_count(), 3);

        let guard = run_next_guard(&mut sched, "dequeue first context");
        assert_eq!(guard.id(), a);
        park_ok(guard, ParkReason::AwaitIo, "park running a");

        assert_eq!(
            sched.state_of(a),
            Some(ContextState::Parked(ParkReason::AwaitIo))
        );
        assert_eq!(sched.runnable_count(), 2);
        assert_eq!(sched.parked_count(), 1);
        assert!(sched.run_queue().contains(b));
        assert!(sched.run_queue().contains(c));

        let guard = run_next_guard(&mut sched, "dequeue b");
        assert_eq!(guard.id(), b);
        assert!(matches!(
            step_ok(guard, "step b"),
            StepOutcome::Stepped {
                id,
                steps_remaining: 1
            } if id == b
        ));

        let guard = run_next_guard(&mut sched, "dequeue c while b waits in queue tail");
        assert_eq!(guard.id(), c);
        assert!(matches!(
            step_ok(guard, "step c"),
            StepOutcome::Completed(id) if id == c
        ));

        let guard = run_next_guard(&mut sched, "dequeue b again");
        assert_eq!(guard.id(), b);
        assert!(matches!(
            step_ok(guard, "finish b"),
            StepOutcome::Completed(id) if id == b
        ));

        assert_eq!(sched.done_count(), 2);
        assert_eq!(sched.runnable_count(), 0);
        assert_eq!(sched.parked_count(), 1);

        resume_ok(&mut sched, a, "resume parked a");
        assert_eq!(sched.state_of(a), Some(ContextState::Runnable));
        assert_eq!(sched.runnable_count(), 1);

        while sched.runnable_count() > 0 {
            let guard = run_next_guard(&mut sched, "drain runnable queue");
            match step_ok(guard, "drain step") {
                StepOutcome::Stepped { .. } => {}
                StepOutcome::Completed(id) => assert_eq!(id, a),
            }
        }

        assert_eq!(sched.done_count(), 3);
        assert_eq!(sched.parked_count(), 0);
        assert_eq!(sched.runnable_count(), 0);
        run_next_none(&mut sched, "run queue drained");
    }

    #[test]
    fn park_already_parked_context_returns_invalid_transition() {
        let mut sched = SingleThreadScheduler::new();
        let id = sched.spawn(2);

        let guard = run_next_guard(&mut sched, "dequeue for double-park test");
        park_ok(guard, ParkReason::AwaitIo, "first park");
        assert_eq!(
            sched.state_of(id),
            Some(ContextState::Parked(ParkReason::AwaitIo))
        );
        assert_eq!(sched.parked_count(), 1);

        // Simulate a stale running guard: context stayed Parked while the worker
        // slot still names it as running.
        {
            let ctx = sched
                .context_mut(id)
                .expect("context must exist for double-park setup");
            ctx.set_state(ContextState::Parked(ParkReason::AwaitMessage));
            sched.running = Some(id);
        }
        let guard = RunningGuard {
            scheduler: &mut sched,
            id,
        };
        let err = park_err(
            guard,
            ParkReason::AwaitIo,
            "second park on already-parked context",
        );
        assert!(matches!(
            err,
            SchedulerError::InvalidTransition {
                id: cid,
                state: ContextState::Parked(ParkReason::AwaitMessage),
                operation: "park",
            } if cid == id
        ));
        assert_eq!(
            sched.state_of(id),
            Some(ContextState::Parked(ParkReason::AwaitMessage))
        );
        assert_eq!(sched.parked_count(), 1);
        assert_eq!(sched.runnable_count(), 0);
    }

    #[test]
    fn resume_non_parked_context_returns_invalid_transition() {
        let mut sched = SingleThreadScheduler::new();
        let id = sched.spawn(1);
        let err = resume_err(&mut sched, id, "runnable cannot resume");
        assert!(matches!(
            err,
            SchedulerError::InvalidTransition {
                state: ContextState::Runnable,
                operation: "resume",
                ..
            }
        ));
    }

    #[test]
    fn park_reason_await_message_round_trip() {
        let mut sched = SingleThreadScheduler::new();
        let id = sched.spawn(2);
        let guard = run_next_guard(&mut sched, "run");
        park_ok(guard, ParkReason::AwaitMessage, "park for mailbox");
        assert_eq!(
            sched.state_of(id),
            Some(ContextState::Parked(ParkReason::AwaitMessage))
        );
        resume_ok(&mut sched, id, "wakeup after message");
        let guard = run_next_guard(&mut sched, "run after resume");
        assert!(matches!(
            step_ok(guard, "step after resume"),
            StepOutcome::Stepped { .. }
        ));
        let guard = run_next_guard(&mut sched, "finish");
        assert!(matches!(
            step_ok(guard, "final step"),
            StepOutcome::Completed(cid) if cid == id
        ));
    }
}
