//! Generation-tagged derived caches for hot-path access.
//!
//! These caches are rebuilt from a `DocumentSnapshot` and carry the
//! snapshot's generation and source epoch for staleness detection.

use kurbo::Rect;
use std::collections::HashMap;

use crate::app::document::version::{DocumentGeneration, SourceEpoch, Versioned};

/// Per-scene cached data, indexed by scene name.
/// For single-scene documents, the key is an empty string.
pub type SceneMap<T> = HashMap<String, T>;

/// All derived caches for hot-path access.
///
/// Consumers check `generation` and `source_epoch` against the
/// current `DocumentSnapshot` before using these values.
#[derive(Clone)]
pub struct DerivedCaches {
    pub generation: DocumentGeneration,
    pub source_epoch: SourceEpoch,
    pub actor_labels: Versioned<Vec<String>>,
    pub actor_keyframes: Versioned<Vec<(String, Vec<(u64, &'static str)>)>>,
    pub hit_regions: Versioned<Vec<(String, Rect)>>,
    pub actor_bounds: Versioned<HashMap<String, Rect>>,
}
