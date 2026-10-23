//! Linear (Row/Col) layout computation with size specs and constraints.

use super::ChildExtent;
use crate::timeline::taffy_layout::{
    ChildSizeSpec, SizeConstraints, TaffyLayoutResult, compute_taffy_linear_layout_with_baselines,
    compute_taffy_linear_layout_with_specs,
};
use crate::timeline::{ContainerMetadata, LayoutType};

pub(super) fn compute_linear_layout_results_with_specs(
    children: &[ChildExtent],
    is_row: bool,
    gap: [f32; 2],
    padding: [f32; 4],
    align: &str,
    metadata: &ContainerMetadata,
    child_baselines: &[f32],
    size_specs: &[Option<ChildSizeSpec>],
    constraints: &[SizeConstraints],
    parent_content_size: [f32; 2],
) -> Vec<TaffyLayoutResult> {
    let layout_type = if is_row {
        LayoutType::Row
    } else {
        LayoutType::Col
    };

    let output = if size_specs.is_empty() && constraints.is_empty() {
        // Legacy path: no specs
        compute_taffy_linear_layout_with_baselines(
            children,
            layout_type,
            gap,
            padding,
            align,
            child_baselines,
            &metadata.vertical_align,
        )
    } else {
        compute_taffy_linear_layout_with_specs(
            children,
            layout_type,
            gap,
            padding,
            align,
            child_baselines,
            &metadata.vertical_align,
            size_specs,
            constraints,
            parent_content_size,
        )
    };
    output.positions
}
