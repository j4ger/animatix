//! Rebuild scheduling with debounce.

use std::time::{Duration, Instant};

use crate::app::document::version::SourceEpoch;

/// State of the rebuild scheduler.
#[derive(Debug, Clone)]
pub enum RebuildState {
    Idle,
    Debounced {
        due_at: Instant,
        source_epoch: SourceEpoch,
    },
}

impl RebuildState {
    pub fn is_idle(&self) -> bool {
        matches!(self, RebuildState::Idle)
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, RebuildState::Debounced { .. })
    }
}
