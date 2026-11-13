//! Grid layout computation.

use super::style::*;
use super::*;

/// Compute layout using Taffy for Grid containers.
pub fn compute_taffy_grid_layout(
    children: &[ChildExtent],
    gap: [f32; 2],
    padding: [f32; 4],
    cols: usize,
) -> TaffyLayoutOutput {
    compute_taffy_grid_layout_internal(children, gap, padding, cols, false, None, |_i, child| {
        fixed_leaf_style(child.half_size)
    })
}

/// Compute layout for Grid containers with size specs and constraints.
pub fn compute_taffy_grid_layout_with_specs(
    children: &[ChildExtent],
    gap: [f32; 2],
    padding: [f32; 4],
    cols: usize,
    size_specs: &[Option<ChildSizeSpec>],
    constraints: &[SizeConstraints],
    parent_content_size: [f32; 2],
) -> TaffyLayoutOutput {
    let size_tracks_to_parent = size_specs
        .iter()
        .flatten()
        .any(|spec| matches!(spec.width, SizeSpec::Fill | SizeSpec::Percent(_)));
    let cols = cols.max(1);
    let cell_width = if size_tracks_to_parent && parent_content_size[0] > 0.0 {
        (parent_content_size[0] - padding[0] - padding[2] - gap[0] * (cols - 1) as f32)
            .max(cols as f32)
            / cols as f32
    } else {
        parent_content_size[0]
    };
    let child_parent_content_size = [cell_width, parent_content_size[1]];
    compute_taffy_grid_layout_internal(
        children,
        gap,
        padding,
        cols,
        true,
        size_tracks_to_parent.then_some(parent_content_size[0]),
        |i, child| {
            let spec = size_specs.get(i).copied().flatten().map(|mut spec| {
                if !size_tracks_to_parent || parent_content_size[0] <= 0.0 {
                    return spec;
                }
                // Grid items with an auto/fill width do not reliably stretch
                // to fixed template tracks in Taffy. Resolve their relative
                // width against this cell explicitly so nested containers see
                // the final width before measuring text.
                spec.width = match spec.width {
                    SizeSpec::Fill => SizeSpec::Fixed(cell_width),
                    SizeSpec::Percent(percent) => SizeSpec::Fixed(cell_width * percent),
                    other => other,
                };
                spec
            });
            let cons = constraints.get(i).copied().unwrap_or_default();
            build_child_style(spec, child.half_size, cons, child_parent_content_size)
        },
    )
}

fn compute_taffy_grid_layout_internal(
    children: &[ChildExtent],
    gap: [f32; 2],
    padding: [f32; 4],
    cols: usize,
    clamp_tracks: bool,
    track_budget_width: Option<f32>,
    mut child_style: impl FnMut(usize, &ChildExtent) -> Style,
) -> TaffyLayoutOutput {
    if children.is_empty() {
        return TaffyLayoutOutput {
            positions: Vec::new(),
            container_size: [0.0, 0.0],
        };
    }

    let cols = cols.max(1);
    let rows = children.len().div_ceil(cols);

    let mut taffy: TaffyTree<()> = TaffyTree::new();

    // Create Taffy nodes for all children so declaration order and spacing semantics
    // remain identical even when some children are manually positioned.
    let mut child_nodes: Vec<NodeId> = Vec::with_capacity(children.len());
    for (i, child) in children.iter().enumerate() {
        let row = i / cols;
        let col = i % cols;
        let mut style = child_style(i, child);
        style.grid_row = line((row + 1) as i16);
        style.grid_column = line((col + 1) as i16);
        let node = taffy
            .new_leaf(style)
            .expect("taffy new_leaf should succeed for valid child size");
        child_nodes.push(node);
    }

    // Compute column widths and row heights (including manual children for sizing)
    let (mut col_widths, row_heights) = compute_grid_tracks(children, cols, rows);
    if let Some(budget) = track_budget_width.filter(|width| width.is_finite() && *width > 0.0) {
        // Relative-width Grid children need final cell widths before nested
        // text can be measured. A single equal-track allocation gives every
        // column a stable share of the declared content box; wrapped height
        // then feeds back into the per-row intrinsic tracks below. Grids with
        // only intrinsic children keep their content-derived track widths.
        let content_width =
            (budget - padding[0] - padding[2] - gap[0] * (cols - 1) as f32).max(cols as f32);
        col_widths.fill(content_width / cols as f32);
    }

    let map_track = |track: f32| {
        let len = if clamp_tracks { track.max(1.0) } else { track };
        GridTemplateComponent::from_length(len)
    };

    let col_template: Vec<GridTemplateComponent<String>> =
        col_widths.iter().copied().map(map_track).collect();
    let row_template: Vec<GridTemplateComponent<String>> =
        row_heights.iter().copied().map(map_track).collect();

    // Create container style with grid
    // Don't set explicit size - let Taffy compute it from the grid template
    let container_style = Style {
        display: Display::Grid,
        grid_template_columns: col_template,
        grid_template_rows: row_template,
        gap: Size {
            width: LengthPercentage::length(gap[0]),
            height: LengthPercentage::length(gap[1]),
        },
        padding: Rect {
            left: LengthPercentage::length(padding[0]),
            right: LengthPercentage::length(padding[2]),
            top: LengthPercentage::length(padding[1]),
            bottom: LengthPercentage::length(padding[3]),
        },
        ..Default::default()
    };

    let container_node = taffy
        .new_leaf(container_style)
        .expect("taffy new_leaf should succeed for grid container style");

    // Add children; their size and constraint styles were combined with the
    // grid placement before node creation so placement cannot overwrite them.
    for child_node in &child_nodes {
        taffy
            .add_child(container_node, *child_node)
            .expect("taffy add_child should succeed for valid parent and child nodes");
    }

    // Compute layout
    taffy
        .compute_layout(container_node, Size::MAX_CONTENT)
        .expect("taffy compute_layout should succeed for grid");

    // Extract positions
    let container_layout = taffy
        .layout(container_node)
        .expect("taffy layout should exist for computed grid container");
    let container_size = [container_layout.size.width, container_layout.size.height];
    let mut results: Vec<TaffyLayoutResult> = Vec::with_capacity(children.len());

    for (i, _child) in children.iter().enumerate() {
        let child_layout = taffy
            .layout(child_nodes[i])
            .expect("taffy layout should exist for computed grid child node");

        results.push(TaffyLayoutResult {
            position: center_relative_position(container_layout, child_layout),
            size: [child_layout.size.width, child_layout.size.height],
        });
    }

    TaffyLayoutOutput {
        positions: results,
        container_size,
    }
}

/// Compute grid column widths and row heights.
fn compute_grid_tracks(children: &[ChildExtent], cols: usize, rows: usize) -> (Vec<f32>, Vec<f32>) {
    let cols = cols.max(1);
    let rows = rows.max(1);

    let mut col_widths = vec![0.0f32; cols];
    let mut row_heights = vec![0.0f32; rows];

    for (index, child) in children.iter().enumerate() {
        let (child_w, child_h) = (child.half_size[0] * 2.0, child.half_size[1] * 2.0);
        let row = index / cols;
        let col = index % cols;
        col_widths[col] = col_widths[col].max(child_w);
        row_heights[row] = row_heights[row].max(child_h);
    }

    (col_widths, row_heights)
}
