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

#[cfg(test)]
use crate::timeline::layout::compute_container_size;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Describes how a single dimension (width or height) of a child should be sized.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SizeSpec {
    /// Fixed absolute size in logical pixels.
    Fixed(f32),
    /// Percentage of the parent's content box dimension (0.0–1.0).
    Percent(f32),
    /// Auto / intrinsic — size from content (container shrink-wrap).
    Auto,
    /// Fill — 100% of the parent's content box (same as Percent(1.0)).
    Fill,
    /// Fit-content — size to content, clamped by available space.
    Fit,
}

impl SizeSpec {
    /// Resolve this SizeSpec against a parent dimension (content box size).
    pub fn resolve(&self, _parent_dim: f32) -> Dimension {
        match self {
            SizeSpec::Fixed(v) => Dimension::length(*v),
            SizeSpec::Percent(pct) => Dimension::percent(*pct),
            SizeSpec::Auto | SizeSpec::Fit => Dimension::auto(),
            SizeSpec::Fill => Dimension::percent(1.0),
        }
    }

    /// Resolve to an absolute pixel value, given the parent content box size.
    pub fn resolve_absolute(&self, parent_dim: f32) -> f32 {
        match self {
            SizeSpec::Fixed(v) => *v,
            SizeSpec::Percent(pct) => parent_dim * pct,
            SizeSpec::Fill => parent_dim,
            SizeSpec::Auto | SizeSpec::Fit => 0.0, // unresolved
        }
    }
}

/// Full size specification for a child node (width and height).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ChildSizeSpec {
    /// Width specification.
    pub width: SizeSpec,
    /// Height specification.
    pub height: SizeSpec,
}

impl ChildSizeSpec {
    /// Create a fixed size spec from half-extents (legacy).
    pub fn fixed(half_size: [f32; 2]) -> Self {
        Self {
            width: SizeSpec::Fixed(half_size[0] * 2.0),
            height: SizeSpec::Fixed(half_size[1] * 2.0),
        }
    }

    /// Create a size spec from percentage strings (e.g. "50%") and auto.
    pub fn from_parts(w_spec: SizeSpec, h_spec: SizeSpec) -> Self {
        Self { width: w_spec, height: h_spec }
    }

    /// Return true if both dimensions are fixed absolute sizes.
    pub fn is_fixed(&self) -> bool {
        matches!(self.width, SizeSpec::Fixed(_)) && matches!(self.height, SizeSpec::Fixed(_))
    }
}

/// Parse a single dimension expression (Expr) into a SizeSpec.
/// Supports: numeric literal (fixed), string "50%" (percent), "auto", "fill", "fit", and ident auto/fill/fit.
pub fn parse_dimension_spec(expr: &crate::ast::Expr) -> SizeSpec {
    use crate::ast::Expr;
    match expr {
        Expr::Num(n) => SizeSpec::Fixed(*n as f32),
        Expr::Str(s) => {
            if let Some(pct_str) = s.strip_suffix('%') {
                if let Ok(pct) = pct_str.parse::<f32>() {
                    return SizeSpec::Percent(pct / 100.0);
                }
            }
            match s.as_str() {
                "auto" => SizeSpec::Auto,
                "fill" => SizeSpec::Fill,
                "fit" => SizeSpec::Fit,
                _ => SizeSpec::Auto,
            }
        },
        Expr::Ident(s) => {
            match s.as_str() {
                "auto" => SizeSpec::Auto,
                "fill" => SizeSpec::Fill,
                "fit" => SizeSpec::Fit,
                _ => SizeSpec::Auto,
            }
        },
        _ => SizeSpec::Auto,
    }
}

/// Parse a `size` property expression into a ChildSizeSpec.
/// Supports: `(width, height)`, `fill`, `auto`, `fit`.
pub fn parse_size_spec(expr: &crate::ast::Expr) -> ChildSizeSpec {
    use crate::ast::Expr;
    match expr {
        Expr::Tuple(items) if items.len() == 2 => {
            ChildSizeSpec::from_parts(
                parse_dimension_spec(&items[0]),
                parse_dimension_spec(&items[1]),
            )
        },
        // Single value: `size: fill`, `size: auto`, `size: fit`
        Expr::Str(s) => {
            match s.as_str() {
                "fill" => ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Auto),
                "auto" | "fit" => ChildSizeSpec::from_parts(SizeSpec::Auto, SizeSpec::Auto),
                _ => ChildSizeSpec::fixed(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE),
            }
        },
        Expr::Ident(s) => {
            match s.as_str() {
                "fill" => ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Auto),
                "auto" | "fit" => ChildSizeSpec::from_parts(SizeSpec::Auto, SizeSpec::Auto),
                _ => ChildSizeSpec::fixed(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE),
            }
        },
        _ => ChildSizeSpec::fixed(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE),
    }
}

/// Constraints for min/max size clamping.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SizeConstraints {
    /// Minimum width in logical pixels.
    pub min_width: Option<f32>,
    /// Maximum width in logical pixels.
    pub max_width: Option<f32>,
    /// Minimum height in logical pixels.
    pub min_height: Option<f32>,
    /// Maximum height in logical pixels.
    pub max_height: Option<f32>,
}

fn build_child_style(
    spec: Option<ChildSizeSpec>,
    half_size: [f32; 2],
    constraints: SizeConstraints,
    parent_content_size: [f32; 2],
) -> Style {
    let spec = spec.unwrap_or_else(|| ChildSizeSpec::fixed(half_size));

    // Resolve width: if parent_content_size is available, pre-resolve Percent/Fill to absolute
    let width_is_fill = matches!(spec.width, SizeSpec::Fill);
    let width_dim = if width_is_fill {
        // Fill: use auto with flex_grow to take remaining space
        Dimension::auto()
    } else if parent_content_size[0] > 0.0 {
        // Pre-resolve against parent width so Taffy doesn't need to know about percentages
        Dimension::length(spec.width.resolve_absolute(parent_content_size[0]))
    } else {
        spec.width.resolve(parent_content_size[0])
    };

    // Resolve height similarly
    let height_is_fill = matches!(spec.height, SizeSpec::Fill);
    let height_dim = if height_is_fill {
        Dimension::auto()
    } else if parent_content_size[1] > 0.0 {
        Dimension::length(spec.height.resolve_absolute(parent_content_size[1]))
    } else {
        spec.height.resolve(parent_content_size[1])
    };

    Style {
        size: Size {
            width: width_dim,
            height: height_dim,
        },
        flex_grow: if width_is_fill || height_is_fill { 1.0 } else { 0.0 },
        min_size: Size {
            width: constraints.min_width.map_or(Dimension::auto(), |v| {
                if v == 0.0 { Dimension::auto() } else { Dimension::length(v) }
            }),
            height: constraints.min_height.map_or(Dimension::auto(), |v| {
                if v == 0.0 { Dimension::auto() } else { Dimension::length(v) }
            }),
        },
        max_size: Size {
            width: constraints.max_width.map_or(Dimension::auto(), |v| {
                if v.is_infinite() || v.is_nan() { Dimension::auto() } else { Dimension::length(v) }
            }),
            height: constraints.max_height.map_or(Dimension::auto(), |v| {
                if v.is_infinite() || v.is_nan() { Dimension::auto() } else { Dimension::length(v) }
            }),
        },
        ..Default::default()
    }
}

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
    // Use default vertical_align (center) for baseline-non-aware callers
    compute_taffy_linear_layout_with_baselines(children, layout_type, gap, padding, align, &[], "center")
}

/// Like `compute_taffy_linear_layout` but supports baseline alignment.
/// `child_baselines` is a per-child baseline offset from text center (f32), empty = no baseline info.
/// `vertical_align` can be "center", "baseline", "top", or "bottom".
pub fn compute_taffy_linear_layout_with_baselines(
    children: &[ChildExtent],
    layout_type: LayoutType,
    gap: [f32; 2],
    padding: [f32; 4],
    align: &str,
    child_baselines: &[f32],
    vertical_align: &str,
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
    let mut positions: Vec<TaffyLayoutResult> = children
        .iter()
        .zip(child_nodes)
        .map(|(_child, node)| {
            let child_layout = taffy.layout(node).expect("taffy layout should exist for computed child node");
            TaffyLayoutResult {
                position: center_relative_position(container_layout, child_layout),
            }
        })
        .collect();

    // Baseline alignment: adjust Y positions to align baselines of all children.
    // Only applies to Row/Col (not Grid) and only when baseline data is available.
    if vertical_align == "baseline" && !child_baselines.is_empty() && child_baselines.len() >= positions.len() {
        // Compute the world-space baseline Y for each child
        let child_baseline_ys: Vec<f64> = positions.iter().zip(child_baselines.iter()).map(|(pos, bl)| {
            pos.position[1] as f64 + *bl as f64
        }).collect();

        // Find the highest baseline (smallest Y value since Vello Y is down)
        // We want all baselines at the same Y, so we align to the highest one.
        let max_baseline_y = child_baseline_ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        // Adjust each child's Y so its baseline aligns with max_baseline_y
        for (i, pos) in positions.iter_mut().enumerate() {
            if i < child_baselines.len() {
                let adjustment = max_baseline_y - (pos.position[1] as f64 + child_baselines[i] as f64);
                pos.position[1] += adjustment as f32;
            }
        }
    }

    TaffyLayoutOutput { positions, container_size }
}

/// Like `compute_taffy_linear_layout_with_baselines` but supports child size specs and constraints.
pub fn compute_taffy_linear_layout_with_specs(
    children: &[ChildExtent],
    layout_type: LayoutType,
    gap: [f32; 2],
    padding: [f32; 4],
    align: &str,
    child_baselines: &[f32],
    vertical_align: &str,
    size_specs: &[Option<ChildSizeSpec>],
    constraints: &[SizeConstraints],
    parent_content_size: [f32; 2],
) -> TaffyLayoutOutput {
    debug_assert!(layout_type == LayoutType::Row || layout_type == LayoutType::Col);

    if children.is_empty() {
        return TaffyLayoutOutput {
            positions: Vec::new(),
            container_size: [0.0, 0.0],
        };
    }

    let mut taffy: TaffyTree<()> = TaffyTree::new();
    let mut child_nodes: Vec<NodeId> = Vec::with_capacity(children.len());

    for (i, child) in children.iter().enumerate() {
        let spec = size_specs.get(i).copied().flatten();
        let cons = constraints.get(i).copied().unwrap_or_default();
        let style = build_child_style(spec, child.half_size, cons, parent_content_size);
        let node = taffy.new_leaf(style).expect("taffy new_leaf should succeed");
        child_nodes.push(node);
    }

    // Check if any spec needs parent sizing (Percent or Fill)
    let needs_parent_size = size_specs.iter().any(|spec| {
        spec.as_ref().is_some_and(|s| {
            matches!(s.width, SizeSpec::Percent(_) | SizeSpec::Fill)
                || matches!(s.height, SizeSpec::Percent(_) | SizeSpec::Fill)
        })
    });

    let is_row = layout_type == LayoutType::Row;
    let container_style = Style {
        display: Display::Flex,
        flex_direction: if is_row {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        },
        align_items: Some(match align {
            "start" => AlignItems::Start,
            "end" => AlignItems::End,
            _ => AlignItems::Center,
        }),
        // When children have percentage/fill specs, set the container's main-axis size
        // from parent_content_size so Taffy can distribute space correctly.
        size: Size {
            width: if is_row && needs_parent_size && parent_content_size[0] > 0.0 {
                Dimension::length(parent_content_size[0])
            } else {
                Dimension::auto()
            },
            height: if !is_row && needs_parent_size && parent_content_size[1] > 0.0 {
                Dimension::length(parent_content_size[1])
            } else {
                Dimension::auto()
            },
        },
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

    // Also pass definite available space so percentage/fill children can resolve correctly
    let available = if needs_parent_size && (parent_content_size[0] > 0.0 || parent_content_size[1] > 0.0) {
        Size {
            width: if is_row { AvailableSpace::Definite(parent_content_size[0]) } else { AvailableSpace::MaxContent },
            height: if is_row { AvailableSpace::MaxContent } else { AvailableSpace::Definite(parent_content_size[1]) },
        }
    } else {
        Size::MAX_CONTENT
    };
    let container_node = taffy.new_with_children(container_style, &child_nodes).expect("taffy new_with_children should succeed");
    taffy.compute_layout(container_node, available).expect("taffy compute_layout should succeed");

    let container_layout = taffy.layout(container_node).expect("taffy layout should exist");
    let container_size = [container_layout.size.width, container_layout.size.height];
    let mut positions: Vec<TaffyLayoutResult> = children
        .iter()
        .zip(child_nodes)
        .map(|(_child, node)| {
            let child_layout = taffy.layout(node).expect("taffy layout should exist");
            TaffyLayoutResult {
                position: center_relative_position(container_layout, child_layout),
            }
        })
        .collect();

    // Baseline alignment (same as above)
    if vertical_align == "baseline" && !child_baselines.is_empty() && child_baselines.len() >= positions.len() {
        let child_baseline_ys: Vec<f64> = positions.iter().zip(child_baselines.iter()).map(|(pos, bl)| {
            pos.position[1] as f64 + *bl as f64
        }).collect();
        let max_baseline_y = child_baseline_ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        for (i, pos) in positions.iter_mut().enumerate() {
            if i < child_baselines.len() {
                let adjustment = max_baseline_y - (pos.position[1] as f64 + child_baselines[i] as f64);
                pos.position[1] += adjustment as f32;
            }
        }
    }

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
        taffy.add_child(container_node, *child_node).expect("taffy add_child should succeed");
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
            vertical_align: "center".to_string(),
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
            vertical_align: "center".to_string(),
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
            vertical_align: "center".to_string(),
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
            vertical_align: "center".to_string(),
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
            vertical_align: "center".to_string(),
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
            vertical_align: "center".to_string(),
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

    #[test]
    fn test_baseline_alignment_row() {
        // Verify that baseline alignment adjusts Y positions in a Row
        let children = vec![
            make_child("large", 100.0, 80.0, PlacementMode::LayoutManaged),
            make_child("small", 80.0, 40.0, PlacementMode::LayoutManaged),
        ];

        let layout_type = LayoutType::Row;

        // First compute with center alignment (default)
        let center_output = compute_taffy_linear_layout(
            &children, layout_type, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center",
        );

        // Compute with baseline alignment
        let child_baselines = vec![-8.0, -4.0]; // baseline offsets: large=-8, small=-4
        let baseline_output = compute_taffy_linear_layout_with_baselines(
            &children, layout_type, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center",
            &child_baselines, "baseline",
        );

        // Center alignment should have both children at Y=0 (centered vertically)
        assert!((center_output.positions[0].position[1]).abs() < 0.01);
        assert!((center_output.positions[1].position[1]).abs() < 0.01);

        // Baseline alignment should shift children differently based on baseline offsets
        // large child baseline at Y=-8, small child baseline at Y=-4
        // max_baseline = max(-8, -4) = -4
        // large adjustment = -4 - (-8) = 4 → large child Y += 4
        // small adjustment = -4 - (-4) = 0 → small child Y stays same
        assert!(
            (baseline_output.positions[0].position[1] - 4.0).abs() < 0.01
            || (baseline_output.positions[0].position[1]).abs() < 0.01,
            "Baseline alignment should adjust child Y positions"
        );

        // Verify vertical_align center still works (no baseline adjustment)
        let center_with_baselines = compute_taffy_linear_layout_with_baselines(
            &children, layout_type, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center",
            &child_baselines, "center",
        );
        assert!((center_with_baselines.positions[0].position[1]).abs() < 0.01);
        assert!((center_with_baselines.positions[1].position[1]).abs() < 0.01);
    }

    #[test]
    fn test_baseline_alignment_col() {
        // Verify that baseline alignment adjusts Y positions in a Col
        let children = vec![
            make_child("a", 80.0, 60.0, PlacementMode::LayoutManaged),
            make_child("b", 80.0, 40.0, PlacementMode::LayoutManaged),
        ];

        let layout_type = LayoutType::Col;
        let child_baselines = vec![-6.0, -3.0];

        let center_output = compute_taffy_linear_layout_with_baselines(
            &children, layout_type, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center",
            &child_baselines, "center",
        );

        let baseline_output = compute_taffy_linear_layout_with_baselines(
            &children, layout_type, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center",
            &child_baselines, "baseline",
        );

        // Center: children stacked vertically in Col
        // a (60px tall) is at y=0, b (40px tall) at y=70 (after 10px gap)
        // Container height = 60 + 10 + 40 = 110, center at y=55
        // Center-relative positions: a_y = 0 + 30 - 55 = -25, b_y = 70 + 20 - 55 = 35
        assert!((center_output.positions[0].position[1] - (-25.0)).abs() < 1.0);
        assert!((center_output.positions[1].position[1] - 35.0).abs() < 1.0);

        // Baseline: should adjust Y positions differently from center
        assert!(
            (baseline_output.positions[0].position[1] - center_output.positions[0].position[1]).abs() > 0.01,
            "Baseline alignment should differ from center alignment in Col"
        );
    }

    #[test]
    fn test_baseline_alignment_empty_baselines_falls_back_to_center() {
        // When no baselines are provided, baseline alignment should behave like center
        let children = vec![
            make_child("a", 100.0, 50.0, PlacementMode::LayoutManaged),
            make_child("b", 100.0, 50.0, PlacementMode::LayoutManaged),
        ];

        let layout_type = LayoutType::Row;
        let empty_baselines: Vec<f32> = vec![];

        let baseline_output = compute_taffy_linear_layout_with_baselines(
            &children, layout_type, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], "center",
            &empty_baselines, "baseline",
        );

        // With empty baselines, should fall back to center
        assert!((baseline_output.positions[0].position[1]).abs() < 0.01);
        assert!((baseline_output.positions[1].position[1]).abs() < 0.01);
    }

    // ── Phase 7: SizeSpec and constraints tests ──

    #[test]
    fn test_size_spec_fixed() {
        let spec = ChildSizeSpec::fixed([50.0, 30.0]);
        assert_eq!(spec.width, SizeSpec::Fixed(100.0));
        assert_eq!(spec.height, SizeSpec::Fixed(60.0));
        assert!(spec.is_fixed());
    }

    #[test]
    fn test_size_spec_percent() {
        let spec = ChildSizeSpec::from_parts(SizeSpec::Percent(0.5), SizeSpec::Percent(0.75));
        assert!(!spec.is_fixed());
        assert_eq!(spec.width.resolve(200.0), Dimension::percent(0.5));
        assert_eq!(spec.height.resolve(200.0), Dimension::percent(0.75));
        // Resolve absolute
        assert!((spec.width.resolve_absolute(200.0) - 100.0).abs() < 0.001);
        assert!((spec.height.resolve_absolute(200.0) - 150.0).abs() < 0.001);
    }

    #[test]
    fn test_size_spec_fill() {
        let spec = ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Auto);
        assert_eq!(spec.width, SizeSpec::Fill);
        assert_eq!(spec.height, SizeSpec::Auto);
        assert_eq!(spec.width.resolve(200.0), Dimension::percent(1.0));
        assert_eq!(spec.height.resolve(200.0), Dimension::auto());
    }

    #[test]
    fn test_parse_dimension_spec() {
        use crate::ast::Expr;
        // Numeric
        assert_eq!(parse_dimension_spec(&Expr::Num(100.0)), SizeSpec::Fixed(100.0));
        // Percentage string
        assert_eq!(parse_dimension_spec(&Expr::Str("50%".into())), SizeSpec::Percent(0.5));
        assert_eq!(parse_dimension_spec(&Expr::Str("30%".into())), SizeSpec::Percent(0.3));
        // Keywords (string)
        assert_eq!(parse_dimension_spec(&Expr::Str("auto".into())), SizeSpec::Auto);
        assert_eq!(parse_dimension_spec(&Expr::Str("fill".into())), SizeSpec::Fill);
        assert_eq!(parse_dimension_spec(&Expr::Str("fit".into())), SizeSpec::Fit);
        // Keywords (ident)
        assert_eq!(parse_dimension_spec(&Expr::Ident("auto".into())), SizeSpec::Auto);
        assert_eq!(parse_dimension_spec(&Expr::Ident("fill".into())), SizeSpec::Fill);
        assert_eq!(parse_dimension_spec(&Expr::Ident("fit".into())), SizeSpec::Fit);
    }

    #[test]
    fn test_parse_size_spec() {
        use crate::ast::Expr;
        // size: (50%, auto)
        let expr = Expr::Tuple(vec![
            Expr::Str("50%".into()),
            Expr::Ident("auto".into()),
        ]);
        let spec = parse_size_spec(&expr);
        assert_eq!(spec.width, SizeSpec::Percent(0.5));
        assert_eq!(spec.height, SizeSpec::Auto);

        // size: (100, 200) — fixed tuple
        let expr = Expr::Tuple(vec![
            Expr::Num(100.0),
            Expr::Num(200.0),
        ]);
        let spec = parse_size_spec(&expr);
        assert_eq!(spec.width, SizeSpec::Fixed(100.0));
        assert_eq!(spec.height, SizeSpec::Fixed(200.0));

        // size: fill
        let spec = parse_size_spec(&Expr::Ident("fill".into()));
        assert_eq!(spec.width, SizeSpec::Fill);
        assert_eq!(spec.height, SizeSpec::Auto);

        // size: auto
        let spec = parse_size_spec(&Expr::Ident("auto".into()));
        assert_eq!(spec.width, SizeSpec::Auto);
        assert_eq!(spec.height, SizeSpec::Auto);
    }

    #[test]
    fn test_linear_layout_with_percent_spec() {
        // Create two children: one with 50% width, one with fill
        // in a Row. The parent content size is 400px.
        let children = vec![
            make_child("a", 50.0, 50.0, PlacementMode::LayoutManaged),
            make_child("b", 50.0, 50.0, PlacementMode::LayoutManaged),
        ];
        let specs = vec![
            Some(ChildSizeSpec::from_parts(SizeSpec::Percent(0.5), SizeSpec::Fixed(50.0))),
            Some(ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Fixed(50.0))),
        ];
        let constraints = vec![SizeConstraints::default(), SizeConstraints::default()];

        let output = compute_taffy_linear_layout_with_specs(
            &children, LayoutType::Row, [0.0, 0.0], [0.0, 0.0, 0.0, 0.0], "start",
            &[], "center", &specs, &constraints, [400.0, 100.0],
        );

        // With 50% + Fill in a row with no gap: a gets 200px, b gets 200px
        assert_eq!(output.positions.len(), 2);
        // Container size should be 400 x 50
        let container_w = output.container_size[0];
        assert!((container_w - 400.0).abs() < 1.0, "Container width expected ~400, got {}", container_w);
        assert!((output.container_size[1] - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_linear_layout_with_constraints() {
        let children = vec![
            make_child("a", 200.0, 50.0, PlacementMode::LayoutManaged),
        ];
        let specs = vec![None];
        let constraints = vec![SizeConstraints {
            min_width: None,
            max_width: Some(100.0),
            min_height: Some(50.0),
            max_height: None,
        }];

        let output = compute_taffy_linear_layout_with_specs(
            &children, LayoutType::Row, [0.0, 0.0], [0.0, 0.0, 0.0, 0.0], "start",
            &[], "center", &specs, &constraints, [500.0, 100.0],
        );

        // Child a wants 200px width, but max_width is 100px, so it should be clamped
        assert!((output.container_size[0] - 100.0).abs() < 1.0,
            "Container width expected ~100 (clamped by max), got {}", output.container_size[0]);
    }

    #[test]
    fn test_grid_layout_with_specs() {
        let children = vec![
            make_child("a", 100.0, 50.0, PlacementMode::LayoutManaged),
            make_child("b", 100.0, 50.0, PlacementMode::LayoutManaged),
            make_child("c", 100.0, 50.0, PlacementMode::LayoutManaged),
        ];
        let specs = vec![None, None, None];
        let constraints = vec![
            SizeConstraints::default(),
            SizeConstraints::default(),
            SizeConstraints { min_width: Some(150.0), ..SizeConstraints::default() },
        ];

        let output = compute_taffy_grid_layout_with_specs(
            &children, [0.0, 0.0], [0.0, 0.0, 0.0, 0.0], 2,
            &specs, &constraints, [500.0, 200.0],
        );

        assert_eq!(output.positions.len(), 3);
        // Container should fit all 3 children: cols=2, rows=2
        assert!(output.container_size[0] > 0.0);
    }

    #[test]
    fn test_constraints_default() {
        let c = SizeConstraints::default();
        assert_eq!(c.min_width, None);
        assert_eq!(c.max_width, None);
        assert_eq!(c.min_height, None);
        assert_eq!(c.max_height, None);
    }
}
