//! Generation-tagged derived caches for hot-path access.
//!
//! These caches are rebuilt from a `DocumentSnapshot` and carry the
//! snapshot's generation and source epoch for staleness detection.

use kurbo::Rect;
use std::collections::HashMap;

use crate::app::document::version::{DocumentGeneration, SourceEpoch, Versioned};

/// All derived caches for hot-path access.
///
/// Consumers check `generation` and `source_epoch` against the
/// current `DocumentSnapshot` before using these values.
#[derive(Clone)]
#[allow(dead_code)] // DerivedCaches will be populated from DocumentSnapshot for hot-path access once panel migration is complete.
pub struct DerivedCaches {
    pub generation: DocumentGeneration,
    pub source_epoch: SourceEpoch,
    pub actor_labels: Versioned<Vec<String>>,
    // Type reflects the nested actor → keyframes → (time, label) structure needed for timeline hit-testing.
    #[allow(clippy::type_complexity)]
    pub actor_keyframes: Versioned<Vec<(String, Vec<(u64, &'static str)>)>>,
    pub hit_regions: Versioned<Vec<(String, Rect)>>,
    pub actor_bounds: Versioned<HashMap<String, Rect>>,
}
