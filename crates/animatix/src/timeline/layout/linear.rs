//! Linear (Row/Col) layout computation with size specs and constraints.

use super::ChildExtent;
use crate::timeline::ContainerMetadata;
use crate::timeline::taffy_layout::{
    ChildSizeSpec, SizeConstraints, TaffyLayoutResult, compute_taffy_linear_layout_with_specs,
};

pub(super) fn compute_linear_layout_results_with_specs(
    metadata: &ContainerMetadata,
    children: &[ChildExtent],
    child_baselines: &[f32],
    size_specs: &[Option<ChildSizeSpec>],
    constraints: &[SizeConstraints],
    parent_content_size: [f32; 2],
    cross_size: Option<f32>,
) -> Vec<TaffyLayoutResult> {
    compute_taffy_linear_layout_with_specs(
        children,
        metadata.layout_type,
        metadata.gap,
        metadata.padding,
        &metadata.align,
        child_baselines,
        &metadata.vertical_align,
        size_specs,
        constraints,
        parent_content_size,
        cross_size,
    )
    .positions
}
