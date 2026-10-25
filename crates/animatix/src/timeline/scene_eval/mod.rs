pub(crate) mod bounds;
pub(crate) mod containers;
#[cfg(test)]
mod tests;

pub(crate) use bounds::{node_local_bounds, transform_rect_bbox};

mod frame;
mod render;
mod solo;
mod transform;

use super::{
    DEFAULT_LAYOUT_HALF_SIZE, DebugRenderOptions, EntityId, EvalError, PlacementMode,
    PositionBinding, SceneDimensions, Timeline, TrackAccessor, Value, VelloPath,
    resolve_bound_position,
};
use crate::renderer::types::TextPath;

#[derive(Clone, Copy)]
pub(crate) struct NodeTransform {
    pub half_size: [f32; 2],
    pub opacity: f32,
    pub scale: f64,
    pub local_transform: kurbo::Affine,
}

/// One per-entity transform-cache entry.
///
/// `Timed` (the entity has keyframes) keys on (time, parent transform), so it
/// only serves repeated evaluation of the same instant. `Static` (the entity
/// has no keyframes on any track, so its own transform inputs are
/// time-invariant) drops the time and keys on the parent state plus the
/// layout position instead — playback reuses it across times, which skips
/// the per-node property sampling for every non-animated actor.
#[derive(Clone, Copy)]
pub(crate) enum TransformCacheEntry {
    Timed(u64, [f64; 6], NodeTransform),
    Static([f64; 6], f32, Option<[f32; 2]>, NodeTransform),
}
