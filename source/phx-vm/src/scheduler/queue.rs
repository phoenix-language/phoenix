//! FIFO runnable queue for scheduler-managed contexts.

use std::collections::VecDeque;

use super::context::ContextId;

/// First-in-first-out queue of context ids eligible to run.
///
/// Post-MVP workers share one or more run queues (per-worker or work-stealing); PHX-sched-0
/// uses a single in-memory queue for the single-threaded harness.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RunQueue {
    ids: VecDeque<ContextId>,
}

impl RunQueue {
    /// Returns an empty runnable queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueues `id` at the tail (spawn and resume order).
    pub fn push(&mut self, id: ContextId) {
        self.ids.push_back(id);
    }

    /// Dequeues the next runnable context id, if any.
    #[must_use]
    pub fn pop(&mut self) -> Option<ContextId> {
        self.ids.pop_front()
    }

    /// Returns whether the queue has no runnable contexts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Returns the number of contexts waiting to run.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Returns whether `id` is currently enqueued.
    #[must_use]
    pub fn contains(&self, id: ContextId) -> bool {
        self.ids.contains(&id)
    }
}
