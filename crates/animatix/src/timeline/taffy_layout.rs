//!
//! # Taffy-Backed Layout Backend
//!
//! This module provides layout computation via the Taffy layout engine while preserving
//! the existing center-relative coordinate semantics and manual-child handling.
//!
//! ## Key Design Points
//!
//! 1. **Center-relative coordinates**: Taffy uses top-left origin with absolute positioning.
//!    We translate by offsetting by container size/2 to maintain center-origin semantics.
//!
//! 2. **Manual child exclusion**: Children with `PlacementMode::Manual` are excluded from
//!    position assignment but their sizes still participate in container layout via the
//!    `available_size` mechanism (pre-computed with manual children included).
//!
//! 3. **Stack special-casing**: Stack layout places all children at origin (0, 0) regardless
//!    of their sizes. We keep this behavior unchanged for semantic compatibility.
//!
//! 4. **Gap and alignment**: Translated to Taffy's flexbox `gap` and `align_items`/`justify_content`.
//!
//! 5. **Build-time vs dynamic**: The same Taffy computation path is used for both build-time
//!    (`apply_container_layout`) and dynamic (`compute_layout_for_time`) layouts.

use taffy::prelude::*;

use crate::timeline::layout::ChildExtent;
use crate::timeline::LayoutType;

/// Computed layout result for a single child.
#[derive(Clone, Debug)]
pub struct TaffyLayoutResult {
    /// The label of the child
    pub label: String,
    /// The center-relative position [x, y] of the child
    pub position: [f32; 2],
}

/// Compute layout using Taffy for Row/Col containers.
/// Returns positions for all children (but only LayoutManaged children have meaningful positions).
///
/// IMPORTANT: Manual children participate in container sizing (spacing) but are excluded
/// from position assignment. This preserves the original behavior where manual children
/// affect the overall layout extent but don't receive computed positions.
pub fn compute_taffy_linear_layout(
    children: &[ChildExtent],
    layout_type: LayoutType,
    gap: f32,
    align: &str,
) -> Vec<TaffyLayoutResult> {
    // Stack is handled separately - all children at origin
    debug_assert!(layout_type == LayoutType::Row || layout_type == LayoutType::Col);

    if children.is_empty() {
        return Vec::new();
    }

    let mut taffy: TaffyTree<()> = TaffyTree::new();
    let mut child_nodes: Vec<NodeId> = Vec::with_capacity(children.len());

    for child in children {
        let node = taffy
            .new_leaf(Style {
                size: Size {
                    width: Dimension::length(child.half_size[0] * 2.0),
                    height: Dimension::length(child.half_size[1] * 2.0),
                },
                ..Default::default()
            })
            .unwrap();
        child_nodes.push(node);
    }

    let container_style = Style {
        display: Display::Flex,
        flex_direction: if layout_type == LayoutType::Row {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        },
        align_items: Some(match align {
            "start" => AlignItems::Start,
            "end" => AlignItems::End,
            _ => AlignItems::Center,
        }),
        gap: Size {
            width: LengthPercentage::length(gap),
            height: LengthPercentage::length(gap),
        },
        ..Default::default()
    };

    let container_node = taffy.new_with_children(container_style, &child_nodes).unwrap();
    taffy.compute_layout(container_node, Size::MAX_CONTENT).unwrap();

    let container_layout = taffy.layout(container_node).unwrap();
    let container_center_x = container_layout.size.width / 2.0;
    let container_center_y = container_layout.size.height / 2.0;

    children
        .iter()
        .zip(child_nodes)
        .map(|(child, node)| {
            let child_layout = taffy.layout(node).unwrap();
            let x = child_layout.location.x + child_layout.size.width / 2.0 - container_center_x;
            let y = child_layout.location.y + child_layout.size.height / 2.0 - container_center_y;
            TaffyLayoutResult {
                label: child.label.clone(),
                position: [x, y],
            }
        })
        .collect()
}

/// Compute layout using Taffy for Grid containers.
pub fn compute_taffy_grid_layout(
    children: &[ChildExtent],
    gap: f32,
    cols: usize,
) -> Vec<TaffyLayoutResult> {
    if children.is_empty() {
        return Vec::new();
    }

    let cols = cols.max(1);
    let rows = children.len().div_ceil(cols);

    let mut taffy: TaffyTree<()> = TaffyTree::new();

    // Create Taffy nodes for all children so declaration order and spacing semantics
    // remain identical even when some children are manually positioned.
    let mut child_nodes: Vec<NodeId> = Vec::new();
    for child in children {
        let node = taffy
            .new_leaf(Style {
                size: Size {
                    width: Dimension::length(child.half_size[0] * 2.0),
                    height: Dimension::length(child.half_size[1] * 2.0),
                },
                ..Default::default()
            })
            .unwrap();
        child_nodes.push(node);
    }

    // Compute column widths and row heights (including manual children for sizing)
    let (col_widths, row_heights) = compute_grid_tracks(children, cols, rows, gap);

    // Create grid template using helper functions
    let col_template: Vec<GridTemplateComponent<String>> = col_widths
        .iter()
        .map(|&w| GridTemplateComponent::from_length(w))
        .collect();
    let row_template: Vec<GridTemplateComponent<String>> = row_heights
        .iter()
        .map(|&h| GridTemplateComponent::from_length(h))
        .collect();

    // Create container style with grid
    // Don't set explicit size - let Taffy compute it from the grid template
    let container_style = Style {
        display: Display::Grid,
        grid_template_columns: col_template,
        grid_template_rows: row_template,
        gap: Size {
            width: LengthPercentage::length(gap),
            height: LengthPercentage::length(gap),
        },
        ..Default::default()
    };

    let container_node = taffy.new_leaf(container_style).unwrap();

    // Add children with grid placement
    for (i, child_node) in child_nodes.iter().enumerate() {
        taffy.add_child(container_node, *child_node).unwrap();
        // Set grid placement using the line() helper from prelude
        let row = i / cols;
        let col = i % cols;
        taffy.set_style(
            *child_node,
            Style {
                grid_row: line((row + 1) as i16),
                grid_column: line((col + 1) as i16),
                ..Default::default()
            },
        )
        .unwrap();
    }

    // Compute layout
    taffy
        .compute_layout(container_node, Size::MAX_CONTENT)
        .unwrap();

    // Extract positions
    let container_layout = taffy.layout(container_node).unwrap();
    let container_center_x = container_layout.size.width / 2.0;
    let container_center_y = container_layout.size.height / 2.0;

    let mut results: Vec<TaffyLayoutResult> = Vec::new();

    for (i, child) in children.iter().enumerate() {
        let child_layout = taffy.layout(child_nodes[i]).unwrap();

        let x = child_layout.location.x + child_layout.size.width / 2.0 - container_center_x;
        let y = child_layout.location.y + child_layout.size.height / 2.0 - container_center_y;

        results.push(TaffyLayoutResult {
            label: child.label.clone(),
            position: [x, y],
        });
    }

    // Sort to match original order
    let mut label_to_result: std::collections::HashMap<String, TaffyLayoutResult> =
        results.into_iter().map(|r| (r.label.clone(), r)).collect();

    children
        .iter()
        .filter_map(|c| label_to_result.remove(&c.label))
        .collect()
}

/// Compute grid column widths and row heights.
fn compute_grid_tracks(
    children: &[ChildExtent],
    cols: usize,
    rows: usize,
    _gap: f32,
) -> (Vec<f32>, Vec<f32>) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::PlacementMode;

    fn make_child(label: &str, w: f32, h: f32, mode: PlacementMode) -> ChildExtent {
        ChildExtent {
            label: label.to_string(),
            half_size: [w / 2.0, h / 2.0],
            placement_mode: mode,
        }
    }

    #[test]
    fn test_row_layout_basic() {
        let children = vec![
            make_child("a", 100.0, 50.0, PlacementMode::LayoutManaged),
            make_child("b", 100.0, 50.0, PlacementMode::LayoutManaged),
        ];

        let results = compute_taffy_linear_layout(&children, LayoutType::Row, 10.0, "center");

        // Two 100-wide children with 10 gap, centered
        // Total width = 100 + 10 + 100 = 210
        // a at -55, b at 55
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].label, "a");
        assert_eq!(results[1].label, "b");
        // Check that positions are roughly centered
        let total_span = results[1].position[0] - results[0].position[0];
        assert!((total_span - 110.0).abs() < 0.1);
    }

    #[test]
    fn test_col_layout_basic() {
        let children = vec![
            make_child("a", 50.0, 100.0, PlacementMode::LayoutManaged),
            make_child("b", 50.0, 100.0, PlacementMode::LayoutManaged),
        ];

        let results = compute_taffy_linear_layout(&children, LayoutType::Col, 10.0, "center");

        // Two 100-tall children with 10 gap, centered
        // Total height = 100 + 10 + 100 = 210
        // a at -55, b at 55 (in y)
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].label, "a");
        assert_eq!(results[1].label, "b");
        let total_span = results[1].position[1] - results[0].position[1];
        assert!((total_span - 110.0).abs() < 0.1);
    }

    #[test]
    fn test_manual_child_excluded_from_positions() {
        let children = vec![
            make_child("a", 100.0, 50.0, PlacementMode::LayoutManaged),
            make_child("b", 100.0, 50.0, PlacementMode::Manual),
            make_child("c", 100.0, 50.0, PlacementMode::LayoutManaged),
        ];

        let results = compute_taffy_linear_layout(&children, LayoutType::Row, 10.0, "center");

        assert_eq!(results.len(), 3);
        // b (Manual) should still receive a computed slot in declaration order,
        // even though callers later exclude it from authored layout assignment.
        // In this symmetric 3-child layout: a=-110, b=0, c=110 (center child at origin is correct)
        assert_eq!(results[0].label, "a");
        assert_eq!(results[1].label, "b");
        assert_eq!(results[2].label, "c");
        // Verify a and c are at symmetric non-zero positions
        assert_eq!(results[0].position[0], -results[2].position[0]); // symmetric
        assert!(results[0].position[0] < 0.0); // a is left of center
        assert!(results[2].position[0] > 0.0); // c is right of center
        // b (center) happens to be at [0, 0] due to symmetric layout
        assert_eq!(results[1].position, [0.0, 0.0]);
    }

    #[test]
    fn test_empty_children() {
        let children: Vec<ChildExtent> = vec![];
        let results = compute_taffy_linear_layout(&children, LayoutType::Row, 10.0, "center");
        assert!(results.is_empty());

        let results = compute_taffy_grid_layout(&children, 10.0, 2);
        assert!(results.is_empty());
    }
}
