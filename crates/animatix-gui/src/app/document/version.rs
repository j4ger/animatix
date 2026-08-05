//! Version types for source and document state tracking.
//!
//! These types are used to detect stale derived state and enable
//! background rebuilds with cancellation.

use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonically increasing epoch for source text changes.
/// Incremented on every source mutation (editor edit, undo, reload, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceEpoch(pub u64);

impl SourceEpoch {
    pub fn initial() -> Self {
        Self(1)
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Hash of source text at a given point in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceHash(pub u64);

/// Monotonically increasing generation for document snapshots.
/// Incremented on every accepted rebuild (successful or failed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentGeneration(pub u64);

impl DocumentGeneration {
    pub fn initial() -> Self {
        Self(1)
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Cancellation token for background rebuild workers.
/// Set `shared_generation` to a new value to cancel older tokens.
#[derive(Clone)]
pub struct CancellationToken {
    generation: u64,
    shared_latest: std::sync::Arc<AtomicU64>,
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.shared_latest.load(Ordering::Relaxed) != self.generation
    }
}

/// Source side of a cancellation token pair.
pub struct CancellationSource {
    shared_latest: std::sync::Arc<AtomicU64>,
}

impl CancellationSource {
    /// Create a new cancellation source with an initial token at generation 0.
    pub fn new() -> Self {
        Self {
            shared_latest: std::sync::Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a cancellation token bound to the current generation.
    /// Tokens created before the next `cancel()` call will not be cancelled.
    pub fn token(&self) -> CancellationToken {
        CancellationToken {
            generation: self.shared_latest.load(Ordering::Relaxed),
            shared_latest: self.shared_latest.clone(),
        }
    }

    /// Cancel all tokens derived from this source.
    pub fn cancel(&self, new_generation: u64) {
        self.shared_latest.store(new_generation, Ordering::Relaxed);
    }
}
