//! [`Opcode::AwaitIo`](phx_bytecode::Opcode::AwaitIo) harness dispatch for the M:N worker pool.
//!
//! PHX-sched-6 wires the reserved post-MVP opcode to scheduler park/resume: a synthetic context
//! parks with [`ParkReason::AwaitIo`], registers an [`IoHandle`] in [`IoWaitRegistry`], and
//! completes after an external [`IoWaitRegistry::signal_ready`] wakeup.
//!
//! Design reference: `docs/design/features/vm-linear.md` § `AWAIT_IO` opcode contract.

use phx_bytecode::{Instruction, Opcode};

use super::context::ContextId;
use super::harness::SchedulerError;
use super::io_wait::{IoHandle, IoWaitError, IoWaitRegistry};
use super::park::ParkReason;
use super::worker_pool::WorkerPool;

/// Operand bundle for [`Opcode::AwaitIo`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AwaitIoOperands {
    /// Schedulable I/O discriminant (file / network / timer — see vm-linear contract).
    pub io_kind: u32,
    /// Index into the VM/host I/O table for this suspend site.
    pub request_id: u32,
}

impl AwaitIoOperands {
    /// Parses operands from a decoded [`Opcode::AwaitIo`] instruction.
    ///
    /// # Errors
    ///
    /// Returns [`AwaitIoHarnessError::MalformedOperands`] when operand count is not two.
    pub fn from_instruction(inst: &Instruction) -> Result<Self, AwaitIoHarnessError> {
        if inst.opcode != Opcode::AwaitIo {
            return Err(AwaitIoHarnessError::WrongOpcode(inst.opcode));
        }
        if inst.operands.len() != 2 {
            return Err(AwaitIoHarnessError::MalformedOperands {
                expected: 2,
                actual: inst.operands.len(),
            });
        }
        Ok(Self {
            io_kind: inst.operands[0],
            request_id: inst.operands[1],
        })
    }

    /// Returns the [`IoHandle`] derived from [`Self::request_id`].
    #[must_use]
    pub fn io_handle(self) -> IoHandle {
        IoHandle::from_index(u64::from(self.request_id))
    }
}

/// Harness failure executing [`Opcode::AwaitIo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AwaitIoHarnessError {
    /// Instruction opcode is not [`Opcode::AwaitIo`].
    WrongOpcode(Opcode),
    /// Operand count does not match the vm-linear contract.
    MalformedOperands {
        /// Expected operand count.
        expected: usize,
        /// Observed operand count.
        actual: usize,
    },
    /// Scheduler rejected park/resume.
    Scheduler(SchedulerError),
    /// I/O wait registry rejected register or signal.
    IoWait(IoWaitError),
}

/// Harness stub for [`Opcode::AwaitIo`]: park with [`ParkReason::AwaitIo`] and register the
/// pending [`IoHandle`].
///
/// Call when a running context executes `AWAIT_IO` and the operation would block a worker.
/// The context PC remains at the suspend site until [`IoWaitRegistry::signal_ready`] resumes it.
///
/// # Errors
///
/// Returns [`AwaitIoHarnessError`] when park or registry registration fails.
pub fn dispatch_await_io_harness(
    pool: &WorkerPool,
    registry: &mut IoWaitRegistry,
    context: ContextId,
    operands: AwaitIoOperands,
) -> Result<(), AwaitIoHarnessError> {
    pool.park_running(context, ParkReason::AwaitIo)
        .map_err(AwaitIoHarnessError::Scheduler)?;
    registry
        .register(operands.io_handle(), context, pool)
        .map_err(AwaitIoHarnessError::IoWait)?;
    let _ = operands.io_kind;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::scheduler::{ContextState, ParkReason, WorkerPool};

    fn signal_ready_ok(
        registry: &mut IoWaitRegistry,
        handle: IoHandle,
        pool: &mut WorkerPool,
        label: &str,
    ) -> ContextId {
        match registry.signal_ready(handle, pool) {
            Ok(context) => context,
            Err(err) => panic!("{label}: {err:?}"),
        }
    }

    fn lock_registry<'a>(
        registry: &'a Arc<Mutex<IoWaitRegistry>>,
        label: &str,
    ) -> std::sync::MutexGuard<'a, IoWaitRegistry> {
        match registry.lock() {
            Ok(guard) => guard,
            Err(err) => panic!("{label}: registry lock poisoned: {err}"),
        }
    }

    #[test]
    fn await_io_operands_parse_from_instruction() {
        let inst = Instruction {
            opcode: Opcode::AwaitIo,
            operands: vec![1, 42],
        };
        let ops = match AwaitIoOperands::from_instruction(&inst) {
            Ok(ops) => ops,
            Err(err) => panic!("parse await_io operands: {err:?}"),
        };
        assert_eq!(ops.io_kind, 1);
        assert_eq!(ops.request_id, 42);
        assert_eq!(ops.io_handle(), IoHandle::from_index(42));
    }

    #[test]
    fn await_io_operands_reject_wrong_opcode() {
        let inst = Instruction {
            opcode: Opcode::Return,
            operands: vec![1, 2],
        };
        assert!(matches!(
            AwaitIoOperands::from_instruction(&inst),
            Err(AwaitIoHarnessError::WrongOpcode(Opcode::Return))
        ));
    }

    #[test]
    fn synthetic_harness_hits_await_io_and_signal_ready_completes_on_worker() {
        let registry = Arc::new(Mutex::new(IoWaitRegistry::new()));
        let mut pool = WorkerPool::with_io_registry(2, Arc::clone(&registry));
        let handle = IoHandle::from_index(99);
        let operands = AwaitIoOperands {
            io_kind: 1,
            request_id: 99,
        };

        let ctx = pool.spawn_await_io_on_first_run(3, operands);

        pool.wait_until(|status| status.parked_count >= 1);

        assert_eq!(
            pool.state_of(ctx),
            Some(ContextState::Parked(ParkReason::AwaitIo))
        );
        assert_eq!(
            lock_registry(&registry, "pending after park").pending_count(),
            1
        );
        assert_eq!(
            lock_registry(&registry, "context_for after park").context_for(handle),
            Some(ctx)
        );

        let resumed = {
            let mut reg = lock_registry(&registry, "signal_ready");
            signal_ready_ok(&mut reg, handle, &mut pool, "external I/O readiness wakeup")
        };
        assert_eq!(resumed, ctx);

        pool.wait_all_done();

        assert_eq!(pool.state_of(ctx), Some(ContextState::Done));
        assert_eq!(pool.parked_count(), 0);
        assert_eq!(
            lock_registry(&registry, "pending after done").pending_count(),
            0
        );
        pool.shutdown();
    }

    fn spawn_await_io_batch(
        pool: &WorkerPool,
        specs: &[(u32, u32)],
    ) -> Vec<(ContextId, IoHandle, AwaitIoOperands)> {
        specs
            .iter()
            .map(|&(steps, request_id)| {
                let operands = AwaitIoOperands {
                    io_kind: 1,
                    request_id,
                };
                let handle = operands.io_handle();
                let ctx = pool.spawn_await_io_on_first_run(steps, operands);
                (ctx, handle, operands)
            })
            .collect()
    }

    fn wait_all_parked(pool: &WorkerPool, expected: usize, label: &str) {
        pool.wait_until(|status| status.parked_count >= expected);
        assert_eq!(
            pool.parked_count(),
            expected,
            "{label}: expected {expected} parked contexts"
        );
    }

    fn assert_all_parked_await_io(
        pool: &WorkerPool,
        contexts: &[(ContextId, IoHandle, AwaitIoOperands)],
        registry: &Arc<Mutex<IoWaitRegistry>>,
        label: &str,
    ) {
        for &(ctx, handle, _) in contexts {
            assert_eq!(
                pool.state_of(ctx),
                Some(ContextState::Parked(ParkReason::AwaitIo)),
                "{label}: context {ctx:?} should be parked for I/O"
            );
            assert_eq!(
                lock_registry(registry, label).context_for(handle),
                Some(ctx),
                "{label}: registry should map handle to context"
            );
        }
        assert_eq!(
            lock_registry(registry, label).pending_count(),
            contexts.len(),
            "{label}: registry pending count"
        );
    }

    fn wake_handles_in_order(
        registry: &Arc<Mutex<IoWaitRegistry>>,
        pool: &mut WorkerPool,
        handles: &[IoHandle],
        label: &str,
    ) {
        for &handle in handles {
            let resumed = {
                let mut reg = lock_registry(registry, label);
                signal_ready_ok(&mut reg, handle, pool, label)
            };
            assert_eq!(
                lock_registry(registry, label).context_for(handle),
                None,
                "{label}: handle should be unregistered after wakeup"
            );
            let _ = resumed;
        }
    }

    fn assert_clean_completion(
        pool: &WorkerPool,
        contexts: &[(ContextId, IoHandle, AwaitIoOperands)],
        registry: &Arc<Mutex<IoWaitRegistry>>,
        label: &str,
    ) {
        pool.wait_all_done();
        assert_eq!(pool.parked_count(), 0, "{label}: no parked contexts");
        assert_eq!(pool.runnable_count(), 0, "{label}: no runnable contexts");
        assert_eq!(
            lock_registry(registry, label).pending_count(),
            0,
            "{label}: no leaked I/O registrations"
        );
        for &(ctx, handle, _) in contexts {
            assert_eq!(
                pool.state_of(ctx),
                Some(ContextState::Done),
                "{label}: context {ctx:?} should be done"
            );
            assert!(
                !lock_registry(registry, label).is_registered(handle),
                "{label}: handle should not remain registered"
            );
        }
    }

    #[test]
    fn multi_context_await_io_wake_in_spawn_order_completes() {
        let registry = Arc::new(Mutex::new(IoWaitRegistry::new()));
        let mut pool = WorkerPool::with_io_registry(3, Arc::clone(&registry));
        let contexts = spawn_await_io_batch(&pool, &[(2, 10), (3, 11), (4, 12), (1, 13)]);

        wait_all_parked(&pool, contexts.len(), "spawn-order");
        assert_all_parked_await_io(&pool, &contexts, &registry, "spawn-order");

        let handles: Vec<IoHandle> = contexts.iter().map(|(_, handle, _)| *handle).collect();
        wake_handles_in_order(&registry, &mut pool, &handles, "spawn-order wakeup");

        assert_clean_completion(&pool, &contexts, &registry, "spawn-order");
        pool.shutdown();
    }

    #[test]
    fn multi_context_await_io_wake_reverse_order_completes() {
        let registry = Arc::new(Mutex::new(IoWaitRegistry::new()));
        let mut pool = WorkerPool::with_io_registry(4, Arc::clone(&registry));
        let contexts = spawn_await_io_batch(&pool, &[(5, 20), (2, 21), (3, 22), (4, 23), (1, 24)]);

        wait_all_parked(&pool, contexts.len(), "reverse-order");
        assert_all_parked_await_io(&pool, &contexts, &registry, "reverse-order");

        let mut handles: Vec<IoHandle> = contexts.iter().map(|(_, handle, _)| *handle).collect();
        handles.reverse();
        wake_handles_in_order(&registry, &mut pool, &handles, "reverse-order wakeup");

        assert_clean_completion(&pool, &contexts, &registry, "reverse-order");
        pool.shutdown();
    }

    #[test]
    fn multi_context_await_io_interleaved_fast_contexts_completes() {
        let registry = Arc::new(Mutex::new(IoWaitRegistry::new()));
        let mut pool = WorkerPool::with_io_registry(3, Arc::clone(&registry));

        let fast_a = pool.spawn(1);
        let io_a = spawn_await_io_batch(&pool, &[(3, 30)]);
        let fast_b = pool.spawn(2);
        let io_b = spawn_await_io_batch(&pool, &[(2, 31), (4, 32)]);
        let fast_c = pool.spawn(1);

        let mut io_contexts = io_a;
        io_contexts.extend(io_b);

        pool.wait_until(|status| {
            status.done_count >= 3 && status.parked_count >= io_contexts.len()
        });

        assert_eq!(pool.state_of(fast_a), Some(ContextState::Done));
        assert_eq!(pool.state_of(fast_b), Some(ContextState::Done));
        assert_eq!(pool.state_of(fast_c), Some(ContextState::Done));
        assert_all_parked_await_io(&pool, &io_contexts, &registry, "interleaved");

        let handles = [
            IoHandle::from_index(32),
            IoHandle::from_index(30),
            IoHandle::from_index(31),
        ];
        wake_handles_in_order(&registry, &mut pool, &handles, "interleaved wakeup");

        assert_clean_completion(&pool, &io_contexts, &registry, "interleaved");
        assert_eq!(pool.done_count(), 6);
        pool.shutdown();
    }

    #[test]
    fn multi_context_await_io_duplicate_signal_returns_not_found() {
        let registry = Arc::new(Mutex::new(IoWaitRegistry::new()));
        let mut pool = WorkerPool::with_io_registry(2, Arc::clone(&registry));
        let contexts = spawn_await_io_batch(&pool, &[(2, 40), (2, 41)]);

        wait_all_parked(&pool, contexts.len(), "duplicate-signal");
        let handle = contexts[0].1;

        {
            let mut reg = lock_registry(&registry, "first signal");
            signal_ready_ok(&mut reg, handle, &mut pool, "first wakeup");
        }

        let err = {
            let mut reg = lock_registry(&registry, "duplicate signal");
            match reg.signal_ready(handle, &mut pool) {
                Err(err) => err,
                Ok(_) => panic!("duplicate signal_ready should fail"),
            }
        };
        assert_eq!(err, IoWaitError::HandleNotFound(handle));

        let remaining = contexts[1].1;
        wake_handles_in_order(&registry, &mut pool, &[remaining], "finish remaining");

        assert_clean_completion(&pool, &contexts, &registry, "duplicate-signal");
        pool.shutdown();
    }
}
