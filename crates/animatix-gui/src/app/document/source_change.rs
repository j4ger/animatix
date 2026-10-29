//! Metadata about a source text change.

use crate::app::document::version::SourceEpoch;

/// Summary of a source text change.
#[derive(Debug, Clone)]
pub struct SourceChange {
    pub after_epoch: SourceEpoch,
}
