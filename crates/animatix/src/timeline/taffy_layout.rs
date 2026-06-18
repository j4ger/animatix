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

fn fixed_leaf_style(half_size: [f32; 2]) -> Style {
    Style {
        size: Size {
            width: Dimension::length(half_size[0] * 2.0),
            height: Dimension::length(half_size[1] * 2.0),
        },
        ..Default::default()
    }
}

fn center_relative_position(container: &Layout, child: &Layout) -> [f32; 2] {
    [
        child.location.x + child.size.width / 2.0 - container.size.width / 2.0,
        child.location.y + child.size.height / 2.0 - container.size.height / 2.0,
    ]
}

/// Computed layout result for a single child.
#[derive(Clone, Debug)]
pub struct TaffyLayoutResult {
    /// The center-relative position [x, y] of the child
    pub position: [f32; 2],
}

/// Compute layout using Taffy for Row/Col containers.
/// Returns positions for all children.
///
/// IMPORTANT: Manual children participate in container sizing (spacing) but are excluded
/// from authored position assignment by the caller. This preserves the original behavior
/// where manual children affect the overall layout extent but don't receive assigned positions.
/// Result of a Taffy layout computation, including container size and child positions.
#[derive(Clone, Debug)]
pub struct TaffyLayoutOutput {
    /// Child positions relative to container center.
    pub positions: Vec<TaffyLayoutResult>,
    /// Container total size [width, height].
    pub container_size: [f32; 2],
}

pub fn compute_taffy_linear_layout(
    children: &[ChildExtent],
    layout_type: LayoutType,
    gap: [f32; 2],
    padding: [f32; 4],
    align: &str,
) -> TaffyLayoutOutput {
    // Stack is handled separately - all children at origin
    debug_assert!(layout_type == LayoutType::Row || layout_type == LayoutType::Col);

    if children.is_empty() {
        return TaffyLayoutOutput {
            positions: Vec::new(),
            container_size: [0.0, 0.0],
        };
    }

    let mut taffy: TaffyTree<()> = TaffyTree::new();
    let mut child_nodes: Vec<NodeId> = Vec::with_capacity(children.len());

    for child in children {
        let node = taffy.new_leaf(fixed_leaf_style(child.half_size)).expect("taffy new_leaf should succeed for valid child size");
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

    let container_node = taffy.new_with_children(container_style, &child_nodes).expect("taffy new_with_children should succeed for valid style and children");
    taffy.compute_layout(container_node, Size::MAX_CONTENT).expect("taffy compute_layout should succeed");

    let container_layout = taffy.layout(container_node).expect("taffy layout should exist for computed container node");
    let container_size = [container_layout.size.width, container_layout.size.height];
    let positions = children
        .iter()
        .zip(child_nodes)
        .map(|(_child, node)| {
            let child_layout = taffy.layout(node).expect("taffy layout should exist for computed child node");
            TaffyLayoutResult {
                position: center_relative_position(container_layout, child_layout),
            }
        })
        .collect();
    TaffyLayoutOutput { positions, container_size }
}

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
        let node = taffy.new_leaf(fixed_leaf_style(child.half_size)).expect("taffy new_leaf should succeed for valid child size");
        child_nodes.push(node);
    }

    // Compute column widths and row heights (including manual children for sizing)
    let (col_widths, row_heights) = compute_grid_tracks(children, cols, rows);

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

    let container_node = taffy.new_leaf(container_style).expect("taffy new_leaf should succeed for grid container style");

    // Add children with grid placement
    for (i, child_node) in child_nodes.iter().enumerate() {
        taffy.add_child(container_node, *child_node).expect("taffy add_child should succeed for valid parent and child nodes");
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
        .expect("taffy set_style should succeed for valid child node and grid placement");
    }

    // Compute layout
    taffy
        .compute_layout(container_node, Size::MAX_CONTENT)
        .expect("taffy compute_layout should succeed for grid");

    // Extract positions
    let container_layout = taffy.layout(container_node).expect("taffy layout should exist for computed grid container");
    let container_size = [container_layout.size.width, container_layout.size.height];
    let mut results: Vec<TaffyLayoutResult> = Vec::with_capacity(children.len());

    for (i, _child) in children.iter().enumerate() {
        let child_layout = taffy.layout(child_nodes[i]).expect("taffy layout should exist for computed grid child node");

        results.push(TaffyLayoutResult {
            position: center_relative_position(container_layout, child_layout),
        });
    }

    TaffyLayoutOutput { positions: results, container_size }
}

/// Compute grid column widths and row heights.
fn compute_grid_tracks(
    children: &[ChildExtent],
    cols: usize,
    rows: usize,
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

        let output = compute_taffy_linear_layout(&children, LayoutType::Row, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center");
        let results = &output.positions;

        // Two 100-wide children with 10 gap, centered
        // Total width = 100 + 10 + 100 = 210
        // a at -55, b at 55
        assert_eq!(results.len(), 2);
        // Check that positions are roughly centered
        let total_span = results[1].position[0] - results[0].position[0];
        assert!((total_span - 110.0).abs() < 0.1);
        // Container size should be computed
        assert!(output.container_size[0] > 0.0);
        assert!(output.container_size[1] > 0.0);
    }

    #[test]
    fn test_col_layout_basic() {
        let children = vec![
            make_child("a", 50.0, 100.0, PlacementMode::LayoutManaged),
            make_child("b", 50.0, 100.0, PlacementMode::LayoutManaged),
        ];

        let output = compute_taffy_linear_layout(&children, LayoutType::Col, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center");
        let results = &output.positions;

        // Two 100-tall children with 10 gap, centered
        // Total height = 100 + 10 + 100 = 210
        // a at -55, b at 55 (in y)
        assert_eq!(results.len(), 2);
        let total_span = results[1].position[1] - results[0].position[1];
        assert!((total_span - 110.0).abs() < 0.1);
        // Container size should be computed
        assert!(output.container_size[0] > 0.0);
        assert!(output.container_size[1] > 0.0);
    }

    #[test]
    fn test_manual_child_excluded_from_positions() {
        let children = vec![
            make_child("a", 100.0, 50.0, PlacementMode::LayoutManaged),
            make_child("b", 100.0, 50.0, PlacementMode::Manual),
            make_child("c", 100.0, 50.0, PlacementMode::LayoutManaged),
        ];

        let output = compute_taffy_linear_layout(&children, LayoutType::Row, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center");
        let results = &output.positions;

        assert_eq!(results.len(), 3);
        // b (Manual) should still receive a computed slot in declaration order,
        // even though callers later exclude it from authored layout assignment.
        // In this symmetric 3-child layout: a=-110, b=0, c=110 (center child at origin is correct)
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
        let output = compute_taffy_linear_layout(&children, LayoutType::Row, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center");
        assert!(output.positions.is_empty());

        let output = compute_taffy_grid_layout(&children, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], 2);
        assert!(output.positions.is_empty());
    }

    // ── Width propagation tests ──

    #[test]
    fn test_compute_available_width_col() {
        // Col: available_width = container_size - horizontal padding
        use crate::timeline::ContainerMetadata;
        use crate::timeline::LayoutType;

        let metadata = ContainerMetadata {
            layout_type: LayoutType::Col,
            gap: [0.0, 0.0],
            padding: [10.0, 5.0, 10.0, 5.0], // left=10, right=10
            align: "center".to_string(),
            cols: None,
            child_order: vec!["child".to_string()],
        };

        // Container width 500, padding left+right=20 → available = 480
        let timeline = crate::timeline::Timeline::new();
        let available = timeline.compute_available_width([500.0, 300.0], &metadata, 0);
        assert!((available - 480.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_available_width_grid() {
        use crate::timeline::ContainerMetadata;
        use crate::timeline::LayoutType;

        let metadata = ContainerMetadata {
            layout_type: LayoutType::Grid,
            gap: [10.0, 10.0],
            padding: [5.0, 5.0, 5.0, 5.0], // left=5, right=5
            align: "center".to_string(),
            cols: Some(3),
            child_order: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        };

        // Container width 400, padding left+right=10, gaps=10*2=20, cols=3
        // Available per cell = (400 - 10 - 20) / 3 = 370 / 3 = 123.33
        let timeline = crate::timeline::Timeline::new();
        let available = timeline.compute_available_width([400.0, 200.0], &metadata, 0);
        assert!((available - 123.33).abs() < 0.1);
    }

    #[test]
    fn test_compute_available_width_row_unbounded() {
        use crate::timeline::ContainerMetadata;
        use crate::timeline::LayoutType;

        let metadata = ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [0.0, 0.0],
            padding: [0.0, 0.0, 0.0, 0.0],
            align: "center".to_string(),
            cols: None,
            child_order: vec!["child".to_string()],
        };

        // Row: unbounded width
        let timeline = crate::timeline::Timeline::new();
        let available = timeline.compute_available_width([500.0, 300.0], &metadata, 0);
        assert_eq!(available, f32::MAX);
    }

    #[test]
    fn test_compute_available_width_min_1px() {
        use crate::timeline::ContainerMetadata;
        use crate::timeline::LayoutType;

        let metadata = ContainerMetadata {
            layout_type: LayoutType::Col,
            gap: [0.0, 0.0],
            padding: [100.0, 0.0, 100.0, 0.0], // left=100, right=100 (200 total)
            align: "center".to_string(),
            cols: None,
            child_order: vec!["child".to_string()],
        };

        // Container width 50, padding 200 → available would be -150, clamped to 1
        let timeline = crate::timeline::Timeline::new();
        let available = timeline.compute_available_width([50.0, 300.0], &metadata, 0);
        assert!((available - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_container_size_row() {
        use crate::timeline::ContainerMetadata;
        use crate::timeline::LayoutType;

        let children = vec![
            make_child("a", 100.0, 50.0, PlacementMode::LayoutManaged),
            make_child("b", 200.0, 50.0, PlacementMode::LayoutManaged),
        ];

        let metadata = ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [10.0, 10.0],
            padding: [5.0, 5.0, 5.0, 5.0],
            align: "center".to_string(),
            cols: None,
            child_order: vec!["a".to_string(), "b".to_string()],
        };

        let size = compute_container_size(&children, &metadata);
        // Total width should be sum of children + gap + padding
        // children: 100 + 200 = 300, gap: 10, padding left+right: 10 = 320
        assert!((size[0] - 320.0).abs() < 1.0, "Expected container width ~320, got {}", size[0]);
    }

    #[test]
    fn test_compute_container_size_col() {
        use crate::timeline::ContainerMetadata;
        use crate::timeline::LayoutType;

        let children = vec![
            make_child("a", 150.0, 100.0, PlacementMode::LayoutManaged),
            make_child("b", 100.0, 100.0, PlacementMode::LayoutManaged),
        ];

        let metadata = ContainerMetadata {
            layout_type: LayoutType::Col,
            gap: [10.0, 10.0],
            padding: [0.0, 0.0, 0.0, 0.0],
            align: "center".to_string(),
            cols: None,
            child_order: vec!["a".to_string(), "b".to_string()],
        };

        let size = compute_container_size(&children, &metadata);
        // Col width = max child width = 150
        assert!((size[0] - 150.0).abs() < 1.0, "Expected container width ~150, got {}", size[0]);
    }

    #[test]
    fn test_read_text_child_props_non_existent_returns_none() {
        let timeline = crate::timeline::Timeline::new();
        // Non-existent labels should return None
        let props = timeline.read_text_child_props("nonexistent", 0);
        assert!(props.is_none());
    }

    #[test]
    fn test_text_child_props_struct_clone_debug() {
        // Verify TextChildProps derives Clone and Debug
        let props = crate::timeline::layout::TextChildProps {
            text_kind: crate::renderer::text::TextKind::Text,
            content: "hello".to_string(),
            font_family: "sans-serif".to_string(),
            font_size: 48.0,
            font_weight: 400.0,
            font_style: "normal".to_string(),
            line_height: 1.2,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            color: [1.0, 1.0, 1.0, 1.0],
            text_align: "left".to_string(),
            overflow: "visible".to_string(),
            existing_max_width: 0.0,
        };
        let _cloned = props.clone();
        let _debug = format!("{:?}", props);
    }
}
