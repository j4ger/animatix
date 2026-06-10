//! Rebuild scheduling with debounce.

use std::time::Instant;

use crate::app::document::version::SourceEpoch;

/// State of the rebuild scheduler.
#[derive(Debug, Clone)]
#[allow(dead_code)]
/// RebuildState tracks debounce scheduling in the frame pipeline, not yet connected.
pub enum RebuildState {
    Idle,
    Debounced {
        due_at: Instant,
        source_epoch: SourceEpoch,
    },
}

#[allow(dead_code)]
impl RebuildState {
    pub fn is_idle(&self) -> bool {
        matches!(self, RebuildState::Idle)
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, RebuildState::Debounced { .. })
    }
}
