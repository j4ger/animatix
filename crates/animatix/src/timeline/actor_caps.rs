//! Actor capabilities: the engine-visible projection of a primitive kind.
//!
//! Identity lives in `World::actor_type` — the authored type name.
//! [`ActorCaps`] is the matchable projection the engine dispatches on,
//! stored on the entity at spawn/identity time (`World::caps`) from the primitive
//! catalog, so frame-time checks are true `Copy` field reads and adding a primitive
//! never requires extending an enum. The capability *vocabulary* ([`ActorCategory`],
//! [`ShapeKind`], [`TextKind`], `ChildProcessing`) lives in `animatix-core::caps` and
//! stays a small closed set: it only grows when the engine learns a new behaviour,
//! which is exactly when the compiler should force every dispatch site to be
//! revisited.

pub use animatix_core::caps::{ActorCaps, ActorCategory, ShapeKind, TextKind};

use crate::timeline::shapes::ShapeType;

impl From<ShapeType> for ShapeKind {
    fn from(st: ShapeType) -> Self {
        match st {
            ShapeType::Rect => Self::Rect,
            ShapeType::Ellipse => Self::Ellipse,
            ShapeType::Line => Self::Line,
            ShapeType::Polygon => Self::Polygon,
            ShapeType::Path => Self::Path,
            ShapeType::Graph => Self::Rect,
            ShapeType::Plot => Self::Rect,
            ShapeType::Arrow => Self::Arrow,
        }
    }
}
