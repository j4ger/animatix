//! Taffy style construction helpers.

use super::*;

pub(super) fn build_child_style(
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
        let abs = spec.width.resolve_absolute(parent_content_size[0]);
        let clamped = if let Some(min_w) = constraints.min_width {
            abs.max(min_w)
        } else {
            abs
        };
        let clamped = if let Some(max_w) = constraints.max_width {
            clamped.min(max_w)
        } else {
            clamped
        };
        Dimension::length(clamped)
    } else {
        spec.width.resolve(parent_content_size[0])
    };

    // Resolve height similarly
    let height_is_fill = matches!(spec.height, SizeSpec::Fill);
    let height_dim = if height_is_fill {
        Dimension::auto()
    } else if parent_content_size[1] > 0.0 {
        let abs = spec.height.resolve_absolute(parent_content_size[1]);
        let clamped = if let Some(min_h) = constraints.min_height {
            abs.max(min_h)
        } else {
            abs
        };
        let clamped = if let Some(max_h) = constraints.max_height {
            clamped.min(max_h)
        } else {
            clamped
        };
        Dimension::length(clamped)
    } else {
        spec.height.resolve(parent_content_size[1])
    };

    Style {
        size: Size {
            width: width_dim,
            height: height_dim,
        },
        flex_grow: if width_is_fill || height_is_fill {
            1.0
        } else {
            0.0
        },
        min_size: Size {
            width: constraints.min_width.map_or(Dimension::auto(), |v| {
                if v == 0.0 {
                    Dimension::auto()
                } else {
                    Dimension::length(v)
                }
            }),
            height: constraints.min_height.map_or(Dimension::auto(), |v| {
                if v == 0.0 {
                    Dimension::auto()
                } else {
                    Dimension::length(v)
                }
            }),
        },
        max_size: Size {
            width: constraints.max_width.map_or(Dimension::auto(), |v| {
                if v.is_infinite() || v.is_nan() {
                    Dimension::auto()
                } else {
                    Dimension::length(v)
                }
            }),
            height: constraints.max_height.map_or(Dimension::auto(), |v| {
                if v.is_infinite() || v.is_nan() {
                    Dimension::auto()
                } else {
                    Dimension::length(v)
                }
            }),
        },
        ..Default::default()
    }
}

pub(super) fn fixed_leaf_style(half_size: [f32; 2]) -> Style {
    Style {
        size: Size {
            width: Dimension::length(half_size[0] * 2.0),
            height: Dimension::length(half_size[1] * 2.0),
        },
        ..Default::default()
    }
}

pub(super) fn center_relative_position(container: &Layout, child: &Layout) -> [f32; 2] {
    [
        child.location.x + child.size.width / 2.0 - container.size.width / 2.0,
        child.location.y + child.size.height / 2.0 - container.size.height / 2.0,
    ]
}
