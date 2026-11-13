//! Linear (flexbox) layout computation.

use super::style::*;
use super::*;

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
    compute_taffy_linear_layout_with_baselines(
        children,
        layout_type,
        gap,
        padding,
        align,
        &[],
        "center",
    )
}

/// Like `compute_taffy_linear_layout` but supports baseline alignment.
/// `child_baselines` is a per-child baseline offset from text center (f32), empty = no baseline
/// info. `vertical_align` can be "center", "baseline", "top", or "bottom".
pub fn compute_taffy_linear_layout_with_baselines(
    children: &[ChildExtent],
    layout_type: LayoutType,
    gap: [f32; 2],
    padding: [f32; 4],
    align: &str,
    child_baselines: &[f32],
    vertical_align: &str,
) -> TaffyLayoutOutput {
    let _stage = crate::perf::ScopedStage::new(crate::perf::stage::LAYOUT);
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
        let node = taffy
            .new_leaf(fixed_leaf_style(child.half_size))
            .expect("taffy new_leaf should succeed for valid child size");
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

    let container_node = taffy
        .new_with_children(container_style, &child_nodes)
        .expect("taffy new_with_children should succeed for valid style and children");
    taffy
        .compute_layout(container_node, Size::MAX_CONTENT)
        .expect("taffy compute_layout should succeed");

    let container_layout = taffy
        .layout(container_node)
        .expect("taffy layout should exist for computed container node");
    let container_size = [container_layout.size.width, container_layout.size.height];
    let mut positions: Vec<TaffyLayoutResult> = children
        .iter()
        .zip(child_nodes)
        .map(|(_child, node)| {
            let child_layout =
                taffy.layout(node).expect("taffy layout should exist for computed child node");
            TaffyLayoutResult {
                position: center_relative_position(container_layout, child_layout),
                size: [child_layout.size.width, child_layout.size.height],
            }
        })
        .collect();

    // Baseline alignment: adjust Y positions to align baselines of all children.
    // Only applies to Row/Col (not Grid) and only when baseline data is available.
    if vertical_align == "baseline"
        && !child_baselines.is_empty()
        && child_baselines.len() >= positions.len()
    {
        // Compute the world-space baseline Y for each child
        let child_baseline_ys: Vec<f64> = positions
            .iter()
            .zip(child_baselines.iter())
            .map(|(pos, bl)| pos.position[1] as f64 + *bl as f64)
            .collect();

        // Find the highest baseline (smallest Y value since Vello Y is down)
        // We want all baselines at the same Y, so we align to the highest one.
        let max_baseline_y = child_baseline_ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        // Adjust each child's Y so its baseline aligns with max_baseline_y
        for (i, pos) in positions.iter_mut().enumerate() {
            if i < child_baselines.len() {
                let adjustment =
                    max_baseline_y - (pos.position[1] as f64 + child_baselines[i] as f64);
                pos.position[1] += adjustment as f32;
            }
        }
    }

    TaffyLayoutOutput {
        positions,
        container_size,
    }
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
    let is_row = layout_type == LayoutType::Row;
    let content_size = [
        (parent_content_size[0] - padding[0] - padding[2]).max(0.0),
        (parent_content_size[1] - padding[1] - padding[3]).max(0.0),
    ];

    for (i, child) in children.iter().enumerate() {
        let spec = size_specs.get(i).copied().flatten().map(|mut spec| {
            // Cross-axis fill does not participate in flex-grow. Resolve it
            // against the parent's content box so Col children fill available
            // width (and Row children can fill available height) before
            // descendants measure their own content.
            if !is_row && content_size[0] > 0.0 && matches!(spec.width, SizeSpec::Fill) {
                spec.width = SizeSpec::Fixed(content_size[0]);
            }
            if is_row && content_size[1] > 0.0 && matches!(spec.height, SizeSpec::Fill) {
                spec.height = SizeSpec::Fixed(content_size[1]);
            }
            spec
        });
        let cons = constraints.get(i).copied().unwrap_or_default();
        // Preserve existing percentage resolution against the declared parent
        // dimensions. Only the explicit cross-axis Fill dimensions above are
        // pre-resolved against the padded content box.
        let style = build_child_style(spec, child.half_size, cons, parent_content_size);
        let node = taffy.new_leaf(style).expect("taffy new_leaf should succeed");
        child_nodes.push(node);
    }

    // Only main-axis percentages/fill need a definite container extent for
    // flex distribution. Cross-axis Fill is already resolved against the
    // content box above; treating it as a parent-size dependency made a Col
    // with `size: fill` width inherit a stale fixed height instead of
    // shrink-wrapping its wrapped children.
    let needs_parent_main_size = size_specs.iter().any(|spec| {
        spec.as_ref().is_some_and(|spec| {
            if is_row {
                matches!(spec.width, SizeSpec::Percent(_) | SizeSpec::Fill)
            } else {
                matches!(spec.height, SizeSpec::Percent(_) | SizeSpec::Fill)
            }
        })
    });

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
            width: if is_row && needs_parent_main_size && parent_content_size[0] > 0.0 {
                Dimension::length(parent_content_size[0])
            } else {
                Dimension::auto()
            },
            height: if !is_row && needs_parent_main_size && parent_content_size[1] > 0.0 {
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
    let available = if needs_parent_main_size
        && (parent_content_size[0] > 0.0 || parent_content_size[1] > 0.0)
    {
        Size {
            width: if is_row {
                AvailableSpace::Definite(parent_content_size[0])
            } else {
                AvailableSpace::MaxContent
            },
            height: if is_row {
                AvailableSpace::MaxContent
            } else {
                AvailableSpace::Definite(parent_content_size[1])
            },
        }
    } else {
        Size::MAX_CONTENT
    };
    let container_node = taffy
        .new_with_children(container_style, &child_nodes)
        .expect("taffy new_with_children should succeed");
    taffy
        .compute_layout(container_node, available)
        .expect("taffy compute_layout should succeed");

    let container_layout = taffy.layout(container_node).expect("taffy layout should exist");
    let container_size = [container_layout.size.width, container_layout.size.height];
    let mut positions: Vec<TaffyLayoutResult> = children
        .iter()
        .zip(child_nodes)
        .map(|(_child, node)| {
            let child_layout = taffy.layout(node).expect("taffy layout should exist");
            TaffyLayoutResult {
                position: center_relative_position(container_layout, child_layout),
                size: [child_layout.size.width, child_layout.size.height],
            }
        })
        .collect();

    // Baseline alignment (same as above)
    if vertical_align == "baseline"
        && !child_baselines.is_empty()
        && child_baselines.len() >= positions.len()
    {
        let child_baseline_ys: Vec<f64> = positions
            .iter()
            .zip(child_baselines.iter())
            .map(|(pos, bl)| pos.position[1] as f64 + *bl as f64)
            .collect();
        let max_baseline_y = child_baseline_ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        for (i, pos) in positions.iter_mut().enumerate() {
            if i < child_baselines.len() {
                let adjustment =
                    max_baseline_y - (pos.position[1] as f64 + child_baselines[i] as f64);
                pos.position[1] += adjustment as f32;
            }
        }
    }

    TaffyLayoutOutput {
        positions,
        container_size,
    }
}
