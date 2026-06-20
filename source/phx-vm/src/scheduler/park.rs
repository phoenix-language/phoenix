//! Park reasons for scheduler-managed execution contexts.

/// Why a context was parked and removed from the runnable queue.
///
/// Matches the post-MVP execution-context state machine in
/// `docs/design/features/vm-linear.md` (`ParkedAwaitIO`, `ParkedAwaitMessage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParkReason {
    /// Waiting for file, network, or timer readiness.
    AwaitIo,
    /// Waiting for an explicit actor mailbox delivery.
    AwaitMessage,
}
