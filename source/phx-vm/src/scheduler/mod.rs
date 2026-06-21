//! VM scheduler data structures and single-threaded park/resume harness.
//!
//! Post-MVP this module drives M:N context scheduling on a worker pool; worker threads
//! dequeue [`RunnableContext`] values from a shared [`RunQueue`], execute bytecode, and
//! park on schedulable I/O or mailbox waits. PHX-sched-0 introduces the in-tree types
//! and a cooperative harness for unit tests — no Phoenix syntax, no std I/O, no opcodes.
//!
//! Design references: `docs/design/features/vm-linear.md` (execution context state machine),
//! `docs/design/features/runtime-transparency.md`, `docs/design/features/concurrency.md`.
//!
//! ## Single-threaded harness
//!
//! [`SingleThreadScheduler`] simulates cooperative scheduling on one OS thread:
//! spawn contexts, dequeue via [`SingleThreadScheduler::run_next`], step or
//! [`RunningGuard::park`], then [`SingleThreadScheduler::resume`] after a wakeup.
//!
//! ## Public API
//!
//! | Type | Role |
//! | --- | --- |
//! | [`ContextId`] | Stable handle for a schedulable unit |
//! | [`RunnableContext`] | Context metadata and harness step budget |
//! | [`ContextState`] | `Runnable` / `Running` / `Parked` / `Done` lifecycle |
//! | [`ParkReason`] | Why a context was parked (`AwaitIo`, `AwaitMessage`) |
//! | [`RunQueue`] | FIFO runnable queue |
//! | [`SingleThreadScheduler`] | Spawn, run, park, resume harness |
//! | [`SchedulerError`] | Invalid park/resume transitions |
//! | [`IoWaitRegistry`] | Tracks [`ParkReason::AwaitIo`] waits and wakeups (PHX-sched-2) |
//! | [`WorkerPool`] | M:N worker threads + shared [`RunQueue`] (PHX-sched-4) |

mod context;
mod harness;
mod io_wait;
mod park;
mod queue;
mod worker_pool;

pub use context::{ContextId, ContextState, RunnableContext, StepOutcome};
pub use harness::{RunningGuard, SchedulerError, SingleThreadScheduler};
pub use io_wait::{IoHandle, IoWaitError, IoWaitRegistry};
pub use park::ParkReason;
pub use queue::RunQueue;
pub use worker_pool::{WorkerPool, WorkerPoolStatus};
