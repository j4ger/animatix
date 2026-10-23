pub(crate) mod bounds;
pub(crate) mod containers;
#[cfg(test)]
mod tests;

pub(crate) use bounds::{node_local_bounds, transform_rect_bbox};

use super::{
    AnimationTrack, DEFAULT_LAYOUT_HALF_SIZE, DebugRenderOptions, EntityId, EvalError,
    PlacementMode, PositionBinding, SceneDimensions, Timeline, TrackAccessor, Value, VelloPath,
    resolve_bound_position,
};
use crate::renderer::types::TextPath;

#[derive(Clone, Copy)]
pub(crate) struct NodeTransform {
    pub half_size: [f32; 2],
    pub opacity: f32,
    pub scale: f64,
    pub local_transform: kurbo::Affine,
}
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
        let rotation =
            effective_f32_resolved(track, node_overrides, time_ms, "rotation", reads.rotation, 0.0)
                as f64;
        let scale =
            effective_f32_resolved(track, node_overrides, time_ms, "scale", reads.scale, 1.0)
                as f64;
        let opacity =
            effective_f32_resolved(track, node_overrides, time_ms, "opacity", reads.opacity, 1.0);
        let half_size = effective_half_size_resolved(
            track,
            node_overrides,
            time_ms,
            reads.size,
            DEFAULT_LAYOUT_HALF_SIZE,
        );

        let transform = effective_transform(
            track,
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

        let rotate_x = match crate::timeline::read_property_value(
            track,
            crate::timeline::ActorField::Tagged("rotate_x"),
            time_ms,
        ) {
            Some(crate::timeline::property_engine::PropertyValue::F32(v)) => v as f64,
            _ => 0.0,
        };
        let rotate_y = match crate::timeline::read_property_value(
            track,
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
    fn add_node_debug_overlays(
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

    /// Resolve the authored `solo` flags for this frame.
    ///
    /// While any actor declares `solo: true`, only soloed actors draw: their
    /// ancestors stay traversable (so a soloed descendant of a non-soloed
    /// container still renders) while every other subtree is pruned whole.
    /// `visible: false` still wins over solo — an explicitly hidden actor stays
    /// hidden. Nothing soloed is the common case and allocates nothing.
    fn resolve_solo_state(&self, time_ms: u64) -> super::SoloState {
        // Scenes that cannot be soloed (the overwhelming majority) skip the
        // per-track walk entirely; see `EvalCaches::solo_scan_needed`.
        if !self.solo_scan_needed() {
            return super::SoloState::default();
        }
        let mut soloed: Option<std::collections::HashSet<String>> = None;
        for label in self.world.tracks.keys() {
            if !self.track_is_soloed(label, time_ms) {
                continue;
            }
            soloed.get_or_insert_with(std::collections::HashSet::new).insert(label.clone());
        }
        let Some(soloed) = soloed else {
            return super::SoloState::default();
        };
        let mut visible = soloed.clone();
        for label in &soloed {
            if let Some(path) = self.find_path_to_actor(label) {
                // The path ends at the actor itself; ancestors are its prefix.
                visible.extend(path.into_iter().take_while(|step| step != label));
            }
        }
        super::SoloState {
            visible: Some(visible),
            soloed: Some(soloed),
        }
    }

    /// Derive (once per mutation) whether the solo gate needs a per-frame walk.
    ///
    /// Deriving it costs the same walk as the gate itself, so the answer is
    /// cached and reset by `invalidate_frame_cache`. Two things can make the
    /// walk necessary:
    ///
    /// - some actor declares `solo: true` (the flag is non-animatable, so this only changes with
    ///   the timeline); or
    /// - an `always` block writes `solo` on some actor, which happens per frame *without* a
    ///   mutation and would otherwise go unnoticed.
    ///
    /// The second test is exact rather than a blanket "has modifiers" flag:
    /// timelines that merely animate other properties (the common case, and
    /// what the actor-count benchmarks measure) still take the fast path.
    fn solo_scan_needed(&self) -> bool {
        if let Some(needed) = self.eval_caches.solo_scan_needed.get() {
            return needed;
        }
        let always_writes_solo = self.modifiers.iter().any(|stmt| {
            matches!(
                stmt,
                crate::ast::Stmt::Assignment { property, .. } if property == "solo"
            )
        });
        let needed = always_writes_solo
            || self.world.tracks.values().any(|track| self.track_declares_solo(track, 0));
        self.eval_caches.solo_scan_needed.set(Some(needed));
        needed
    }

    /// Read one actor's `solo` flag at `time_ms`; the visibility rule applies.
    fn track_is_soloed(&self, label: &str, time_ms: u64) -> bool {
        let Some(track) = self.world.tracks.get(label) else {
            return false;
        };
        // `visible: false` wins: an explicitly hidden actor is not soloed back
        // into view, and a hidden actor must not keep the rest of the scene
        // suppressed.
        if !track.visible {
            return false;
        }
        self.track_declares_solo(track, time_ms)
    }

    /// The raw flag read, without the visibility rule: the scan asks whether
    /// the flag is declared at all, independent of `visible`.
    fn track_declares_solo(&self, track: &crate::timeline::AnimationTrack, time_ms: u64) -> bool {
        matches!(
            crate::timeline::dispatch::read_property_value(
                track,
                crate::timeline::property_registry::ActorField::Tagged("solo"),
                time_ms,
            ),
            Some(crate::timeline::PropertyValue::Bool(true))
        )
    }

    /// Check whether a filter actor can safely use zero-readback post-render compositing.
    /// This is only safe when the filter is the last child in every ancestor container
    /// (nothing renders after the filter in the scene graph).
    pub(crate) fn can_post_composite_filter(&self, node_id: EntityId) -> bool {
        let Some(label) = self.world.label(node_id) else {
            return false;
        };
        // Find the path from root to this actor
        let Some(path) = self.find_path_to_actor(label) else {
            return false;
        };

        // The actor must be in the root set (no orphan check)
        if path.is_empty() {
            return false;
        }

        // Check that at every level, the actor is the last child. The root
        // node set counts as the outermost container: a root-level filter
        // must be the last root node, or later siblings would render *under*
        // the post-render blit instead of after it.
        for i in 0..path.len() {
            let id = match self.world.get_id(&path[i]) {
                Some(id) => id,
                None => return false,
            };

            if i == 0 {
                if self.world.root_entities().last() != Some(&id) {
                    return false;
                }
                continue;
            }

            // Check it's the last child of its parent
            let Some(parent_id) = self.world.get_id(&path[i - 1]) else {
                return false;
            };
            if self.world.hierarchy(parent_id).children.last() != Some(&id) {
                return false;
            }
        }

        true
    }

    pub(crate) fn evaluate_node_by_id(
        &self,
        node_id: EntityId,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let Some(track) = self.world.tracks.get_by_id(node_id) else {
            return;
        };
        let blend = self.track_blend_mode(track, frame.time_ms);
        if let Some(mix) = blend {
            let unbounded = kurbo::Rect::new(-1.0e9, -1.0e9, 1.0e9, 1.0e9);
            out.scene.push_layer(
                vello::peniko::Fill::NonZero,
                vello::peniko::BlendMode::new(mix, vello::peniko::Compose::SrcOver),
                1.0,
                parent_transform,
                &unbounded,
            );
        }

        let (global_transform, global_opacity) = self.render_actor_node_by_id(
            node_id,
            parent_transform,
            parent_opacity,
            layout_positions,
            allow_pending_composites,
            frame,
            out,
        );

        self.render_node_children_by_id(
            node_id,
            global_transform,
            global_opacity,
            allow_pending_composites,
            frame,
            out,
        );

        if blend.is_some() {
            out.scene.pop_layer();
        }
    }

    pub(crate) fn evaluate_node(
        &self,
        node_label: &str,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        if let Some(id) = self.world.tracks.get_id(node_label) {
            self.evaluate_node_by_id(
                id,
                parent_transform,
                parent_opacity,
                layout_positions,
                allow_pending_composites,
                frame,
                out,
            );
        }
    }

    /// Sample the node's `blend:` track; `None` when it is absent or
    /// "normal" (the source-over default, no layer).
    fn track_blend_mode(&self, track: &AnimationTrack, time_ms: u64) -> Option<vello::peniko::Mix> {
        use crate::timeline::TrackAccessor;

        if !self.blend_used.get() {
            return None;
        }
        let mode = track.style.blend.get(time_ms, "normal".to_string());
        match mode.as_str() {
            "normal" | "" => None,
            "multiply" => Some(vello::peniko::Mix::Multiply),
            "screen" => Some(vello::peniko::Mix::Screen),
            "overlay" => Some(vello::peniko::Mix::Overlay),
            "darken" => Some(vello::peniko::Mix::Darken),
            "lighten" => Some(vello::peniko::Mix::Lighten),
            "color-dodge" => Some(vello::peniko::Mix::ColorDodge),
            "color-burn" => Some(vello::peniko::Mix::ColorBurn),
            "hard-light" => Some(vello::peniko::Mix::HardLight),
            "soft-light" => Some(vello::peniko::Mix::SoftLight),
            "difference" => Some(vello::peniko::Mix::Difference),
            "exclusion" => Some(vello::peniko::Mix::Exclusion),
            "hue" => Some(vello::peniko::Mix::Hue),
            "saturation" => Some(vello::peniko::Mix::Saturation),
            "color" => Some(vello::peniko::Mix::Color),
            "luminosity" => Some(vello::peniko::Mix::Luminosity),
            other => {
                tracing::warn!(
                    "{}: unknown blend mode '{other}' (expected a CSS mix-mode name); treating as normal",
                    track.label
                );
                None
            },
        }
    }

    #[allow(dead_code)] // Fallback when label is used directly
    fn node_blend_mode(&self, node_label: &str, time_ms: u64) -> Option<vello::peniko::Mix> {
        let track = self.world.tracks.get(node_label)?;
        self.track_blend_mode(track, time_ms)
    }

    fn render_actor_node_by_id(
        &self,
        node_id: EntityId,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) -> (kurbo::Affine, f32) {
        let Some(track) = self.world.tracks.get_by_id(node_id) else {
            return (parent_transform, parent_opacity);
        };
        self.render_actor_node_impl(
            track,
            parent_transform,
            parent_opacity,
            layout_positions,
            allow_pending_composites,
            frame,
            out,
        )
    }

    fn render_actor_node_impl(
        &self,
        track: &AnimationTrack,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) -> (kurbo::Affine, f32) {
        let node_label = &track.label;
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let debug_options = frame.debug_options;
        let overrides = frame.overrides;
        let frame_env = frame.frame_env;
        let scene = &mut *out.scene;
        let hit_regions = &mut *out.hit_regions;
        let program_items = &mut *out.program_items;

        // Authored solo: while any actor is soloed, a subtree that contains no
        // soloed actor is pruned whole — unlike `visible`, which hides only the
        // node itself. Ancestors of a soloed actor stay traversable but do not
        // draw their own commands.
        let draws_self = if self.eval_caches.solo_active.get() {
            let solo = self.eval_caches.solo.borrow();
            if !solo.is_reachable(node_label) {
                return (parent_transform, parent_opacity);
            }
            solo.is_soloed(node_label)
        } else {
            true
        };

        // Skip actors that haven't been declared yet.
        // They are not clickable in the preview canvas before they appear;
        // selection is still possible via the layers / inspector tab.
        if time_ms < track.first_seen_ms {
            // Still recurse into children so they are also hidden
            for &child_id in &self.world.hierarchy(track.id).children {
                self.evaluate_node_by_id(
                    child_id,
                    parent_transform,
                    parent_opacity,
                    layout_positions,
                    allow_pending_composites,
                    frame,
                    out,
                );
            }
            return (parent_transform, parent_opacity);
        }

        // ── Evaluate transform first for visibility culling (P2.19) ──
        // Extract node overrides from modifier (always block) so spatial
        // properties (position, rotation, scale, opacity, shift, size, transform)
        // are applied rather than being silently ignored.
        let node_overrides = overrides.get(node_label);

        let layout_pos = if self.dynamic_layout {
            layout_positions
                .get_by_id(track.id)
                .copied()
                .or_else(|| layout_positions.get(node_label).copied())
        } else {
            None
        };

        // P2.18: Temporal coherence — cache node transforms to avoid re-sampling
        // properties when the same (time, parent_transform) is evaluated again.
        let parent_coeffs = parent_transform.as_coeffs();
        let node_transform = {
            // Copy the hit out first so the borrow ends before the recompute
            // below. A miss frame must not allocate a fresh `String` key per
            // node on every insert. Bypassed when node_overrides is present.
            let hit = if node_overrides.is_some() {
                None
            } else {
                let id_idx = track.id.index();
                self.eval_caches
                    .transform_cache
                    .borrow()
                    .get(id_idx)
                    .copied()
                    .flatten()
                    .filter(|(cached_time, cached_parent, _)| {
                        *cached_time == time_ms && *cached_parent == parent_coeffs
                    })
                    .map(|(_, _, cached_transform)| cached_transform)
            };
            if let Some(transform) = hit {
                transform
            } else {
                let t = self.evaluate_node_transform(
                    track,
                    time_ms,
                    parent_opacity,
                    parent_transform,
                    scene_dimensions,
                    layout_pos,
                    node_overrides,
                );
                if node_overrides.is_none() {
                    let mut cache = self.eval_caches.transform_cache.borrow_mut();
                    let id_idx = track.id.index();
                    if id_idx >= cache.len() {
                        cache.resize(id_idx + 1, None);
                    }
                    cache[id_idx] = Some((time_ms, parent_coeffs, t));
                }
                t
            }
        };
        let half_size = node_transform.half_size;
        let opacity = node_transform.opacity;
        let local_transform = node_transform.local_transform;

        // Skip hidden actors (visibility toggle in GUI). Children are still
        // evaluated in render_node_children with the correct parent transform.
        if !track.visible {
            return (local_transform, opacity);
        }

        // P2.19: Viewport culling — skip rendering for off-screen actors.
        // Compute a conservative world-space bounding box and check intersection
        // with the viewport. Children are still evaluated since they may extend
        // back into view even when the parent is off-screen.
        let half_extent =
            (half_size[0].max(half_size[1]) as f64 * node_transform.scale.abs()) + 100.0;
        let coeffs = local_transform.as_coeffs();
        let is_visible = coeffs[4] + half_extent > 0.0
            && coeffs[4] - half_extent < scene_dimensions.width as f64
            && coeffs[5] + half_extent > 0.0
            && coeffs[5] - half_extent < scene_dimensions.height as f64;
        if !is_visible || !draws_self {
            return (local_transform, opacity);
        }

        // PF-6: shared Arc — the common static-track case returns a refcount
        // bump instead of a full path-list clone per node per frame.
        let mut vector_paths = track.evaluate_vector_paths(time_ms);
        // `BarChart` data transitions: once a `data = {…}` assignment has begun,
        // the declaration's paths are stale, so rebuild the bars from the dataset
        // in effect. Gated on the side channel being non-empty, so a chart whose
        // data never animates pays nothing.
        if !track.bar_data_transitions().is_empty() {
            if let Some(layout) = track.bar_layout() {
                let data = crate::timeline::plot::bar_data_at(
                    track.bar_data(),
                    track.bar_data_transitions(),
                    time_ms,
                );
                vector_paths = std::sync::Arc::new(layout.paths_for(&data).0);
            }
        }
        // Re-sample procedural plots at frame time so they can reference `t`.
        // Use the shared frame_env if available; fall back to creating one on-demand
        // (should only happen when frame_env was created at top level).
        if let Some(procedural_plot) = track.procedural_plot() {
            // Guard: only resample per-frame when the plot is dynamic (references `t`
            // or has animated params) or a func transition is currently active.
            // Static, non-transitioning plots keep the cached build-time vector_paths.
            let transitioning = track
                .func_transitions()
                .iter()
                .any(|t| time_ms >= t.start_ms && time_ms <= t.end_ms);
            // Also use per-frame sampling once all transitions are complete so that
            // the final `to` function (not the original declaration) is rendered.
            let has_completed_transitions =
                track.func_transitions().iter().any(|t| t.is_complete_at(time_ms));

            if procedural_plot.is_dynamic(&self.frame_written_vars)
                || transitioning
                || has_completed_transitions
            {
                let fallback_env;
                let env = if let Some(env) = frame_env {
                    env
                } else {
                    fallback_env =
                        self.build_frame_env_internal(time_ms, scene_dimensions, overrides);
                    &fallback_env
                };

                // Inject plot parameter values from keyframe tracks into a
                // scoped let-chain overlay on the frame environment, so that
                // `sample_procedural_plot_at` sees animated values without
                // deep-cloning the entire Environment hash map.
                let mut param_bindings = Vec::new();
                for name in &procedural_plot.param_names {
                    if let Some(param_track) = track.plot_param_track(name) {
                        let val = param_track.evaluate(time_ms);
                        let num_val = crate::timeline::Value::Num(val);
                        // Set the dotted key (e.g. "curve.freq") for explicit references
                        let mut key = String::new();
                        crate::timeline::env_keys::property_into(
                            &procedural_plot.actor_label,
                            name,
                            &mut key,
                        );
                        param_bindings.push((key, num_val.clone()));
                        // Set the bare name (e.g. "freq") for closure captures,
                        // but don't shadow closure sample arguments.
                        if !procedural_plot.func_args.contains(name) {
                            param_bindings.push((name.clone(), num_val));
                        }
                    }
                }
                let has_params = !param_bindings.is_empty();
                if has_params {
                    env.push_let_scope(param_bindings);
                }

                vector_paths =
                    std::sync::Arc::new(crate::timeline::plot::sample_procedural_plot_at(
                        procedural_plot,
                        env,
                        time_ms,
                        track.func_transitions(),
                    ));

                if has_params {
                    env.pop_let_scope();
                }
                // Surface silent sample failures once per actor per frame: a
                // closure whose evaluation fails renders NaN gaps with no
                // other trace (the sampler memoizes per-sample errors, so the
                // path itself is the only signal). PathElement scan is cheap
                // relative to sampling; one diagnostic per frame per actor
                // cannot spam the panel.
                // The path assembler drops NaN points entirely (they only
                // reset the pen), so a failed closure produces an EMPTY
                // BezPath inside a present VelloPath — that emptiness is the
                // observable signal. (Per-point NaN scans never see it.)
                let has_nan_gap = vector_paths
                    .iter()
                    .any(|vp| vp.path.elements().is_empty() && vp.stroke.is_some());
                if has_nan_gap {
                    self.eval_caches.runtime_diagnostics.borrow_mut().push(
                        crate::diagnostics::Diagnostic::warning(
                            crate::diagnostics::DiagnosticCode::RenderFailure,
                            crate::diagnostics::DiagnosticPhase::Render,
                            format!(
                                "plot '{}' produced non-finite samples at t={time_ms}ms; \
                                 the func closure likely failed — rendering gaps",
                                procedural_plot.actor_label
                            ),
                        ),
                    );
                }
            }
        }

        // ── Phase 10b.3: Trait-dispatch scene evaluation ──
        // The primitive's evaluate() is the only render path (no per-type
        // dispatch table). `Some(commands)`
        // draws the commands and records a hit region; `None` means
        // "no drawable content" — nothing is drawn and no hit region or
        // precise bounds are recorded for it.
        let primitive_dispatch = {
            let primitive = self.track_primitive_at(track, time_ms);
            if let Some(primitive) = primitive {
                // Clear the shape-memo bounds handoff: a non-shape
                // primitive must never observe the previous node's data.
                track.begin_shape_commands();
                let ctx = crate::primitives::EvaluateCtx {
                    track,
                    time_ms,
                    local_transform,
                    opacity,
                    scene_dimensions,
                    background_color: self.eval_caches.background_color.get(),
                    overrides: node_overrides,
                    vector_paths: &vector_paths,
                    asset_cache: &self.asset_cache,
                    target_resolver: Some(self),
                };
                let mut text_ctx = crate::primitives::TextCompileCtx {
                    text_compiler: &mut self.text_compiler.borrow_mut(),
                    font_context: self.font_context.as_ref(),
                };
                match primitive.evaluate(&ctx, Some(&mut text_ctx)) {
                    Ok(commands) => commands,
                    Err(e) => {
                        self.eval_caches.runtime_diagnostics.borrow_mut().push(
                            crate::diagnostics::Diagnostic::error(
                                crate::diagnostics::DiagnosticCode::RenderFailure,
                                crate::diagnostics::DiagnosticPhase::Render,
                                format!(
                                    "failed to evaluate '{}' at t={time_ms}ms: {e}",
                                    node_label
                                ),
                            ),
                        );
                        None
                    },
                }
            } else {
                None
            }
        };
        // Record the hit region for this actor (used by GUI picking and
        // precise-bounds consumers). Command-derived bounds when commands
        // exist; otherwise the actor's layout half-size box — this keeps
        // container shells (which return `Some(vec![])`) clickable at the
        // box they occupy. Actors whose `evaluate()` returned `None` have
        // no drawable content (e.g. empty text), so they intentionally
        // record nothing (semantics pinned by
        // `runtime_empty_text_override_clears_stale_glyphs`).
        let mut record_hit_region = |local_bounds: Option<kurbo::Rect>| {
            let world_bounds = if let Some(lb) = local_bounds {
                transform_rect_bbox(&local_transform, lb)
            } else {
                let default_bounds = kurbo::Rect::new(
                    (-half_size[0]) as f64,
                    (-half_size[1]) as f64,
                    half_size[0] as f64,
                    half_size[1] as f64,
                );
                transform_rect_bbox(&local_transform, default_bounds)
            };
            // `hit_regions` is only read back when picking was requested
            // (`evaluate_program_inner` discards it otherwise). The id is the
            // entity handle, so no label is materialized per node per frame.
            if debug_options.compute_hit_regions {
                hit_regions.push((track.id, world_bounds));
            }
            // PF-6 slot-id bounds: the frame start stamped every track's
            // slot, so the write is a `Vec` store — no string key, no hash.
            let slot = track.bounds_slot.get();
            self.record_precise_bounds(slot, node_label, world_bounds);
        };
        if let Some(commands) = primitive_dispatch {
            for cmd in &commands {
                cmd.execute(scene, &local_transform, opacity);
            }
            if let Some(items) = program_items.as_mut() {
                items.push(crate::timeline::scene_program::SceneItem {
                    label: node_label.to_string(),
                    transform: local_transform,
                    opacity,
                    commands: commands.clone(),
                });
            }
            // Hit region — compute from commands, not stale vector_paths.
            // PF-4 scoped item: a shape-command memo hit hands over the
            // build-time-computed bounds (image_size is irrelevant —
            // shape commands never carry images), skipping the per-node
            // bezpath `bounding_box` unions entirely.
            let memo_bounds = track.take_shape_command_bounds();
            let image_size = track
                .image_track()
                .is_some_and(|t| t.evaluate(time_ms).is_some())
                .then_some(half_size);
            let local_bounds: Option<kurbo::Rect> = match memo_bounds {
                Some(bounds) => bounds,
                None => {
                    let mut bounds: Option<kurbo::Rect> = None;
                    for cmd in &commands {
                        if let Some(cmd_bounds) = cmd.local_bounds(image_size) {
                            bounds = Some(match bounds {
                                Some(existing) => existing.union(cmd_bounds),
                                None => cmd_bounds,
                            });
                        }
                    }
                    bounds
                },
            };
            // Shape-primitive commands came from the track's command
            // memo on a hit — hand the buffers back so the next frame
            // can take them again (PF-6 round 8; `take_shape_commands`
            // returns `None` for non-memo actors, so a recycle of a
            // foreign `Vec` is simply dropped by the memo).
            track.recycle_shape_commands(commands);
            record_hit_region(local_bounds);

            // Debug overlays
            if debug_options.draw_bounds {
                let svg_paths = track.svg_paths_at(time_ms).unwrap_or_default();
                let text_paths = track.evaluate_text_paths(time_ms);
                let _ = self.add_node_debug_overlays(
                    &svg_paths,
                    half_size,
                    &local_transform,
                    scene,
                    &vector_paths,
                    &text_paths,
                    image_size.is_some(),
                );
            }

            return (local_transform, opacity);
        }

        (local_transform, opacity)
    }

    /// Resolve the primitive a track renders with at `time_ms`. The track's
    /// `actor_type` is the registry key — except across a cross-type morph,
    /// where the per-frame `shape_type` value, not the (last) identity, picks
    /// the primitive; see `AnimationTrack::render_type_name`.
    pub(crate) fn track_primitive_at<'a>(
        &'a self,
        track: &AnimationTrack,
        time_ms: u64,
    ) -> Option<&'a dyn crate::primitives::Primitive> {
        self.primitive_registry.find(track.render_type_name(time_ms))
    }

    /// Local-space clip geometry for a Mask's `clip_shape` child, obtained from
    /// the child's own primitive via [`Primitive::clip_path`]. Placed at the
    /// child's declared position (the default clip geometry is origin-centered)
    /// to match the previous Rect/Ellipse behavior. `None` means the primitive
    /// has no clip geometry — the caller warns and falls back to a rectangle.
    pub(crate) fn clip_path_for_child(
        &self,
        child: &AnimationTrack,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Option<kurbo::BezPath> {
        let primitive = self.track_primitive_at(child, time_ms)?;
        let vector_paths = child.evaluate_vector_paths(time_ms);
        child.begin_shape_commands();
        let ctx = crate::primitives::EvaluateCtx {
            track: child,
            time_ms,
            local_transform: kurbo::Affine::IDENTITY,
            opacity: 1.0,
            scene_dimensions,
            background_color: self.eval_caches.background_color.get(),
            overrides: overrides.get(&child.label),
            vector_paths: &vector_paths,
            asset_cache: &self.asset_cache,
            target_resolver: Some(self),
        };
        let path = primitive.clip_path(&ctx, &child.caps);
        // Consume the memo-bounds handoff so it can't leak into the next node.
        let _ = child.take_shape_command_bounds();
        // Place the (origin-centered) clip geometry with the child's fully
        // resolved local transform — anchor/offset/rotation/scale included, not
        // just its raw position.
        let node = self.evaluate_node_transform(
            child,
            time_ms,
            1.0,
            kurbo::Affine::IDENTITY,
            scene_dimensions,
            None,
            overrides.get(&child.label),
        );
        path.map(|mut path| {
            path.apply_affine(node.local_transform);
            path
        })
    }

    /// Recursively render child nodes using the primitive capability hook by EntityId.
    pub(crate) fn render_node_children_by_id(
        &self,
        node_id: EntityId,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let Some(track) = self.world.tracks.get_by_id(node_id) else {
            return;
        };
        self.render_node_children_impl(
            track,
            global_transform,
            global_opacity,
            allow_pending_composites,
            frame,
            out,
        );
    }

    fn render_node_children_impl(
        &self,
        track: &AnimationTrack,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let node_label = &track.label;
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let debug_options = frame.debug_options;
        let overrides = frame.overrides;
        let frame_env = frame.frame_env;
        // The primitive is the single child-rendering entry point; the pipeline
        // no longer branches on the child-processing strategy. A primitive
        // without a registered type renders nothing (validated at build time).
        let Some(primitive) = self.track_primitive_at(track, time_ms) else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
        };
        let mut ctx = crate::primitives::RenderChildrenCtx {
            timeline: self,
            host_id: track.id,
            node_label,
            time_ms,
            global_transform,
            global_opacity,
            scene_dimensions,
            debug_options,
            overrides,
            layout_positions: child_layout_positions,
            frame_env,
            allow_pending_composites,
            scene: &mut *out.scene,
            hit_regions: &mut *out.hit_regions,
            program_items: &mut *out.program_items,
            filter_backend: &mut *out.filter_backend,
        };
        let res =
            primitive.render_children_by_id(&mut ctx, &self.world.hierarchy(track.id).children);
        if let Err(e) = res {
            let label = node_label.to_string();
            self.eval_caches.runtime_diagnostics.borrow_mut().push(
                crate::diagnostics::Diagnostic::warning(
                    crate::diagnostics::DiagnosticCode::RenderFailure,
                    crate::diagnostics::DiagnosticPhase::Render,
                    format!("failed to render children of '{label}': {e}"),
                ),
            );
        }
    }

    /// Adapter used by the [`crate::primitives::Primitive::render_children`]
    /// Filter strategy: children render into an offscreen scene, the Filter's
    /// post-processing is applied, and the result is composited back. Reached
    /// only through `FilterPrimitive::render_children`.
    pub fn evaluate(&self, time_s: f64, scene_dimensions: SceneDimensions) -> vello::Scene {
        let mut fb = None;
        self.evaluate_with_debug(time_s, scene_dimensions, DebugRenderOptions::default(), &mut fb)
    }

    /// Evaluate the timeline with optional debug overlays.
    pub fn evaluate_with_debug(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
    ) -> vello::Scene {
        if let Some(program) =
            self.restore_frame_cache(time_s, scene_dimensions, debug_options, filter_backend, false)
        {
            return program.scene;
        }
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            false,
            &[],
            &std::collections::HashMap::new(),
        )
        .scene
    }

    /// Evaluate the timeline with optional debug overlays and transient property overrides.
    pub fn evaluate_with_transient_overrides(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        transient_overrides: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        >,
    ) -> vello::Scene {
        if transient_overrides.is_empty() {
            if let Some(program) = self.restore_frame_cache(
                time_s,
                scene_dimensions,
                debug_options,
                filter_backend,
                false,
            ) {
                return program.scene;
            }
        }
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            false,
            &[],
            transient_overrides,
        )
        .scene
    }

    /// Whether a supplied filter backend could actually be consulted while
    /// building a frame.
    ///
    /// The backend is only reached by effect-scope actors, so its mere presence
    /// is not a reason to skip the frame cache — a runtime that always supplies
    /// one (the web player passes it on every frame) would otherwise never
    /// cache anything. Debug options are a separate matter: they change what is
    /// drawn, so they always block.
    fn backend_can_run(
        &self,
        filter_backend: &Option<&mut dyn crate::timeline::effects::FilterBackend>,
    ) -> bool {
        filter_backend.is_some() && self.has_effect_scopes
    }

    /// Restore cache-derived frame state when a frame cache entry matches.
    fn restore_frame_cache(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        collect_items: bool,
    ) -> Option<crate::timeline::scene_program::SceneProgram> {
        if self.backend_can_run(filter_backend) || debug_options != DebugRenderOptions::default() {
            return None;
        }
        let time_ms = (time_s * 1000.0) as u64;
        let needs_frame_env = self.needs_frame_env();
        let has_child_orders = !self.child_orders.is_empty();
        let cached = self.eval_caches.frame_cache.borrow();
        let cached = cached.as_ref()?;
        if cached.time_ms != time_ms
            || cached.dimensions != scene_dimensions
            || cached.has_modifiers != needs_frame_env
            || cached.has_dynamic_layout != self.dynamic_layout
            || cached.has_child_orders != has_child_orders
            || cached.collect_items != collect_items
        {
            return None;
        }
        // Restore bounds from the frame-cache entry. Scene-only entries stash
        // flat `(slot, rect)` pairs, so the restore is a plain `Vec` copy into
        // the slot table — no string keys, no per-key allocation. Observable
        // (collect_items) entries leave the pairs empty (as before, the bounds
        // live on the cloned program); their rare hits re-derive slots from the
        // label map by binary search over the registry.
        {
            let mut table = self.eval_caches.precise_bounds.borrow_mut();
            table.clear_frame();
            if collect_items {
                drop(table);
                self.ensure_bounds_registry();
                let pairs: Vec<(u32, kurbo::Rect)> = {
                    let registry = self.eval_caches.bounds_registry.borrow();
                    cached
                        .program
                        .precise_bounds
                        .iter()
                        .filter_map(|(label, rect)| {
                            let slot = registry
                                .as_ref()?
                                .labels
                                .binary_search_by(|probe| probe.as_str().cmp(label.as_str()))
                                .ok()? as u32;
                            Some((slot, *rect))
                        })
                        .collect()
                };
                let mut table = self.eval_caches.precise_bounds.borrow_mut();
                for (slot, rect) in pairs {
                    table.write(slot, rect);
                }
            } else {
                for &(slot, rect) in &cached.precise_bounds {
                    table.write(slot, rect);
                }
            }
        }
        *self.eval_caches.runtime_diagnostics.borrow_mut() = cached.program.diagnostics.clone();
        self.eval_caches.hit_regions.borrow_mut().clear();
        // On the scene-only path (no observable item collection) hand back a thin
        // program: only the authoritative scene is deep-copied; the per-frame
        // items/bounds/diagnostics vectors are not re-cloned on every hit. The
        // bounds/diagnostics are still mirrored into the caches above for any
        // tooling, and the collect_items path below keeps the full program.
        if collect_items {
            Some(cached.program.clone())
        } else {
            Some(crate::timeline::scene_program::SceneProgram {
                dimensions: cached.dimensions,
                background: cached.program.background,
                scene: cached.program.scene.clone(),
                items: Vec::new(),
                suppressed_items: Vec::new(),
                precise_bounds: std::collections::HashMap::new(),
                diagnostics: Vec::new(),
            })
        }
    }

    /// Evaluate the timeline into an observable [`crate::timeline::scene_program::SceneProgram`].
    ///
    /// The program carries the authoritative encoded scene plus the structured
    /// frame data used by tooling/tests. Filter and mask paths remain encoded
    /// directly into the authoritative scene; ordinary primitive actors are also
    /// collected as [`SceneItem`](crate::timeline::scene_program::SceneItem)s.
    pub fn evaluate_program_with_debug(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
    ) -> crate::timeline::scene_program::SceneProgram {
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            true,
            &[],
            &std::collections::HashMap::new(),
        )
    }

    /// Evaluates scene program while suppressing specified actor labels (e.g. for FLIP
    /// transitions).
    pub fn evaluate_program_with_suppressed(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        suppressed_actors: &[String],
    ) -> crate::timeline::scene_program::SceneProgram {
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            true,
            suppressed_actors,
            &std::collections::HashMap::new(),
        )
    }

    /// Evaluates scene program with transient property overrides (e.g. interactive inspector sliders).
    pub fn evaluate_program_with_transient_overrides(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        transient_overrides: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        >,
    ) -> crate::timeline::scene_program::SceneProgram {
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            true,
            &[],
            transient_overrides,
        )
    }

    fn evaluate_program_inner(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        collect_items: bool,
        suppressed_actors: &[String],
        transient_overrides: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        >,
    ) -> crate::timeline::scene_program::SceneProgram {
        if suppressed_actors.is_empty() && transient_overrides.is_empty() {
            if let Some(program) = self.restore_frame_cache(
                time_s,
                scene_dimensions,
                debug_options,
                filter_backend,
                collect_items,
            ) {
                return program;
            }
        }

        let time_ms = (time_s * 1000.0) as u64;
        let needs_frame_env = self.needs_frame_env();
        let has_child_orders = !self.child_orders.is_empty();

        // Clear stale runtime diagnostics from previous frame.
        self.clear_runtime_diagnostics();

        // PF-4: Reuse a previously encoded scene buffer instead of allocating.
        // On a plain miss the replaced frame-cache entry is stale by
        // definition (restore_frame_cache just rejected it), so its encoded
        // scene is moved out — no clone — and becomes this frame's encode
        // buffer. Filter/debug frames bypass the frame cache (its entry stays
        // valid), so they recycle via scene_buffer instead.
        let debug_or_filter = self.backend_can_run(filter_backend)
            || debug_options != DebugRenderOptions::default()
            || !transient_overrides.is_empty();
        let mut scene = if debug_or_filter {
            self.eval_caches.scene_buffer.borrow_mut().take().unwrap_or_default()
        } else {
            self.eval_caches
                .frame_cache
                .borrow_mut()
                .take()
                .map(|entry| entry.program.scene)
                .unwrap_or_else(|| {
                    self.eval_caches.scene_buffer.borrow_mut().take().unwrap_or_default()
                })
        };
        scene.reset();
        // Precise bounds are only valid for the frame that computed them.
        // PF-6 slot-id table: sparse clear of the previous frame's slots, then
        // stamp every track's slot for this frame (registry rebuilds lazily
        // after invalidation).
        self.eval_caches.precise_bounds.borrow_mut().clear_frame();
        self.ensure_bounds_registry();
        let bg_color = self.background_color.evaluate_copy(time_ms);
        // Share the frame's background color with the per-node primitive
        // EvaluateCtx (legend label-contrast) without re-sampling the constant
        // background track once per node.
        self.eval_caches.background_color.set(bg_color);
        let solo_state = self.resolve_solo_state(time_ms);
        self.eval_caches.solo_active.set(solo_state.is_active());
        self.eval_caches.solo.replace(solo_state);

        // Collect actor world-space bounding boxes for click-to-select
        let mut hit_regions: Vec<(EntityId, kurbo::Rect)> = if debug_options.compute_hit_regions {
            self.eval_caches.hit_regions.take()
        } else {
            Vec::new()
        };
        hit_regions.clear();

        let mut overrides = self.eval_caches.overrides_buffer.take();
        for inner in overrides.values_mut() {
            inner.clear();
        }
        for label in suppressed_actors {
            overrides
                .entry(label.clone())
                .or_default()
                .insert("opacity".to_string(), Value::Num(0.0));
        }
        for (label, props) in transient_overrides {
            let entry = overrides.entry(label.clone()).or_default();
            for (prop, val) in props {
                entry.insert(prop.clone(), val.clone());
            }
        }

        // P2.16: Skip frame environment creation when no modifiers or procedural plots exist.
        // For static scenes, this eliminates ~95% of evaluation overhead.
        let mut frame_env = if needs_frame_env {
            Some(self.build_frame_env_internal(time_ms, scene_dimensions, &overrides))
        } else {
            None
        };

        // Execute lowered modifier IR only. Lowering is total for all modifier
        // statements; if lowering failed at build time, the build already
        // emitted a ModifierCompilationError diagnostic.
        let mut modifier_errors: Vec<EvalError> = Vec::new();
        if !self.modifier_programs.is_empty() {
            if let Some(ref mut env) = frame_env {
                for program in &self.modifier_programs {
                    if let Err(e) = self.apply_modifier_program(
                        program,
                        time_ms,
                        scene_dimensions,
                        env,
                        &mut overrides,
                    ) {
                        modifier_errors.push(e);
                    }
                }
            }
        } else if !self.modifiers.is_empty() {
            tracing::warn!(
                "No lowered modifier IR is available for {} modifier statement(s); skipping modifier execution",
                self.modifiers.len()
            );
        }

        // Re-apply transient overrides so user interaction wins over modifier execution.
        if !transient_overrides.is_empty() {
            for (label, props) in transient_overrides {
                let entry = overrides.entry(label.clone()).or_default();
                for (prop, val) in props {
                    entry.insert(prop.clone(), val.clone());
                }
            }
        }

        // Collect modifier evaluation errors as runtime diagnostics.
        for err in &modifier_errors {
            tracing::warn!("Modifier evaluation error at t={time_ms}ms: {err}");
            self.eval_caches.runtime_diagnostics.borrow_mut().push(
                crate::diagnostics::Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::ModifierRuntimeError,
                    crate::diagnostics::DiagnosticPhase::Render,
                    format!("Modifier evaluation error at t={time_ms}ms: {err}"),
                ),
            );
        }

        let bg = vello::peniko::Color::new([bg_color[0], bg_color[1], bg_color[2], bg_color[3]]);
        scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            bg,
            None,
            &kurbo::Rect::new(
                0.0,
                0.0,
                scene_dimensions.width as f64,
                scene_dimensions.height as f64,
            ),
        );

        let mut program_items = if collect_items {
            Some(Vec::new())
        } else {
            None
        };
        // The frame-invariant bundle and the mutable outputs are built once and
        // threaded through the whole recursion (see `RenderFrame`).
        let frame = crate::primitives::RenderFrame {
            time_ms,
            scene_dimensions,
            debug_options,
            overrides: &overrides,
            frame_env: frame_env.as_ref(),
        };
        // `sample` covers per-frame property sampling and node evaluation; the
        // vello encoding is currently interleaved inside it (see `stage::ENCODE_SCENE`).
        let _sample_stage = crate::perf::ScopedStage::new(crate::perf::stage::SAMPLE);
        // Scoped so the output borrows end with the loop: the code below reuses
        // `scene` / `hit_regions` / `program_items` directly.
        {
            let mut out = crate::primitives::RenderOutputs {
                scene: &mut scene,
                hit_regions: &mut hit_regions,
                program_items: &mut program_items,
                filter_backend: &mut *filter_backend,
            };
            // The scene camera wraps every root subtree (see `timeline::camera`).
            // It is read *after* the modifier pass above, so an
            // `always { camera.at = … }` write is already in the override map.
            // The background fill deliberately stays un-camerad.
            let camera_affine = if self.camera_used.get() {
                self.camera.affine(
                    time_ms,
                    scene_dimensions,
                    overrides.get(crate::timeline::camera::CAMERA_TARGET),
                )
            } else {
                kurbo::Affine::IDENTITY
            };
            let camera_exempt = self.camera_used.get() && self.camera_exempt_present.get();
            let allow_static = !self.backend_can_run(out.filter_backend)
                && !self.camera_used.get()
                && suppressed_actors.is_empty()
                && transient_overrides.is_empty();
            for &root_id in self.world.root_entities() {
                let Some(root_track) = self.world.tracks.get_by_id(root_id) else {
                    continue;
                };
                // `camera_follow: false` keeps a root subtree out of the camera
                // entirely — the HUD that must not move while the scene pushes
                // in. Only roots are consulted: the camera is applied here, once,
                // and a nested actor inherits its container's transform, so an
                // opt-out deeper in the tree would have nothing to undo (and is
                // reported at build). Short-circuited on `camera_exempt` so a
                // scene that never uses the flag does no track lookup per root.
                let root_affine = if camera_exempt {
                    if !root_track.camera_follow || root_track.parallax == 0.0 {
                        kurbo::Affine::IDENTITY
                    } else if (root_track.parallax - 1.0).abs() < f32::EPSILON {
                        camera_affine
                    } else {
                        let p = root_track.parallax as f64;
                        let cam = camera_affine.as_coeffs();
                        kurbo::Affine::new([
                            1.0 + p * (cam[0] - 1.0),
                            p * cam[1],
                            p * cam[2],
                            1.0 + p * (cam[3] - 1.0),
                            p * cam[4],
                            p * cam[5],
                        ])
                    }
                } else {
                    camera_affine
                };
                // P2.17: Static subtree cache — fully-static subtrees are evaluated once
                // and their vello encoding is reused on subsequent frames. Dimensions
                // and item collection are part of the key so different canvas sizes or
                // observable-program requests cannot reuse an incompatible entry.
                // PF-11: hit regions no longer opt out — the entry captures the
                // subtree's (label, rect) pairs once and restores them on hits, so
                // the GUI (which always requests `compute_hit_regions` for picking)
                // keeps its static-subtree reuse.
                // A camera move invalidates it: the cached encoding is appended
                // verbatim and cannot be re-transformed, so the frame must
                // re-evaluate the subtree instead.
                let is_static = allow_static && self.is_static_subtree_id(root_id);
                if is_static {
                    let cache_key = (root_id, scene_dimensions, collect_items, debug_options);
                    let cache = self.eval_caches.static_subtree_cache.borrow_mut();
                    if let Some((cached_scene, cached_bounds, cached_items, cached_hit_regions)) =
                        cache.get(&cache_key)
                    {
                        // Fast path: append cached encoding directly and restore the
                        // precise bounds that were computed for this subtree.
                        out.scene.encoding_mut().append(cached_scene.encoding(), &None);
                        if !cached_bounds.is_empty() {
                            let mut table = self.eval_caches.precise_bounds.borrow_mut();
                            for &(slot, rect) in cached_bounds {
                                table.write(slot, rect);
                            }
                        }
                        if debug_options.compute_hit_regions && !cached_hit_regions.is_empty() {
                            out.hit_regions.extend(cached_hit_regions.iter().cloned());
                        }
                        if let Some(items) = out.program_items.as_mut() {
                            if !cached_items.is_empty() {
                                items.reserve(cached_items.len());
                                items.extend(cached_items.iter().cloned());
                            }
                        }
                    } else {
                        drop(cache);
                        let mut temp_scene = vello::Scene::new();
                        // Snapshot offset into the frame's `written` list: entries
                        // appended during this subtree's evaluation are exactly its
                        // bounds (deterministic, unlike the old count-based skip
                        // over unordered HashMap iteration).
                        let subtree_written_before =
                            self.eval_caches.precise_bounds.borrow().written.len();
                        let mut subtree_items_slot = if collect_items {
                            Some(Vec::new())
                        } else {
                            None
                        };
                        let subtree_hits_before = out.hit_regions.len();
                        // The scratch subtree swaps both the draw target and the
                        // item sink, so it builds its own outputs rather than
                        // reusing the frame's.
                        // The scratch subtree swaps both the draw target and
                        // the item sink, so it runs through its own outputs.
                        out.with_scene_and_items(
                            &mut temp_scene,
                            &mut subtree_items_slot,
                            |temp_out| {
                                self.evaluate_node_by_id(
                                    root_id,
                                    root_affine,
                                    1.0,
                                    &crate::timeline::layout::LayoutPositions::new(),
                                    true,
                                    &frame,
                                    temp_out,
                                );
                            },
                        );
                        let subtree_items = subtree_items_slot.take().unwrap_or_default();
                        if let Some(items) = out.program_items.as_mut() {
                            items.extend(subtree_items.iter().cloned());
                        }
                        // Capture the subtree's hit regions once (only collected
                        // when requested; see the cache-key's debug_options).
                        let new_hit_regions: Vec<(EntityId, kurbo::Rect)> =
                            if debug_options.compute_hit_regions {
                                out.hit_regions[subtree_hits_before..].to_vec()
                            } else {
                                Vec::new()
                            };
                        // Append to main out.scene and cache for next time.
                        out.scene.encoding_mut().append(temp_scene.encoding(), &None);
                        let new_bounds: Vec<(u32, kurbo::Rect)> = self
                            .eval_caches
                            .precise_bounds
                            .borrow()
                            .pairs_from(subtree_written_before);
                        self.eval_caches.static_subtree_cache.borrow_mut().insert(
                            cache_key,
                            (temp_scene, new_bounds, subtree_items, new_hit_regions),
                        );
                    }
                } else {
                    // A filter scope may only take the zero-readback pending path
                    // when nothing renders after it (`can_post_composite_filter`);
                    // otherwise its post-render blit would cover later siblings.
                    // Mid-out.scene filters fall back to the inline readback path.
                    let allow_pending = self.can_post_composite_filter(root_id);
                    self.evaluate_node_by_id(
                        root_id,
                        root_affine,
                        1.0,
                        &crate::timeline::layout::LayoutPositions::new(), // empty for roots
                        allow_pending,
                        &frame,
                        &mut out,
                    );
                }
            }
        }

        // P2.24: Only store hit regions when explicitly requested.
        // Saves bounding-box computation for frames where click-to-select is not needed.
        if debug_options.compute_hit_regions {
            *self.eval_caches.hit_regions.borrow_mut() = hit_regions;
        } else {
            self.eval_caches.hit_regions.borrow_mut().clear();
        }

        // The program owns the encoded scene; the frame cache keeps a clone
        // for restore, and filter/debug frames park their copy in scene_buffer.
        // The slot table builds `program.precise_bounds` (label-keyed, for the
        // public `SceneProgram` surface) only on the observable path — it has
        // no production reader outside tooling/tests (checked 2026-09-05), so
        // the scene-only path skips the label materialization entirely. On the
        // scene-only path the frame cache gets the flat `(slot, rect)` pairs;
        // on the observable path the pairs stay empty (the cloned program
        // carries the label map, and its rare hits re-derive slots from it —
        // avoids allocating both representations per observable miss).
        let (program_bounds, cached_bounds) = if collect_items {
            // One pass over the written slots builds the label-keyed map; the
            // pre-sized map avoids rehash growth. Registry borrowed in place.
            let table = self.eval_caches.precise_bounds.borrow();
            let registry = self.eval_caches.bounds_registry.borrow();
            let labels = registry.as_ref().map(|r| r.labels.as_slice());
            let mut b = std::collections::HashMap::with_capacity(table.written.len());
            for &slot in &table.written {
                if let Some(rect) = table.slots[slot as usize] {
                    if let Some(label) = labels.and_then(|l| l.get(slot as usize)) {
                        b.insert(label.clone(), rect);
                    }
                }
            }
            (b, Vec::new())
        } else {
            let b = self.eval_caches.precise_bounds.borrow().pairs_from(0);
            (std::collections::HashMap::new(), b)
        };
        let program = crate::timeline::scene_program::SceneProgram {
            dimensions: scene_dimensions,
            background: bg_color,
            scene,
            items: program_items.take().unwrap_or_default(),
            suppressed_items: Vec::new(),
            precise_bounds: program_bounds,
            diagnostics: self.eval_caches.runtime_diagnostics.borrow().clone(),
        };
        if !self.backend_can_run(filter_backend)
            && debug_options == DebugRenderOptions::default()
            && transient_overrides.is_empty()
        {
            *self.eval_caches.frame_cache.borrow_mut() = Some(super::FrameCacheEntry {
                time_ms,
                dimensions: scene_dimensions,
                has_modifiers: needs_frame_env,
                has_dynamic_layout: self.dynamic_layout,
                has_child_orders,
                program: program.clone(),
                precise_bounds: cached_bounds,
                collect_items,
            });
        } else {
            // Filter/debug frames bypass the frame cache (its entry stays
            // valid for later default-option frames); hand the encoded scene
            // back to the buffer so the next such frame can reuse it.
            *self.eval_caches.scene_buffer.borrow_mut() = Some(program.scene.clone());
        }

        // PF-6: hand the finished frame environment back to the pool — its
        // overrides map keeps the table capacity and every key `String` alive
        // for the next frame's in-place rebuild. Only the plain evaluate path
        // pools; callers that received a cloned env (`local_env` in the plot
        // path) or built one on demand return nothing here.
        if let Some(env) = frame_env {
            *self.env_pool.borrow_mut() = Some(env);
        }
        *self.eval_caches.overrides_buffer.borrow_mut() = overrides;

        program
    }
}
