//! Node transform and world-space resolution.

use super::*;

impl Timeline {
    /// Evaluate position, size, and transform for a node.
    ///
    /// `node_overrides` — when set (from an `always` modifier), overrides for
    /// spatial properties (`at`/`position`, `shift`, `rotation`, `scale`,
    /// `opacity`, `size`, `transform`) are applied via the property registry
    /// helpers in place of the track's keyframed values.
    pub(crate) fn evaluate_node_transform(
        &self,
        track: &AnimationTrack,
        time_ms: u64,
        parent_opacity: f32,
        parent_transform: kurbo::Affine,
        scene_dimensions: SceneDimensions,
        layout_position: Option<[f32; 2]>,
        node_overrides: Option<&std::collections::HashMap<String, Value>>,
    ) -> NodeTransform {
        use crate::timeline::property_engine::{
            effective_f32_resolved, effective_half_size_resolved, effective_transform,
        };

        // ── Position: special handling for anchor/binding ──
        let override_position: Option<[f32; 2]> = node_overrides
            .and_then(|ov| ov.get("at").or_else(|| ov.get("position")))
            .and_then(|v| match v {
                Value::Vec2(pos) => Some([pos[0] as f32, pos[1] as f32]),
                _ => None,
            });
        let placement_mode =
            track.geometry.placement_mode.get(time_ms, PlacementMode::LayoutManaged);
        let mut base_position = if let Some(ov_pos) = override_position {
            ov_pos
        } else {
            track.geometry.position.get(time_ms, [0.0, 0.0])
        };
        if let Some(layout_pos) = layout_position {
            if placement_mode == PlacementMode::LayoutManaged && override_position.is_none() {
                base_position = layout_pos;
            }
        }

        // When an override position is set, always use Absolute binding
        // so the modifier value is used directly (skips anchor resolution).
        let binding = if override_position.is_some() {
            PositionBinding::Absolute
        } else {
            track.geometry.position_binding.get(time_ms, PositionBinding::Absolute)
        };
        let position =
            resolve_bound_position(binding, base_position, parent_transform, scene_dimensions);

        // ── Spatial properties: read through registry-based helpers ──
        // Note: shift is handled manually because "shift" is not yet in the
        // property registry (despite being injectable into the environment).
        let motion_offset =
            if let Some(Value::Vec2(v)) = node_overrides.and_then(|ov| ov.get("shift")) {
                [v[0] as f32, v[1] as f32]
            } else {
                track.geometry.motion_offset.get(time_ms, [0.0, 0.0])
            };
        // PF-4: the three scalar reads resolve their registry entries once per
        // process instead of hashing the property name on every node every
        // frame; semantics are unchanged (see `effective_f32_resolved`).
        let reads = crate::timeline::property_registry::transform_property_reads();
        let rotation = effective_f32_resolved(
            track,
            self.world.property_plan(track.id),
            node_overrides,
            time_ms,
            "rotation",
            reads.rotation,
            0.0,
        ) as f64;
        let scale = effective_f32_resolved(
            track,
            self.world.property_plan(track.id),
            node_overrides,
            time_ms,
            "scale",
            reads.scale,
            1.0,
        ) as f64;
        let opacity = effective_f32_resolved(
            track,
            self.world.property_plan(track.id),
            node_overrides,
            time_ms,
            "opacity",
            reads.opacity,
            1.0,
        );
        let half_size = effective_half_size_resolved(
            track,
            self.world.property_plan(track.id),
            node_overrides,
            time_ms,
            reads.size,
            DEFAULT_LAYOUT_HALF_SIZE,
        );

        let transform = effective_transform(
            track,
            self.world.property_plan(track.id),
            node_overrides,
            time_ms,
            "transform",
            reads.transform,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        );
        let transform_affine = kurbo::Affine::new([
            transform[0] as f64,
            transform[1] as f64,
            transform[2] as f64,
            transform[3] as f64,
            transform[4] as f64,
            transform[5] as f64,
        ]);

        let rotate_x = match self.world.read_property_value(
            track.id,
            crate::timeline::ActorField::Tagged("rotate_x"),
            time_ms,
        ) {
            Some(crate::timeline::property_engine::PropertyValue::F32(v)) => v as f64,
            _ => 0.0,
        };
        let rotate_y = match self.world.read_property_value(
            track.id,
            crate::timeline::ActorField::Tagged("rotate_y"),
            time_ms,
        ) {
            Some(crate::timeline::property_engine::PropertyValue::F32(v)) => v as f64,
            _ => 0.0,
        };
        let tilt_affine = if rotate_x != 0.0 || rotate_y != 0.0 {
            kurbo::Affine::new([
                rotate_y.cos(),
                rotate_x.sin() * rotate_y.sin(),
                0.0,
                rotate_x.cos(),
                0.0,
                0.0,
            ])
        } else {
            kurbo::Affine::IDENTITY
        };

        let local_transform = parent_transform
            * kurbo::Affine::translate((
                position[0] as f64 + motion_offset[0] as f64,
                position[1] as f64 + motion_offset[1] as f64,
            ))
            * transform_affine
            * tilt_affine
            * kurbo::Affine::rotate(rotation)
            * kurbo::Affine::scale(scale);

        NodeTransform {
            half_size,
            opacity: opacity * parent_opacity,
            scale,
            local_transform,
        }
    }

    /// Add debug bounds and hit regions for a node.
    pub(crate) fn add_node_debug_overlays(
        &self,
        svg_paths: &[VelloPath],
        half_size: [f32; 2],
        local_transform: &kurbo::Affine,
        scene: &mut vello::Scene,
        vector_paths: &[VelloPath],
        text_paths: &[TextPath],
        has_image: bool,
    ) -> Option<kurbo::Rect> {
        let local_bounds =
            node_local_bounds(vector_paths, text_paths, svg_paths, has_image.then_some(half_size));

        if let Some(bounds) = local_bounds {
            let stroke = vello::kurbo::Stroke::new(1.25);
            let debug_color = vello::peniko::Color::from_rgba8(255, 214, 102, 220);
            scene.stroke(&stroke, *local_transform, debug_color, None, &bounds);
        }

        local_bounds
    }
}

impl Timeline {
    /// Resolve the world-space affine transform of an actor at a given time.
    ///
    /// Delegates to [`Timeline::actor_world_affine`] which walks the scene
    /// graph from root to `label` accumulating transforms.
    pub fn resolve_actor_world_transform(
        &self,
        label: &str,
        time_ms: u64,
        dims: [f64; 2],
    ) -> Option<kurbo::Affine> {
        self.actor_world_affine(
            label,
            time_ms,
            SceneDimensions {
                width: dims[0] as u32,
                height: dims[1] as u32,
            },
        )
    }

    /// Resolve the world-space position `[x, y]` of an actor at a given time.
    ///
    /// Extracts the translation component of the world-space affine transform
    /// returned by `resolve_actor_world_transform`.
    ///
    /// Returns `None` if the actor is not present in this timeline.
    pub fn resolve_actor_world_position(
        &self,
        label: &str,
        time_ms: u64,
        dims: [f64; 2],
    ) -> Option<[f32; 2]> {
        let affine = self.resolve_actor_world_transform(label, time_ms, dims)?;
        let t = affine.translation();
        Some([t.x as f32, t.y as f32])
    }

    /// Extract all text glyph paths from every track in the timeline.
    pub fn extract_all_glyphs(&self) -> Vec<TextPath> {
        let mut glyphs = Vec::new();
        for track in self.world.tracks.values() {
            if let Some(text_paths) = &track.text.text_paths {
                for (paths, _) in text_paths.keyframes.values() {
                    for glyph in paths {
                        glyphs.push(glyph.clone());
                    }
                }
                for glyph in &text_paths.default_value {
                    glyphs.push(glyph.clone());
                }
            }
        }
        glyphs
    }
}
