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
