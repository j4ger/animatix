//! Metadata about a source text change.

use crate::app::document::version::SourceEpoch;

/// Summary of a source text change.
#[derive(Debug, Clone)]
#[allow(dead_code)] // SourceChange fields reserved for future diagnostics/metrics
/// SourceChange is constructed during source mutations but fields are for future
/// diagnostics/metrics.
pub struct SourceChange {
    pub before_epoch: SourceEpoch,
    pub after_epoch: SourceEpoch,
    pub source_len: usize,
}
