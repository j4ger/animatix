//! Pure snap strategies for preview canvas drag interactions.
//!
//! Each strategy is a standalone function that takes the current candidate
//! position and a collection of reference values, returning the snapped
//! position (if within threshold) along with metadata for HUD rendering.

/// Result of attempting to snap a single coordinate.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapResult {
    /// The (possibly) snapped coordinate.
    pub value: f32,
    /// Whether any snap occurred.
    pub did_snap: bool,
    /// Human-readable label for the HUD (e.g. "Guide y=100").
    pub label: Option<String>,
}

impl SnapResult {
    fn unchanged(value: f32) -> Self {
        Self { value, did_snap: false, label: None }
    }
}

/// Snap a coordinate to the nearest guide line.
pub fn snap_to_guides(value: f32, guides: &[f32], threshold: f32) -> SnapResult {
    let mut best: Option<(f32, f32)> = None; // (guide_value, distance)
    for &guide in guides {
        let d = (value - guide).abs();
        if d < threshold {
            if best.as_ref().map_or(true, |(_, bd)| d < *bd) {
                best = Some((guide, d));
            }
        }
    }
    if let Some((guide, _)) = best {
        SnapResult {
            value: guide,
            did_snap: true,
            label: Some(format!("Guide {}", guide as i32)),
        }
    } else {
        SnapResult::unchanged(value)
    }
}

/// Snap a set of dragged edges to a set of reference edges.
///
/// `dragged_edges` — e.g. [left, center, right] of the dragged actor.
/// `reference_edges` — e.g. [left, center, right] of another actor.
/// `edge_labels` — human-readable labels for each reference edge.
///
/// Returns the delta to apply to the dragged position (if any).
pub fn snap_edges(
    dragged_value: f32,
    dragged_edges: &[f32],
    reference_edges: &[(f32, &str)],
    threshold: f32,
) -> SnapResult {
    let mut best: Option<(f32, f32, String)> = None; // (delta, distance, label)

    for &de in dragged_edges {
        for (re, label) in reference_edges {
            let delta = re - de;
            let d = delta.abs();
            if d < threshold && d > 0.001 {
                if best.as_ref().map_or(true, |(_, bd, _)| d < *bd) {
                    best = Some((delta, d, label.to_string()));
                }
            }
        }
    }

    if let Some((delta, _, label)) = best {
        SnapResult {
            value: dragged_value + delta,
            did_snap: true,
            label: Some(label),
        }
    } else {
        SnapResult::unchanged(dragged_value)
    }
}

/// Snap a point to another point (used for container center / keyframe).
pub fn snap_to_point(value: f32, target: f32, threshold: f32, label: String) -> SnapResult {
    let d = (value - target).abs();
    if d < threshold {
        SnapResult {
            value: target,
            did_snap: true,
            label: Some(label),
        }
    } else {
        SnapResult::unchanged(value)
    }
}

/// Snap a value to a grid.
pub fn snap_to_grid(value: f32, grid_size: f32) -> f32 {
    if grid_size > 0.0 {
        (value / grid_size).round() * grid_size
    } else {
        value
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_to_guide_within_threshold() {
        let guides = vec![100.0, 200.0, 300.0];
        let result = snap_to_guides(105.0, &guides, 10.0);
        assert!(result.did_snap);
        assert_eq!(result.value, 100.0);
        assert_eq!(result.label, Some("Guide 100".to_string()));
    }

    #[test]
    fn snap_to_guide_outside_threshold() {
        let guides = vec![100.0, 200.0];
        let result = snap_to_guides(150.0, &guides, 10.0);
        assert!(!result.did_snap);
        assert_eq!(result.value, 150.0);
        assert_eq!(result.label, None);
    }

    #[test]
    fn snap_to_guide_prefers_nearest() {
        let guides = vec![100.0, 115.0];
        // 112 is 12 away from 100, 3 away from 115 → should snap to 115
        let result = snap_to_guides(112.0, &guides, 15.0);
        assert!(result.did_snap);
        assert_eq!(result.value, 115.0);
    }

    #[test]
    fn snap_to_guide_empty_guides() {
        let result = snap_to_guides(50.0, &[], 10.0);
        assert!(!result.did_snap);
        assert_eq!(result.value, 50.0);
    }

    #[test]
    fn snap_edges_finds_match() {
        // dragged actor center at 100, half-width 20 → edges at [80, 100, 120]
        // reference actor center at 150, half-width 30 → edges at [120, 150, 180]
        // 120 (dragged right) aligns with 120 (reference left), delta = 0
        // but delta > 0.001 filter rejects it. Let's use offset values.
        let dragged_edges = [80.0, 100.0, 120.0];
        let reference_edges = &[(125.0, "other right")];
        let result = snap_edges(100.0, &dragged_edges, reference_edges, 10.0);
        assert!(result.did_snap);
        assert_eq!(result.value, 105.0); // 100 + (125 - 120)
        assert_eq!(result.label, Some("other right".to_string()));
    }

    #[test]
    fn snap_edges_no_match() {
        let dragged_edges = [0.0, 50.0, 100.0];
        let reference_edges = &[(200.0, "far")];
        let result = snap_edges(50.0, &dragged_edges, reference_edges, 10.0);
        assert!(!result.did_snap);
        assert_eq!(result.value, 50.0);
    }

    #[test]
    fn snap_edges_ignores_zero_delta() {
        // dragged right edge at 120, reference left edge at 120 → delta = 0
        let dragged_edges = [120.0];
        let reference_edges = &[(120.0, "aligned")];
        let result = snap_edges(100.0, &dragged_edges, reference_edges, 10.0);
        assert!(!result.did_snap); // filtered by delta > 0.001
    }

    #[test]
    fn snap_to_point_within_threshold() {
        let result = snap_to_point(105.0, 100.0, 10.0, "center".to_string());
        assert!(result.did_snap);
        assert_eq!(result.value, 100.0);
        assert_eq!(result.label, Some("center".to_string()));
    }

    #[test]
    fn snap_to_point_outside_threshold() {
        let result = snap_to_point(150.0, 100.0, 10.0, "center".to_string());
        assert!(!result.did_snap);
        assert_eq!(result.value, 150.0);
    }

    #[test]
    fn snap_to_grid_positive() {
        assert_eq!(snap_to_grid(105.0, 20.0), 100.0);
        assert_eq!(snap_to_grid(115.0, 20.0), 120.0);
        assert_eq!(snap_to_grid(100.0, 20.0), 100.0); // already on grid
    }

    #[test]
    fn snap_to_grid_negative() {
        assert_eq!(snap_to_grid(-105.0, 20.0), -100.0);
        assert_eq!(snap_to_grid(-115.0, 20.0), -120.0);
    }

    #[test]
    fn snap_to_grid_zero_size() {
        assert_eq!(snap_to_grid(123.0, 0.0), 123.0);
    }
}
