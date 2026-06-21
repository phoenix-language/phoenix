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
            signal_ready_ok(
                &mut reg,
                handle,
                &mut pool,
                "external I/O readiness wakeup",
            )
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
}
