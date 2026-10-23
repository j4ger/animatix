//! Grid layout computation with size specs and constraints.

use super::ChildExtent;
use crate::timeline::taffy_layout::{
    ChildSizeSpec, SizeConstraints, TaffyLayoutResult, compute_taffy_grid_layout,
    compute_taffy_grid_layout_with_specs,
};

pub(super) fn compute_grid_layout_results_with_specs(
    children: &[ChildExtent],
    gap: [f32; 2],
    padding: [f32; 4],
    cols: usize,
    size_specs: &[Option<ChildSizeSpec>],
    constraints: &[SizeConstraints],
    parent_content_size: [f32; 2],
) -> Vec<TaffyLayoutResult> {
    let output = if size_specs.is_empty() && constraints.is_empty() {
        compute_taffy_grid_layout(children, gap, padding, cols)
    } else {
        compute_taffy_grid_layout_with_specs(
            children,
            gap,
            padding,
            cols,
            size_specs,
            constraints,
            parent_content_size,
        )
    };
    output.positions
}
