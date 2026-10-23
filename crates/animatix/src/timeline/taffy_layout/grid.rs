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
    for child in children {
        let node = taffy
            .new_leaf(fixed_leaf_style(child.half_size))
            .expect("taffy new_leaf should succeed for valid child size");
        child_nodes.push(node);
    }

    // Compute column widths and row heights (including manual children for sizing)
    let (col_widths, row_heights) = compute_grid_tracks(children, cols, rows);

    // Create grid template using helper functions
    let col_template: Vec<GridTemplateComponent<String>> =
        col_widths.iter().map(|&w| GridTemplateComponent::from_length(w)).collect();
    let row_template: Vec<GridTemplateComponent<String>> =
        row_heights.iter().map(|&h| GridTemplateComponent::from_length(h)).collect();

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

    // Add children with grid placement
    for (i, child_node) in child_nodes.iter().enumerate() {
        taffy
            .add_child(container_node, *child_node)
            .expect("taffy add_child should succeed for valid parent and child nodes");
        // Set grid placement using the line() helper from prelude
        let row = i / cols;
        let col = i % cols;
        taffy
            .set_style(
                *child_node,
                Style {
                    grid_row: line((row + 1) as i16),
                    grid_column: line((col + 1) as i16),
                    ..Default::default()
                },
            )
            .expect("taffy set_style should succeed for valid child node and grid placement");
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
    if children.is_empty() {
        return TaffyLayoutOutput {
            positions: Vec::new(),
            container_size: [0.0, 0.0],
        };
    }

    let cols = cols.max(1);
    let rows = children.len().div_ceil(cols);

    let mut taffy: TaffyTree<()> = TaffyTree::new();

    let mut child_nodes: Vec<NodeId> = Vec::with_capacity(children.len());
    for (i, child) in children.iter().enumerate() {
        let spec = size_specs.get(i).copied().flatten();
        let cons = constraints.get(i).copied().unwrap_or_default();
        let style = build_child_style(spec, child.half_size, cons, parent_content_size);
        let node = taffy.new_leaf(style).expect("taffy new_leaf should succeed");
        child_nodes.push(node);
    }

    // Compute column widths and row heights
    let (col_widths, row_heights) = compute_grid_tracks(children, cols, rows);

    let col_template: Vec<GridTemplateComponent<String>> = col_widths
        .iter()
        .map(|&w| GridTemplateComponent::from_length(w.max(1.0)))
        .collect();
    let row_template: Vec<GridTemplateComponent<String>> = row_heights
        .iter()
        .map(|&h| GridTemplateComponent::from_length(h.max(1.0)))
        .collect();

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

    let container_node = taffy.new_leaf(container_style).expect("taffy new_leaf should succeed");

    for (i, child_node) in child_nodes.iter().enumerate() {
        taffy
            .add_child(container_node, *child_node)
            .expect("taffy add_child should succeed");
        let row = i / cols;
        let col = i % cols;
        taffy
            .set_style(
                *child_node,
                Style {
                    grid_row: line((row + 1) as i16),
                    grid_column: line((col + 1) as i16),
                    ..Default::default()
                },
            )
            .expect("taffy set_style should succeed");
    }

    taffy
        .compute_layout(container_node, Size::MAX_CONTENT)
        .expect("taffy compute_layout should succeed for grid");

    let container_layout = taffy.layout(container_node).expect("taffy layout should exist");
    let container_size = [container_layout.size.width, container_layout.size.height];
    let mut results: Vec<TaffyLayoutResult> = Vec::with_capacity(children.len());

    for (i, _child) in children.iter().enumerate() {
        let child_layout = taffy.layout(child_nodes[i]).expect("taffy layout should exist");
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
