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
/// An empty `child_baselines` slice leaves the Taffy cross-axis positions intact.
pub fn compute_taffy_linear_layout_with_baselines(
    children: &[ChildExtent],
    layout_type: LayoutType,
    gap: [f32; 2],
    padding: [f32; 4],
    align: &str,
    child_baselines: &[f32],
    vertical_align: &str,
) -> TaffyLayoutOutput {
    compute_taffy_linear_layout_with_specs(
        children,
        layout_type,
        gap,
        padding,
        align,
        child_baselines,
        vertical_align,
        &[],
        &[],
        [0.0, 0.0],
        None,
    )
}

/// Solve Row/Col flow with optional child size specs and an optional definite
/// cross axis. An intrinsic cross axis is measured by Taffy.
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
    cross_size: Option<f32>,
) -> TaffyLayoutOutput {
    let _stage = (size_specs.is_empty() && constraints.is_empty())
        .then(|| crate::perf::ScopedStage::new(crate::perf::stage::LAYOUT));
    debug_assert!(layout_type == LayoutType::Row || layout_type == LayoutType::Col);

    if children.is_empty() {
        return TaffyLayoutOutput {
            positions: Vec::new(),
            container_size: [0.0, 0.0],
        };
    }

    let mut taffy: TaffyTree<()> = TaffyTree::new();
    if cross_size.is_some() {
        // The actual child geometry can be fractional (especially text).
        // Keep its layout edge on the same coordinate as the authored edge.
        taffy.disable_rounding();
    }
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
            width: if !is_row {
                cross_size.map_or(Dimension::auto(), Dimension::length)
            } else if needs_parent_main_size && parent_content_size[0] > 0.0 {
                Dimension::length(parent_content_size[0])
            } else {
                Dimension::auto()
            },
            height: if is_row {
                cross_size.map_or(Dimension::auto(), Dimension::length)
            } else if needs_parent_main_size && parent_content_size[1] > 0.0 {
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

    if vertical_align == "baseline" && child_baselines.len() >= positions.len() {
        let baseline_y = positions
            .iter()
            .zip(child_baselines)
            .map(|(child, offset)| child.position[1] + offset)
            .fold(f32::NEG_INFINITY, f32::max);
        for (child, offset) in positions.iter_mut().zip(child_baselines) {
            child.position[1] = baseline_y - offset;
        }
    }

    TaffyLayoutOutput {
        positions,
        container_size,
    }
}
