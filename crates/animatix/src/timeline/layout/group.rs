//! Resolve explicit relative dimensions through structural groups without placing children.

use super::*;
use crate::timeline::EntityId;
use crate::timeline::taffy_layout::SizeSpec;

pub(super) fn clamp_group_axis(
    geometry: &crate::timeline::animation_track::GeometryTracks,
    time_ms: u64,
    axis: usize,
    size: f32,
) -> f32 {
    let (minimum, maximum) = if axis == 0 {
        (
            geometry.min_width.get(time_ms, 0.0),
            geometry.max_item_width.get(time_ms, f32::INFINITY),
        )
    } else {
        (
            geometry.min_height.get(time_ms, 0.0),
            geometry.max_height.get(time_ms, f32::INFINITY),
        )
    };
    size.max(minimum).min(maximum)
}

impl Timeline {
    /// Whether a structural subtree needs the constraint-aware layout walk.
    pub(crate) fn group_needs_layout_reflow(&self, id: EntityId) -> bool {
        let mut pending = vec![id];
        let mut seen = std::collections::HashSet::new();
        while let Some(node) = pending.pop() {
            if !seen.insert(node) {
                return true; // The reflow walk diagnoses a malformed cycle.
            }
            for child in &self.world.hierarchy(node).children {
                if self.world.container_layout.get(*child).is_some()
                    || self.world.geometry(*child).size_spec.is_some_and(|s| s.needs_parent_box())
                {
                    return true;
                }
                if self.world.caps(*child).is_group_like() {
                    pending.push(*child);
                }
            }
        }
        false
    }

    /// Descendants of an unfinished layout/group wait for its resolved box.
    pub(crate) fn defer_nested_text_width(&self, id: EntityId) -> bool {
        let Some(parent) = self.world.hierarchy(id).parent else {
            return false;
        };
        self.world.caps(parent).is_group_like()
            || matches!(self.world.actor_type(parent), "Col" | "Grid")
            || (self.world.actor_type(parent) == "Row"
                && self.world.geometry(id).size_spec.is_some_and(|s| s.needs_parent_box()))
    }

    /// Complete a Group after its children build, or defer to its parent layout.
    pub(crate) fn finalize_group_layout(
        &mut self,
        label: &str,
        time_ms: u64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        self.invalidate_frame_cache();
        let Some(id) = self.world.get_id(label) else {
            return;
        };
        let has_layout_parent = self.world.hierarchy(id).parent.is_some_and(|parent| {
            self.world.caps(parent).is_group_like()
                || matches!(self.world.actor_type(parent), "Row" | "Col" | "Grid")
        });
        // The scene is a definite parent box for a top-level relative Group,
        // including generated wrappers around full-screen backdrop components.
        let assigned = if self.world.hierarchy(id).parent.is_none() {
            let defaults = crate::timeline::SceneDimensions::default();
            let (width, height) = self.resolution.unwrap_or((defaults.width, defaults.height));
            self.resolve_group_relative_child(
                id,
                [Some(width as f32), Some(height as f32)],
                time_ms,
                diagnostics,
            )
        } else {
            [false; 2]
        };
        if !has_layout_parent
            && (self.group_needs_layout_reflow(id) || assigned.into_iter().any(|axis| axis))
        {
            self.reflow_group_children(id, time_ms, diagnostics, assigned);
        } else {
            self.publish_group_measured_extent(label, time_ms);
        }
    }

    pub(super) fn resolve_group_relative_child(
        &mut self,
        child: EntityId,
        parent_box: [Option<f32>; 2],
        time_ms: u64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> [bool; 2] {
        let geometry = self.world.geometry(child);
        let Some(spec) = geometry.size_spec else {
            return [false; 2];
        };
        let mut half = geometry
            .layout_size_get(time_ms)
            .or_else(|| geometry.layout_size_last())
            .unwrap_or([0.0; 2]);
        let minimum = [
            geometry.min_width.get(time_ms, 0.0),
            geometry.min_height.get(time_ms, 0.0),
        ];
        let maximum = [
            geometry.max_item_width.get(time_ms, f32::INFINITY),
            geometry.max_height.get(time_ms, f32::INFINITY),
        ];
        let mut resolved = [false; 2];
        let mut missing = false;
        for (axis, dimension) in [spec.width, spec.height].into_iter().enumerate() {
            if !matches!(dimension, SizeSpec::Fill | SizeSpec::Percent(_)) {
                continue;
            }
            if let Some(size) = parent_box[axis].filter(|size| size.is_finite() && *size >= 0.0) {
                half[axis] =
                    dimension.resolve_absolute(size).max(minimum[axis]).min(maximum[axis]) / 2.0;
                resolved[axis] = true;
            } else {
                missing = true;
            }
        }
        if missing {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCode::LayoutSizeFallback,
                DiagnosticPhase::Build,
                "Relative size through Group has no definite parent axis; declare a Group size or request a resolved parent layout slot. Earlier intrinsic measurements are retained.",
            ).with_subject(self.world.label(child).unwrap_or("<unknown>")));
        }
        if resolved.into_iter().any(|resolved| resolved) {
            let geometry = self.world.geometry.get_mut(child);
            geometry
                .ensure_layout_size(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
                .add_keyframe(time_ms, half, Easing::Linear);
            geometry.size.ensure(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
                time_ms,
                half,
                Easing::Linear,
            );
        }
        resolved
    }
}
