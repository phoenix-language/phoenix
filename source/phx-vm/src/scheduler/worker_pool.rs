//! Fixed-size OS-thread worker pool dequeuing from a shared runnable queue.
//!
//! PHX-sched-4 extends the single-threaded harness to M:N scheduling: several workers
//! compete for [`RunnableContext`] ids on one mutex-protected [`RunQueue`], while
//! [`Self::resume`] and park hooks preserve the park/resume contract from PHX-sched-0.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use super::await_io::AwaitIoOperands;
use super::context::{ContextId, ContextState, RunnableContext, StepOutcome};
use super::harness::SchedulerError;
use super::io_wait::{IoWaitRegistry, IoWaitState};
use super::park::ParkReason;
use super::queue::RunQueue;

/// Point-in-time worker pool metrics for [`WorkerPool::wait_until`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerPoolStatus {
    /// Total spawned contexts.
    pub context_count: usize,
    /// Contexts waiting in the runnable queue.
    pub runnable_count: usize,
    /// Contexts parked for I/O or mailbox waits.
    pub parked_count: usize,
    /// Contexts that finished their harness step budget.
    pub done_count: usize,
    /// Contexts currently assigned to a worker thread.
    pub running_count: usize,
}

/// Shared scheduler state protected by the worker pool mutex.
#[derive(Debug)]
struct PoolInner {
    contexts: Vec<RunnableContext>,
    run_queue: RunQueue,
    next_id: u64,
    running: Vec<ContextId>,
    /// Harness hook: park a context immediately when a worker dequeues it.
    park_on_dequeue: HashMap<ContextId, ParkReason>,
    /// Harness hook: register [`Opcode::AwaitIo`](phx_bytecode::Opcode::AwaitIo) after park on dequeue.
    await_io_on_dequeue: HashMap<ContextId, AwaitIoOperands>,
    /// Shared I/O wait registry for await-I/O harness contexts (PHX-sched-6).
    io_registry: Option<Arc<Mutex<IoWaitRegistry>>>,
}

/// Read-only worker-pool snapshot for [`IoWaitRegistry::register`] from worker threads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PoolInnerSnapshot {
    contexts: *const RunnableContext,
    context_count: usize,
}

impl PoolInnerSnapshot {
    fn state_of(self, id: ContextId) -> Option<ContextState> {
        // SAFETY: `contexts` points at `PoolInner::contexts` while the pool mutex is held.
        let contexts = unsafe { std::slice::from_raw_parts(self.contexts, self.context_count) };
        contexts
            .iter()
            .find(|ctx| ctx.id() == id)
            .map(RunnableContext::state)
    }
}

struct PoolIoWaitState(PoolInnerSnapshot);

impl IoWaitState for PoolIoWaitState {
    fn io_wait_state_of(&self, id: ContextId) -> Option<ContextState> {
        self.0.state_of(id)
    }
}

impl PoolInner {
    fn snapshot(&self) -> PoolInnerSnapshot {
        PoolInnerSnapshot {
            contexts: self.contexts.as_ptr(),
            context_count: self.contexts.len(),
        }
    }
    fn context(&self, id: ContextId) -> Option<&RunnableContext> {
        self.contexts.iter().find(|ctx| ctx.id() == id)
    }

    fn context_mut(&mut self, id: ContextId) -> Option<&mut RunnableContext> {
        self.contexts.iter_mut().find(|ctx| ctx.id() == id)
    }

    fn clear_running(&mut self, id: ContextId) {
        if let Some(pos) = self.running.iter().position(|&running| running == id) {
            self.running.remove(pos);
        }
    }

    fn parked_count(&self) -> usize {
        self.contexts
            .iter()
            .filter(|ctx| matches!(ctx.state(), ContextState::Parked(_)))
            .count()
    }

    fn done_count(&self) -> usize {
        self.contexts
            .iter()
            .filter(|ctx| ctx.state() == ContextState::Done)
            .count()
    }

    fn has_pending_work(&self) -> bool {
        !self.run_queue.is_empty() || !self.running.is_empty()
    }

    fn pop_and_mark_running(&mut self) -> Option<ContextId> {
        let id = self.run_queue.pop()?;
        let ctx = self.context_mut(id)?;
        ctx.set_state(ContextState::Running);
        self.running.push(id);
        Some(id)
    }

    fn park_context(&mut self, id: ContextId, reason: ParkReason) -> Result<(), SchedulerError> {
        let state = self
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
            .context_mut(id)
            .ok_or(SchedulerError::ContextNotFound(id))?;
        ctx.set_state(ContextState::Parked(reason));
        self.clear_running(id);
        Ok(())
    }

    fn step_context(&mut self, id: ContextId) -> Result<StepOutcome, SchedulerError> {
        let outcome = {
            let ctx = self
                .context_mut(id)
                .ok_or(SchedulerError::ContextNotFound(id))?;
            let remaining = ctx.dec_step();
            if remaining == 0 {
                ctx.set_state(ContextState::Done);
                StepOutcome::Completed(id)
            } else {
                ctx.set_state(ContextState::Runnable);
                self.run_queue.push(id);
                StepOutcome::Stepped {
                    id,
                    steps_remaining: remaining,
                }
            }
        };
        self.clear_running(id);
        Ok(outcome)
    }

    fn resume(&mut self, id: ContextId) -> Result<(), SchedulerError> {
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
}

fn lock_inner(inner: &Mutex<PoolInner>) -> MutexGuard<'_, PoolInner> {
    inner.lock().unwrap_or_else(PoisonError::into_inner)
}

/// M:N scheduler harness: fixed worker threads share one runnable queue.
///
/// Workers dequeue [`RunnableContext`] ids, execute one harness step per dequeue, and
/// honor [`Self::park_on_next_run`] / [`Self::resume`] without blocking OS threads on I/O.
pub struct WorkerPool {
    inner: Arc<Mutex<PoolInner>>,
    cvar: Arc<Condvar>,
    shutdown: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}

impl fmt::Debug for WorkerPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkerPool")
            .field("status", &self.status())
            .field("worker_count", &self.worker_count())
            .finish()
    }
}

impl WorkerPool {
    /// Creates a worker pool with `worker_count` OS threads (clamped to 2..=4).
    ///
    /// Workers start immediately and block until contexts are spawned or shutdown.
    ///
    /// # Panics
    ///
    /// Panics if a worker thread fails to spawn (OS resource exhaustion).
    #[must_use]
    pub fn new(worker_count: usize) -> Self {
        let worker_count = worker_count.clamp(2, 4);
        let inner = Arc::new(Mutex::new(PoolInner {
            contexts: Vec::new(),
            run_queue: RunQueue::new(),
            next_id: 0,
            running: Vec::new(),
            park_on_dequeue: HashMap::new(),
            await_io_on_dequeue: HashMap::new(),
            io_registry: None,
        }));
        let cvar = Arc::new(Condvar::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let inner = Arc::clone(&inner);
            let cvar = Arc::clone(&cvar);
            let shutdown = Arc::clone(&shutdown);
            workers.push(
                thread::Builder::new()
                    .name("phx-sched-worker".into())
                    .spawn(move || worker_loop(&inner, &cvar, &shutdown))
                    .unwrap_or_else(|err| panic!("worker thread spawn: {err}")),
            );
        }

        Self {
            inner,
            cvar,
            shutdown,
            workers,
        }
    }

    /// Creates a worker pool with `worker_count` OS threads wired to `io_registry`.
    ///
    /// Worker threads register [`Opcode::AwaitIo`](phx_bytecode::Opcode::AwaitIo) parks through
    /// `io_registry` when contexts are spawned via [`Self::spawn_await_io_on_first_run`].
    #[must_use]
    pub fn with_io_registry(worker_count: usize, io_registry: Arc<Mutex<IoWaitRegistry>>) -> Self {
        let pool = Self::new(worker_count);
        lock_inner(&pool.inner).io_registry = Some(io_registry);
        pool
    }

    /// Spawns a context that parks on [`ParkReason::AwaitIo`] at its first worker dequeue and
    /// registers the pending [`super::IoHandle`] in the pool's I/O registry.
    ///
    /// Simulates a synthetic harness context executing [`Opcode::AwaitIo`] with `operands`.
    #[must_use]
    pub fn spawn_await_io_on_first_run(&self, steps: u32, operands: AwaitIoOperands) -> ContextId {
        let id = {
            let mut inner = lock_inner(&self.inner);
            let id = ContextId::from_index(inner.next_id);
            inner.next_id += 1;
            inner.contexts.push(RunnableContext::new(id, steps));
            inner.park_on_dequeue.insert(id, ParkReason::AwaitIo);
            inner.await_io_on_dequeue.insert(id, operands);
            inner.run_queue.push(id);
            id
        };
        self.cvar.notify_all();
        id
    }

    /// Parks a currently running context without re-enqueueing it.
    ///
    /// Harness-only; production schedulers park from bytecode dispatch on the owning worker.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError`] when `id` is unknown or not [`ContextState::Running`].
    pub fn park_running(&self, id: ContextId, reason: ParkReason) -> Result<(), SchedulerError> {
        {
            let mut inner = lock_inner(&self.inner);
            inner.park_context(id, reason)?;
        }
        self.cvar.notify_all();
        Ok(())
    }

    /// Spawns a context with `steps` harness work units and enqueues it for workers.
    #[must_use]
    pub fn spawn(&self, steps: u32) -> ContextId {
        self.spawn_inner(steps, None)
    }

    /// Spawns a context that parks with `reason` on its first worker dequeue.
    ///
    /// Unlike [`Self::park_on_next_run`] after [`Self::spawn`], the park hook is registered
    /// before the context is enqueued, so workers cannot run it first.
    #[must_use]
    pub fn spawn_parking_on_first_run(&self, steps: u32, reason: ParkReason) -> ContextId {
        self.spawn_inner(steps, Some(reason))
    }

    fn spawn_inner(&self, steps: u32, park_on_first_run: Option<ParkReason>) -> ContextId {
        let id = {
            let mut inner = lock_inner(&self.inner);
            let id = ContextId::from_index(inner.next_id);
            inner.next_id += 1;
            inner.contexts.push(RunnableContext::new(id, steps));
            if let Some(reason) = park_on_first_run {
                inner.park_on_dequeue.insert(id, reason);
            }
            inner.run_queue.push(id);
            id
        };
        self.cvar.notify_all();
        id
    }

    /// Registers `id` to be parked with `reason` when a worker next dequeues it.
    ///
    /// Harness-only hook for unit tests; production schedulers will park from bytecode.
    pub fn park_on_next_run(&self, id: ContextId, reason: ParkReason) {
        let mut inner = lock_inner(&self.inner);
        inner.park_on_dequeue.insert(id, reason);
    }

    /// Moves a parked context back to the shared runnable queue.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError`] when `id` is unknown or not parked.
    pub fn resume(&self, id: ContextId) -> Result<(), SchedulerError> {
        {
            let mut inner = lock_inner(&self.inner);
            inner.resume(id)?;
        }
        self.cvar.notify_all();
        Ok(())
    }

    /// Returns the lifecycle state of `id`, if it exists.
    #[must_use]
    pub fn state_of(&self, id: ContextId) -> Option<ContextState> {
        lock_inner(&self.inner)
            .context(id)
            .map(RunnableContext::state)
    }

    /// Returns how many contexts are waiting in the run queue.
    #[must_use]
    pub fn runnable_count(&self) -> usize {
        lock_inner(&self.inner).run_queue.len()
    }

    /// Returns how many contexts are parked (any [`ParkReason`]).
    #[must_use]
    pub fn parked_count(&self) -> usize {
        lock_inner(&self.inner).parked_count()
    }

    /// Returns how many contexts have reached [`ContextState::Done`].
    #[must_use]
    pub fn done_count(&self) -> usize {
        lock_inner(&self.inner).done_count()
    }

    /// Returns the total number of spawned contexts (all lifecycle states).
    #[must_use]
    pub fn context_count(&self) -> usize {
        lock_inner(&self.inner).contexts.len()
    }

    /// Returns how many OS worker threads this pool runs.
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    fn status_from_inner(inner: &PoolInner) -> WorkerPoolStatus {
        WorkerPoolStatus {
            context_count: inner.contexts.len(),
            runnable_count: inner.run_queue.len(),
            parked_count: inner.parked_count(),
            done_count: inner.done_count(),
            running_count: inner.running.len(),
        }
    }

    /// Returns a snapshot of queue and lifecycle counts.
    #[must_use]
    pub fn status(&self) -> WorkerPoolStatus {
        Self::status_from_inner(&lock_inner(&self.inner))
    }

    /// Blocks until `predicate` returns true on a [`WorkerPoolStatus`] snapshot.
    pub fn wait_until(&self, mut predicate: impl FnMut(WorkerPoolStatus) -> bool) {
        loop {
            let status = self.status();
            if predicate(status) {
                break;
            }
            let inner = lock_inner(&self.inner);
            let status = Self::status_from_inner(&inner);
            if predicate(status) {
                break;
            }
            let _guard = self
                .cvar
                .wait(inner)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Blocks until every spawned context is [`ContextState::Done`].
    pub fn wait_all_done(&self) {
        self.wait_until(|status| {
            status.context_count > 0 && status.done_count == status.context_count
        });
    }

    /// Signals workers to exit once the run queue and running set are drained, then joins threads.
    /// # Panics
    ///
    /// Panics if a worker thread panicked during execution.
    pub fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.cvar.notify_all();
        for handle in self.workers.drain(..) {
            if let Err(err) = handle.join() {
                std::panic::resume_unwind(err);
            }
        }
    }
}

fn worker_loop(inner: &Arc<Mutex<PoolInner>>, cvar: &Arc<Condvar>, shutdown: &Arc<AtomicBool>) {
    loop {
        let id = {
            let mut guard = lock_inner(inner);
            loop {
                if let Some(id) = guard.pop_and_mark_running() {
                    break Some(id);
                }
                if shutdown.load(Ordering::Acquire) && !guard.has_pending_work() {
                    break None;
                }
                guard = cvar.wait(guard).unwrap_or_else(PoisonError::into_inner);
            }
        };

        let Some(id) = id else {
            break;
        };

        let step_result = {
            let mut guard = lock_inner(inner);
            if let Some(reason) = guard.park_on_dequeue.remove(&id) {
                let await_io = guard.await_io_on_dequeue.remove(&id);
                let park_result = guard.park_context(id, reason);
                if park_result.is_ok()
                    && let (Some(registry), Some(operands)) = (&guard.io_registry, await_io)
                {
                    let snapshot = guard.snapshot();
                    let mut registry = registry.lock().unwrap_or_else(PoisonError::into_inner);
                    let register_result =
                        registry.register(operands.io_handle(), id, &PoolIoWaitState(snapshot));
                    if let Err(err) = register_result {
                        panic!("await_io register failed: {err:?}");
                    }
                }
                cvar.notify_all();
                if let Err(err) = park_result {
                    panic!("worker park failed: {err:?}");
                }
                continue;
            }
            guard.step_context(id)
        };

        if let Err(err) = step_result {
            panic!("worker step failed: {err:?}");
        }

        cvar.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resume_ok(pool: &WorkerPool, id: ContextId, label: &str) {
        if let Err(err) = pool.resume(id) {
            panic!("{label}: {err:?}");
        }
    }

    fn resume_err(pool: &WorkerPool, id: ContextId, label: &str) -> SchedulerError {
        match pool.resume(id) {
            Err(err) => err,
            Ok(()) => panic!("{label}: expected resume error"),
        }
    }

    #[test]
    fn worker_pool_drains_spawned_contexts_across_threads() {
        let pool = WorkerPool::new(3);
        let _ = pool.spawn(2);
        let _ = pool.spawn(1);
        let _ = pool.spawn(3);

        assert_eq!(pool.context_count(), 3);
        pool.wait_all_done();
        assert_eq!(pool.done_count(), 3);
        assert_eq!(pool.runnable_count(), 0);
        assert_eq!(pool.parked_count(), 0);
        pool.shutdown();
    }

    #[test]
    fn worker_pool_park_one_context_resume_then_shutdown() {
        let pool = WorkerPool::new(2);
        let parked = pool.spawn(4);
        let fast_a = pool.spawn(1);
        let fast_b = pool.spawn(1);
        pool.park_on_next_run(parked, ParkReason::AwaitIo);

        pool.wait_until(|status| status.done_count >= 2 && status.parked_count >= 1);

        assert_eq!(pool.state_of(fast_a), Some(ContextState::Done));
        assert_eq!(pool.state_of(fast_b), Some(ContextState::Done));
        assert_eq!(
            pool.state_of(parked),
            Some(ContextState::Parked(ParkReason::AwaitIo))
        );
        assert_eq!(pool.done_count(), 2);
        assert_eq!(pool.parked_count(), 1);

        resume_ok(&pool, parked, "resume parked context");
        pool.wait_all_done();

        assert_eq!(pool.done_count(), 3);
        assert_eq!(pool.parked_count(), 0);
        assert_eq!(pool.state_of(parked), Some(ContextState::Done));
        pool.shutdown();
    }

    #[test]
    fn worker_pool_resume_non_parked_returns_invalid_transition() {
        let pool = WorkerPool::new(2);
        let id = pool.spawn(1);
        pool.wait_all_done();
        let err = resume_err(&pool, id, "done context cannot resume");
        assert!(matches!(
            err,
            SchedulerError::InvalidTransition {
                state: ContextState::Done,
                operation: "resume",
                ..
            }
        ));
        pool.shutdown();
    }

    #[test]
    fn worker_pool_clamps_thread_count_to_valid_range() {
        let low = WorkerPool::new(1);
        assert_eq!(low.worker_count(), 2);
        low.shutdown();

        let high = WorkerPool::new(99);
        assert_eq!(high.worker_count(), 4);
        high.shutdown();
    }
}
