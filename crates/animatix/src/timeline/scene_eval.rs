use kurbo::Shape;

use super::{
    AnimationTrack, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, DebugRenderOptions, EvalError,
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

fn union_rect(acc: Option<kurbo::Rect>, rect: kurbo::Rect) -> Option<kurbo::Rect> {
    Some(match acc {
        Some(existing) => existing.union(rect),
        None => rect,
    })
}

fn node_local_bounds(
    vector_paths: &[VelloPath],
    text_paths: &[TextPath],
    svg_paths: &[VelloPath],
    image_half_size: Option<[f32; 2]>,
) -> Option<kurbo::Rect> {
    let mut bounds = None;

    for vector_path in vector_paths {
        bounds = union_rect(bounds, vector_path.path.bounding_box());
    }
    for text_path in text_paths {
        bounds = union_rect(bounds, text_path.path.bounding_box());
    }
    for svg_path in svg_paths {
        bounds = union_rect(bounds, svg_path.path.bounding_box());
    }

    if let Some([half_width, half_height]) = image_half_size {
        // The image command centers its box on the local origin (see
        // `ImagePrimitive::evaluate`), so the bounds are the centered box,
        // not a box from the origin.
        bounds = union_rect(
            bounds,
            kurbo::Rect::new(
                (-half_width) as f64,
                (-half_height) as f64,
                half_width as f64,
                half_height as f64,
            ),
        );
    }

    bounds
}

fn transform_rect_bbox(transform: &kurbo::Affine, rect: kurbo::Rect) -> kurbo::Rect {
    let p0 = *transform * kurbo::Point::new(rect.x0, rect.y0);
    let p1 = *transform * kurbo::Point::new(rect.x0, rect.y1);
    let p2 = *transform * kurbo::Point::new(rect.x1, rect.y0);
    let p3 = *transform * kurbo::Point::new(rect.x1, rect.y1);
    let x0 = p0.x.min(p1.x).min(p2.x).min(p3.x);
    let y0 = p0.y.min(p1.y).min(p2.y).min(p3.y);
    let x1 = p0.x.max(p1.x).max(p2.x).max(p3.x);
    let y1 = p0.y.max(p1.y).max(p2.y).max(p3.y);
    kurbo::Rect::new(x0, y0, x1, y1)
}

/// Escape a Fragment's content for embedding in one #box() markup block.
///
/// Typst parses a leading '+' as an enum item and a leading '-' as a bullet,
/// so a term fragment like "+ sin(3x)/3" otherwise renders as the list marker
/// "1." — escape the marker so equations read as written.
pub(crate) fn equation_markup_escaped(content: &str) -> String {
    let trimmed = content.trim_start();
    if trimmed.starts_with('+') || trimmed.starts_with('-') {
        // Keep any leading whitespace, escape the marker itself.
        let cut = content.len() - trimmed.len();
        format!("{}\\{}", &content[..cut], trimmed)
    } else {
        content.to_string()
    }
}

impl Timeline {
    /// Evaluate position, size, and transform for a node.
    ///
    /// `node_overrides` — when set (from an `always` modifier), overrides for
    /// spatial properties (`at`/`position`, `shift`, `rotation`, `scale`,
    /// `opacity`, `size`, `transform`) are applied via the property registry
    /// helpers in place of the track's keyframed values.
    fn evaluate_node_transform(
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
            effective_f32_resolved, effective_transform, effective_vec2_resolved,
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
        let half_size = effective_vec2_resolved(
            track,
            node_overrides,
            time_ms,
            "size",
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

        let local_transform = parent_transform
            * kurbo::Affine::translate((
                position[0] as f64 + motion_offset[0] as f64,
                position[1] as f64 + motion_offset[1] as f64,
            ))
            * transform_affine
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
        for track in self.tracks.values() {
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
        for label in self.tracks.keys() {
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
    /// - some actor declares `solo: true` (the flag is non-animatable, so this
    ///   only changes with the timeline); or
    /// - an `always` block writes `solo` on some actor, which happens per frame
    ///   *without* a mutation and would otherwise go unnoticed.
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
            || self.tracks.values().any(|track| self.track_declares_solo(track, 0));
        self.eval_caches.solo_scan_needed.set(Some(needed));
        needed
    }

    /// Read one actor's `solo` flag at `time_ms`; the visibility rule applies.
    fn track_is_soloed(&self, label: &str, time_ms: u64) -> bool {
        let Some(track) = self.tracks.get(label) else {
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
    pub(crate) fn can_post_composite_filter(&self, node_label: &str) -> bool {
        // Find the path from root to this actor
        let Some(path) = self.find_path_to_actor(node_label) else {
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
            let label = &path[i];
            if !self.tracks.contains_key(label) {
                return false;
            }

            if i == 0 {
                if self.root_nodes.last() != Some(label) {
                    return false;
                }
                continue;
            }

            // Check it's the last child of its parent
            let parent_label = &path[i - 1];
            let Some(parent_track) = self.tracks.get(parent_label) else {
                return false;
            };
            if parent_track.children.last() != Some(label) {
                return false;
            }
        }

        true
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
        // A non-"normal" `blend:` composites the node's whole subtree (own
        // commands and children alike) into a full-surface layer with the
        // requested mix, so `screen` on a Group blends the *composited* group
        // against what is behind it — the semantics that make light effects
        // (glows, light sweeps, color washes) one-liners.
        let blend = self.node_blend_mode(node_label, frame.time_ms);
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

        let (global_transform, global_opacity) = self.render_actor_node(
            node_label,
            parent_transform,
            parent_opacity,
            layout_positions,
            allow_pending_composites,
            frame,
            out,
        );

        self.render_node_children(
            node_label,
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

    /// Sample the node's `blend:` track; `None` when it is absent or
    /// "normal" (the source-over default, no layer).
    fn node_blend_mode(&self, node_label: &str, time_ms: u64) -> Option<vello::peniko::Mix> {
        use crate::timeline::TrackAccessor;

        if !self.blend_used.get() {
            return None;
        }
        let track = self.tracks.get(node_label)?;
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
                    "{node_label}: unknown blend mode '{other}' (expected a CSS mix-mode name); treating as normal"
                );
                None
            },
        }
    }

    /// Evaluate a single actor node and render it to the scene.
    /// Returns the (transform, opacity) to use for child rendering.
    fn render_actor_node(
        &self,
        node_label: &str,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) -> (kurbo::Affine, f32) {
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let debug_options = frame.debug_options;
        let overrides = frame.overrides;
        let frame_env = frame.frame_env;
        let scene = &mut *out.scene;
        let hit_regions = &mut *out.hit_regions;
        let program_items = &mut *out.program_items;
        let Some(track) = self.tracks.get(node_label) else {
            return (parent_transform, parent_opacity);
        };

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
            let children: Vec<&str> = track.children.iter().map(|s| s.as_str()).collect();
            for child_label in children {
                self.evaluate_node(
                    child_label,
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
            layout_positions.get(node_label).copied()
        } else {
            None
        };

        // P2.18: Temporal coherence — cache node transforms to avoid re-sampling
        // properties when the same (time, parent_transform) is evaluated again.
        let parent_coeffs = parent_transform.as_coeffs();
        let node_transform = {
            // Copy the hit out first so the borrow ends before the recompute
            // below. A miss frame must not allocate a fresh `String` key per
            // node on every insert.
            let hit = self
                .eval_caches
                .transform_cache
                .borrow()
                .get(node_label)
                .filter(|(cached_time, cached_parent, _)| {
                    *cached_time == time_ms && *cached_parent == parent_coeffs
                })
                .map(|(_, _, cached_transform)| *cached_transform);
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
                let mut cache = self.eval_caches.transform_cache.borrow_mut();
                // Labels are stable across frames, so refresh the existing slot
                // in place rather than re-inserting an owned key.
                if let Some(slot) = cache.get_mut(node_label) {
                    *slot = (time_ms, parent_coeffs, t);
                } else {
                    cache.insert(node_label.to_string(), (time_ms, parent_coeffs, t));
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
        let viewport = kurbo::Rect::new(
            0.0,
            0.0,
            scene_dimensions.width as f64,
            scene_dimensions.height as f64,
        );
        let max_extent = half_size[0].max(half_size[1]) as f64 * node_transform.scale.abs();
        let margin = 100.0; // margin for effects and small children
        let world_pos = local_transform * kurbo::Point::new(0.0, 0.0);
        let actor_bounds = kurbo::Rect::new(
            world_pos.x - max_extent - margin,
            world_pos.y - max_extent - margin,
            world_pos.x + max_extent + margin,
            world_pos.y + max_extent + margin,
        );
        let is_visible = viewport.intersect(actor_bounds).area() > 0.0;

        // PF-6: shared Arc — the common static-track case returns a refcount
        // bump instead of a full path-list clone per node per frame.
        let mut vector_paths = track.evaluate_vector_paths(time_ms);
        // Re-sample procedural plots at frame time so they can reference `t`.
        // Use the shared frame_env if available; fall back to creating one on-demand
        // (should only happen when frame_env was created at top level).
        if let Some(procedural_plot) = track.procedural_plot.as_ref() {
            // Guard: only resample per-frame when the plot is dynamic (references `t`
            // or has animated params) or a func transition is currently active.
            // Static, non-transitioning plots keep the cached build-time vector_paths.
            let transitioning = track
                .func_transitions
                .iter()
                .any(|t| time_ms >= t.start_ms && time_ms <= t.end_ms);
            // Also use per-frame sampling once all transitions are complete so that
            // the final `to` function (not the original declaration) is rendered.
            let has_completed_transitions =
                track.func_transitions.iter().any(|t| t.is_complete_at(time_ms));

            if procedural_plot.is_dynamic(&self.frame_written_vars)
                || transitioning
                || has_completed_transitions
            {
                let mut local_env = if let Some(env) = frame_env {
                    env.clone()
                } else {
                    self.build_frame_env_internal(time_ms, scene_dimensions, overrides)
                };

                // Inject plot parameter values from keyframe tracks into the
                // evaluation environment so that `sample_procedural_plot_at` sees
                // the animated value rather than the build-time static default.
                for name in &procedural_plot.param_names {
                    if let Some(param_track) = track.plot_param_tracks.get(name) {
                        let val = param_track.evaluate(time_ms);
                        let num_val = crate::timeline::Value::Num(val);
                        // Set the dotted key (e.g. "curve.freq") for explicit references
                        let mut key = String::new();
                        crate::timeline::env_keys::property_into(
                            &procedural_plot.actor_label,
                            name,
                            &mut key,
                        );
                        local_env.set(&key, num_val.clone());
                        // Set the bare name (e.g. "freq") for closure captures,
                        // but don't shadow closure sample arguments.
                        if !procedural_plot.func_args.contains(name) {
                            local_env.set(name, num_val);
                        }
                    }
                }

                vector_paths =
                    std::sync::Arc::new(crate::timeline::plot::sample_procedural_plot_at(
                        procedural_plot,
                        &mut local_env,
                        time_ms,
                        &track.func_transitions,
                    ));
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

        // P2.19: Only sample properties and render if actor is visible on screen.
        // For off-screen actors we still return transform/opacity so children
        // (which may extend back into view) are correctly evaluated.
        if is_visible && draws_self {
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
                // (`evaluate_program_inner` discards it otherwise), so building
                // the owned label per node per frame is wasted allocation.
                if debug_options.compute_hit_regions {
                    hit_regions.push((node_label.to_string(), world_bounds));
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
                let image_size = track.image.get(time_ms, None).is_some().then_some(half_size);
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

    /// Recursively render child nodes using the primitive capability hook.
    pub(crate) fn render_node_children(
        &self,
        node_label: &str,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let debug_options = frame.debug_options;
        let overrides = frame.overrides;
        let frame_env = frame.frame_env;
        let Some(track) = self.tracks.get(node_label) else {
            return;
        };
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
        let children: Vec<&str> = track.children.iter().map(|s| s.as_str()).collect();
        let mut ctx = crate::primitives::RenderChildrenCtx {
            timeline: self,
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
        if let Err(e) = primitive.render_children(&mut ctx, &children) {
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
    fn render_filter_children(
        &self,
        node_label: &str,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let Some(track) = self.tracks.get(node_label) else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
        };
        let children: Vec<&str> = track.children.iter().map(|s| s.as_str()).collect();
        if children.is_empty() {
            return;
        }

        // Check if a filter backend is available
        let has_backend = out.filter_backend.is_some();
        if !has_backend {
            // Surface the fallback: authored filter effects are silently
            // dropped without it.
            self.eval_caches.runtime_diagnostics.borrow_mut().push(
                crate::diagnostics::Diagnostic::warning(
                    crate::diagnostics::DiagnosticCode::RenderFailure,
                    crate::diagnostics::DiagnosticPhase::Render,
                    format!(
                        "Filter '{node_label}' has no filter backend available; \
                             rendering children unfiltered"
                    ),
                ),
            );
            // Fallback: render children directly (no filtering)
            for child in &children {
                self.evaluate_node(
                    child,
                    global_transform,
                    global_opacity,
                    &child_layout_positions,
                    allow_pending_composites,
                    frame,
                    out,
                );
            }
            return;
        }

        // Build sub-scene with children rendered at their world positions
        let mut sub_scene = vello::Scene::new();
        out.with_scene(&mut sub_scene, |out| {
            for child in &children {
                self.evaluate_node(
                    child,
                    global_transform,
                    global_opacity,
                    &child_layout_positions,
                    false,
                    frame,
                    out,
                );
            }
        });

        // Effects are lowered onto the scope's chain at build time; identity
        // and disabled stages are dropped during sampling.
        let chain = track.effects.build_chain(time_ms);

        if chain.is_empty() {
            out.scene.encoding_mut().append(sub_scene.encoding(), &None);
            return;
        }

        // Region of interest, in priority order:
        // 1. an authored `bounds: (x, y, w, h)`;
        // 2. derived from the content bounds the sub-scene evaluation just
        //    recorded (`docs/effects.md` §3), expanded by worst-case support;
        // 3. `None` — the historical full-scene path.
        // The GPU textures stay at full scene capacity in every case, so
        // varying regions never reallocate (PF-7).
        let region = match self.effect_scope_region(track, scene_dimensions, time_ms) {
            Some(region) => Some(region),
            None => {
                let mut content: Option<kurbo::Rect> = None;
                for child in &track.children {
                    self.subtree_bounds_union(child, &mut content);
                }
                content.and_then(|rect| {
                    Self::region_from_rect(
                        rect,
                        track.effects.worst_case_support(),
                        scene_dimensions,
                    )
                })
            },
        };

        // Always take the zero-readback path when a backend exists: the
        // pending composites are blitted after the main scene render in
        // declaration order, and the CPU readback path is the one that loses
        // full-canvas scopes in exported video (see docs/roadmap.md Known
        // Issues).
        if allow_pending_composites {
            if let Some(backend) = out.filter_backend.as_mut() {
                match backend.render_scene_to_pending_composite(
                    &sub_scene,
                    scene_dimensions,
                    region,
                    &chain,
                    global_opacity,
                ) {
                    Ok(()) => {
                        // Filter output is stored as a pending GPU composite.
                        // The renderer will blit it after the main scene render.
                        return;
                    },
                    Err(e) => {
                        tracing::warn!(
                            "Zero-readback filter path failed, falling back to readback: {e}"
                        );
                    },
                }
            }
        }

        // Render sub-scene to image via backend, apply GPU filters, draw result
        if let Some(backend) = out.filter_backend.as_mut() {
            match backend.render_scene_to_image_gpu_filtered(
                &sub_scene,
                scene_dimensions,
                region,
                &chain,
            ) {
                Ok(filtered) => {
                    // A region-scoped result is composited back at its origin;
                    // a full-scene result covers the target exactly.
                    let transform = match region {
                        Some(region) => kurbo::Affine::translate((
                            region.origin[0] as f64,
                            region.origin[1] as f64,
                        )),
                        None => kurbo::Affine::IDENTITY,
                    };
                    let brush = vello::peniko::ImageBrush::new(filtered.data.clone())
                        .with_extend(vello::peniko::Extend::Pad)
                        .with_quality(vello::peniko::ImageQuality::Medium)
                        .with_alpha(global_opacity);
                    out.scene.draw_image(&brush, transform);
                },
                Err(e) => {
                    tracing::warn!(
                        "Filter backend error, falling back to unfiltered rendering: {e}"
                    );
                    self.eval_caches.runtime_diagnostics.borrow_mut().push(
                        crate::diagnostics::Diagnostic::warning(
                            crate::diagnostics::DiagnosticCode::RenderFailure,
                            crate::diagnostics::DiagnosticPhase::Render,
                            format!(
                                "Filter '{node_label}' backend failed ({e}); \
                                     rendering children unfiltered"
                            ),
                        ),
                    );
                    out.scene.encoding_mut().append(sub_scene.encoding(), &None);
                },
            }
        }
    }

    /// Compute the region of interest for an effect scope from an authored
    /// `bounds: (x, y, w, h)` (tagged `filter_bounds` storage). Returns `None`
    /// when no bounds are authored, the region is degenerate, or it already
    /// covers the whole scene (the historical full-scene path).
    pub(crate) fn effect_scope_region(
        &self,
        track: &AnimationTrack,
        scene_dimensions: SceneDimensions,
        time_ms: u64,
    ) -> Option<crate::timeline::effects::EffectRegion> {
        let value = crate::timeline::dispatch::read_property_value(
            track,
            crate::timeline::property_registry::ActorField::Tagged("filter_bounds"),
            time_ms,
        );
        let Some(crate::timeline::PropertyValue::Vec4([x, y, w, h])) = value else {
            return None;
        };
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let rect = kurbo::Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64);
        Self::region_from_rect(rect, track.effects.worst_case_support(), scene_dimensions)
    }

    /// Union the recorded world bounds of `label`'s content subtree into `out`.
    ///
    /// Bounds are recorded by the sub-scene evaluation that just ran, so they
    /// reflect exactly what this frame drew. Nodes without recorded ink (pure
    /// containers, disabled effects) contribute nothing themselves; their
    /// subtrees are still walked.
    fn subtree_bounds_union(&self, label: &str, out: &mut Option<kurbo::Rect>) {
        if let Some(rect) = self.precise_bounds_for(label) {
            *out = Some(match out.take() {
                Some(accumulated) => accumulated.union(rect),
                None => rect,
            });
        }
        if let Some(track) = self.tracks.get(label) {
            for child in &track.children {
                self.subtree_bounds_union(child, out);
            }
        }
    }

    /// Turn a content-bounds rectangle into an [`EffectRegion`], padding by
    /// `support` and clamping to the scene. Returns `None` when the padded
    /// region covers the whole scene (fall back to the full-scene path) or is
    /// degenerate.
    fn region_from_rect(
        rect: kurbo::Rect,
        support: f32,
        scene_dimensions: SceneDimensions,
    ) -> Option<crate::timeline::effects::EffectRegion> {
        let x0 = (rect.x0 as f32 - support).max(0.0);
        let y0 = (rect.y0 as f32 - support).max(0.0);
        let x1 = (rect.x1 as f32 + support).min(scene_dimensions.width as f32);
        let y1 = (rect.y1 as f32 + support).min(scene_dimensions.height as f32);
        let width = (x1 - x0).floor().max(1.0) as u32;
        let height = (y1 - y0).floor().max(1.0) as u32;
        if width >= scene_dimensions.width && height >= scene_dimensions.height {
            return None;
        }
        Some(crate::timeline::effects::EffectRegion {
            origin: [x0, y0],
            size: SceneDimensions { width, height },
        })
    }

    /// Mask strategy: children render inside the Mask's clip geometry. Reached
    /// only through `MaskPrimitive::render_children`.
    fn render_mask_children(
        &self,
        node_label: &str,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let overrides = frame.overrides;
        let Some(track) = self.tracks.get(node_label) else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
        };
        let half_size = track.geometry.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);

        // Resolve the clip geometry. A child labelled `clip_shape` defines
        // the clip and is NOT rendered itself; its primitive supplies the
        // geometry via `Primitive::clip_path`, so any shape (built-in or
        // extension) works. Without one — or when the primitive has no clip
        // geometry — the clip falls back to a rect covering the Mask's own
        // size; the latter case warns instead of silently clipping wrong.
        let clip_shape_track = track
            .children
            .iter()
            .filter_map(|c| self.tracks.get(c))
            .find(|c| c.label == "clip_shape");
        let fallback_clip = || {
            let w = half_size[0] as f64;
            let h = half_size[1] as f64;
            kurbo::Rect::new(-w, -h, w, h).into_path(1e-3)
        };
        let clip_path: kurbo::BezPath = match clip_shape_track {
            Some(child) => {
                match self.clip_path_for_child(child, time_ms, scene_dimensions, overrides) {
                    Some(path) => path,
                    None => {
                        self.eval_caches.runtime_diagnostics.borrow_mut().push(
                            crate::diagnostics::Diagnostic::warning(
                                crate::diagnostics::DiagnosticCode::RenderFailure,
                                crate::diagnostics::DiagnosticPhase::Render,
                                format!(
                                    "Mask clip_shape '{}' provides no clip geometry; \
                                         using a rectangular clip",
                                    child.label
                                ),
                            ),
                        );
                        fallback_clip()
                    },
                }
            },
            None => fallback_clip(),
        };
        let clip_child_label = clip_shape_track.map(|c| c.label.as_str());

        // Push clip layer. The clip path is in the mask's LOCAL space, so
        // it must be transformed into out.scene space — pushing it with the
        // identity transform pinned the clip at the out.scene origin, clipping
        // away every child of any mask not positioned at the top-left
        // corner (Mask + Image children were the visible symptom).
        out.scene.push_layer(
            vello::peniko::Fill::NonZero,
            vello::peniko::BlendMode::default(),
            1.0,
            global_transform,
            &clip_path,
        );

        // Render all children normally inside the clip
        let children: Vec<&str> = track.children.iter().map(|s| s.as_str()).collect();
        for child in children {
            if clip_child_label == Some(child) {
                // The clip shape defines the clip geometry; it does not
                // render itself.
                continue;
            }
            self.evaluate_node(
                child,
                global_transform,
                global_opacity,
                &child_layout_positions,
                allow_pending_composites,
                frame,
                out,
            );
        }

        // Pop clip layer
        out.scene.pop_layer();
    }

    /// Equation strategy: fragment children aggregate into one Typst document.
    /// Reached only through `EquationPrimitive::render_children`.
    fn render_equation_children(
        &self,
        node_label: &str,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let overrides = frame.overrides;
        let Some(track) = self.tracks.get(node_label) else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
        };
        // ── Equation: compile all child Fragments as one Typst document ──
        let children: Vec<&str> = track.children.iter().map(|s| s.as_str()).collect();

        // Collect Fragment children with their content and highlight state.
        struct FragInfo {
            content: String,
            hl_color: [f32; 4],
            hl_opacity: f32,
            hl_padding: f32,
            hl_radius: f32,
            hl_blend: vello::peniko::Mix,
        }
        let mut frags: Vec<FragInfo> = Vec::new();
        for child_label in &children {
            let Some(child_track) = self.tracks.get(*child_label) else {
                continue;
            };
            // A fragment is any child whose primitive opts into
            // `equation_fragment` — never a hard-coded type name.
            let Some(primitive) = self.track_primitive_at(child_track, time_ms) else {
                continue;
            };
            let child_vector_paths = child_track.evaluate_vector_paths(time_ms);
            let ctx = crate::primitives::EvaluateCtx {
                track: child_track,
                time_ms,
                local_transform: kurbo::Affine::IDENTITY,
                opacity: 1.0,
                scene_dimensions,
                background_color: self.eval_caches.background_color.get(),
                overrides: overrides.get(&child_track.label),
                vector_paths: &child_vector_paths,
                asset_cache: &self.asset_cache,
                target_resolver: Some(self),
            };
            if let Some(fragment) = primitive.equation_fragment(&ctx) {
                frags.push(FragInfo {
                    content: fragment.content,
                    hl_color: fragment.highlight_color,
                    hl_opacity: fragment.highlight_opacity,
                    hl_padding: fragment.highlight_padding,
                    hl_radius: fragment.highlight_radius,
                    hl_blend: fragment.highlight_blend,
                });
            }
        }

        if !frags.is_empty() {
            // Build Typst string: each fragment wrapped in #box() so they
            // produce separate Groups in the output frame.
            let typst_body: String = frags
                .iter()
                .map(|f| format!("#box()[{}]", equation_markup_escaped(&f.content)))
                .collect::<Vec<_>>()
                .join(" ");

            // Use equation-level font_size and color from the Equation track.
            let font_size = track.text.font_size.get(time_ms, 48.0);
            let eq_color = track.style.color.get(time_ms, DEFAULT_WHITE);
            let font_family = track.text.font_family.get(time_ms, String::new());
            let font_weight = track.text.font_weight.get(time_ms, 400.0);
            let font_style = track.text.font_style.get(time_ms, "normal".to_string());
            let line_height = track.text.line_height.get(time_ms, 1.2);
            let letter_spacing = track.text.letter_spacing.get(time_ms, 0.0);
            let word_spacing = track.text.word_spacing.get(time_ms, 0.0);

            // Compile the Typst markup. Memoized process-wide: fragment
            // content and font properties rarely change between frames, so
            // scrubbing an equation-heavy out.scene reuses one compilation
            // instead of re-running Typst every frame.
            match crate::renderer::text::compile_typst_grouped_cached(
                &typst_body,
                font_size,
                eq_color,
                &font_family,
                self.font_context.as_ref(),
                font_weight,
                &font_style,
                line_height,
                letter_spacing,
                word_spacing,
            ) {
                Ok(compiled) => {
                    // One glyph group per #box() wrapper, resolved at
                    // compile time and shared through the memo.
                    let all_glyphs: &[TextPath] = &compiled.glyphs;
                    let ranges: &[std::ops::Range<usize>] = &compiled.ranges;

                    // Compute highlight bounding boxes from the shared
                    // glyph groups before drawing them.
                    let mut highlight_cmds: Vec<crate::primitives::RenderCommand> = Vec::new();
                    for (frag_idx, frag) in frags.iter().enumerate() {
                        if frag.hl_opacity > 0.001 && frag_idx < ranges.len() {
                            let range = &ranges[frag_idx];
                            let mut min_x = f64::INFINITY;
                            let mut max_x = f64::NEG_INFINITY;
                            let mut min_y = f64::INFINITY;
                            let mut max_y = f64::NEG_INFINITY;
                            for tp in &all_glyphs[range.start..range.end] {
                                use kurbo::Shape;
                                let b = tp.path.bounding_box();
                                min_x = min_x.min(b.x0);
                                max_x = max_x.max(b.x1);
                                min_y = min_y.min(b.y0);
                                max_y = max_y.max(b.y1);
                            }
                            if min_x.is_finite() && max_x.is_finite() {
                                let pad = frag.hl_padding as f64;
                                let hl_rect = kurbo::Rect::new(
                                    min_x - pad,
                                    min_y - pad,
                                    max_x + pad,
                                    max_y + pad,
                                );
                                let hl_color = vello::peniko::Color::from_rgba8(
                                    (frag.hl_color[0] * 255.0) as u8,
                                    (frag.hl_color[1] * 255.0) as u8,
                                    (frag.hl_color[2] * 255.0) as u8,
                                    255,
                                );
                                highlight_cmds.push(
                                    crate::primitives::RenderCommand::HighlightLayer {
                                        rect: hl_rect,
                                        color: hl_color,
                                        blend: frag.hl_blend,
                                        alpha: frag.hl_opacity,
                                        corner_radius: frag.hl_radius as f64,
                                    },
                                );
                            }
                        }
                    }

                    // Render all glyphs as a single text command. The memo
                    // owns the glyph slice, so share it by refcount instead
                    // of re-wrapping a freshly built Vec.
                    if !all_glyphs.is_empty() {
                        let cmd = crate::primitives::RenderCommand::Text {
                            paths: std::sync::Arc::clone(&compiled.glyphs),
                        };
                        cmd.execute(out.scene, &global_transform, global_opacity);
                    }

                    // Render highlight overlays.
                    for cmd in &highlight_cmds {
                        cmd.execute(out.scene, &global_transform, global_opacity);
                    }
                },
                Err(e) => {
                    tracing::warn!("Equation Typst compilation failed: {e}");
                },
            }
        }

        // Render Fragment children. They return `None` from evaluate (no
        // visual output of their own), so they draw nothing here and
        // record no hit region; the parent Equation aggregates their
        // content into one document.
        for child in children {
            self.evaluate_node(
                child,
                global_transform,
                global_opacity,
                &child_layout_positions,
                allow_pending_composites,
                frame,
                out,
            );
        }
    }

    /// Apply the Filter strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_filter_children_ctx(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
    ) {
        // The container primitives drive their own recursion through
        // `RenderChildrenCtx` (the documented extension surface); the render
        // recursion itself takes the bundled frame/outputs.
        let frame = crate::primitives::RenderFrame {
            time_ms: ctx.time_ms,
            scene_dimensions: ctx.scene_dimensions,
            debug_options: ctx.debug_options,
            overrides: ctx.overrides,
            frame_env: ctx.frame_env,
        };
        let mut out = crate::primitives::RenderOutputs {
            scene: &mut *ctx.scene,
            hit_regions: &mut *ctx.hit_regions,
            program_items: &mut *ctx.program_items,
            filter_backend: &mut *ctx.filter_backend,
        };
        self.render_filter_children(
            ctx.node_label,
            ctx.global_transform,
            ctx.global_opacity,
            ctx.allow_pending_composites,
            &frame,
            &mut out,
        );
    }

    /// Apply the Mask strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_mask_children_ctx(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
    ) {
        // The container primitives drive their own recursion through
        // `RenderChildrenCtx` (the documented extension surface); the render
        // recursion itself takes the bundled frame/outputs.
        let frame = crate::primitives::RenderFrame {
            time_ms: ctx.time_ms,
            scene_dimensions: ctx.scene_dimensions,
            debug_options: ctx.debug_options,
            overrides: ctx.overrides,
            frame_env: ctx.frame_env,
        };
        let mut out = crate::primitives::RenderOutputs {
            scene: &mut *ctx.scene,
            hit_regions: &mut *ctx.hit_regions,
            program_items: &mut *ctx.program_items,
            filter_backend: &mut *ctx.filter_backend,
        };
        self.render_mask_children(
            ctx.node_label,
            ctx.global_transform,
            ctx.global_opacity,
            ctx.allow_pending_composites,
            &frame,
            &mut out,
        );
    }

    /// Apply the Equation strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_equation_children_ctx(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
    ) {
        // The container primitives drive their own recursion through
        // `RenderChildrenCtx` (the documented extension surface); the render
        // recursion itself takes the bundled frame/outputs.
        let frame = crate::primitives::RenderFrame {
            time_ms: ctx.time_ms,
            scene_dimensions: ctx.scene_dimensions,
            debug_options: ctx.debug_options,
            overrides: ctx.overrides,
            frame_env: ctx.frame_env,
        };
        let mut out = crate::primitives::RenderOutputs {
            scene: &mut *ctx.scene,
            hit_regions: &mut *ctx.hit_regions,
            program_items: &mut *ctx.program_items,
            filter_backend: &mut *ctx.filter_backend,
        };
        self.render_equation_children(
            ctx.node_label,
            ctx.global_transform,
            ctx.global_opacity,
            ctx.allow_pending_composites,
            &frame,
            &mut out,
        );
    }

    /// Evaluate the timeline at the given time and return a rendered `vello::Scene`.
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
        self.evaluate_program_inner(time_s, scene_dimensions, debug_options, filter_backend, false)
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
        self.evaluate_program_inner(time_s, scene_dimensions, debug_options, filter_backend, true)
    }

    fn evaluate_program_inner(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        collect_items: bool,
    ) -> crate::timeline::scene_program::SceneProgram {
        if let Some(program) = self.restore_frame_cache(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            collect_items,
        ) {
            return program;
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
        let debug_or_filter =
            self.backend_can_run(filter_backend) || debug_options != DebugRenderOptions::default();
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
        let mut hit_regions: Vec<(String, kurbo::Rect)> = Vec::new();

        let mut overrides: std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        > = std::collections::HashMap::new();

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
            for root in &self.root_nodes {
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
                if !self.backend_can_run(out.filter_backend)
                    && !self.camera_used.get()
                    && self.is_static_subtree(root)
                {
                    let cache_key = (root.clone(), scene_dimensions, collect_items, debug_options);
                    let cache = self.eval_caches.static_subtree_cache.borrow_mut();
                    if let Some((cached_scene, cached_bounds, cached_items, cached_hit_regions)) =
                        cache.get(&cache_key)
                    {
                        // Fast path: append cached encoding directly and restore the
                        // precise bounds that were computed for this subtree.
                        out.scene.encoding_mut().append(cached_scene.encoding(), &None);
                        {
                            let mut table = self.eval_caches.precise_bounds.borrow_mut();
                            for &(slot, rect) in cached_bounds {
                                table.write(slot, rect);
                            }
                        }
                        if debug_options.compute_hit_regions {
                            out.hit_regions.extend(cached_hit_regions.iter().cloned());
                        }
                        if let Some(items) = out.program_items.as_mut() {
                            items.extend(cached_items.iter().cloned());
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
                                self.evaluate_node(
                                    root,
                                    camera_affine,
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
                        let new_hit_regions: Vec<(String, kurbo::Rect)> =
                            out.hit_regions[subtree_hits_before..].to_vec();
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
                    let allow_pending = self.can_post_composite_filter(root);
                    self.evaluate_node(
                        root,
                        camera_affine,
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
            precise_bounds: program_bounds,
            diagnostics: self.eval_caches.runtime_diagnostics.borrow().clone(),
        };
        if !self.backend_can_run(filter_backend) && debug_options == DebugRenderOptions::default() {
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

        program
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;
    use crate::timeline::{AnimationTrack, PropertyTrack, ShapeType};

    /// Helper to create a minimal Timeline with one root track.
    fn make_minimal_timeline() -> Timeline {
        let mut timeline = Timeline::new();
        let mut track = AnimationTrack::placeholder("test_box".to_string());
        // Set first_seen_ms to 0 so the actor is visible from time 0
        track.first_seen_ms = 0;
        // Give it a shape type
        track.shape.shape_type = Some({
            let mut t = PropertyTrack::new(ShapeType::Rect);
            t.add_keyframe(0, ShapeType::Rect, Easing::Linear);
            t
        });
        // Give it a size so it has content
        track.geometry.size = Some({
            let mut t = PropertyTrack::new([50.0, 50.0]);
            t.add_keyframe(0, [50.0, 50.0], Easing::Linear);
            t
        });
        // Add a color so it renders something visible
        track.style.color = Some({
            let mut t = PropertyTrack::new([1.0, 0.0, 0.0, 1.0]);
            t.add_keyframe(0, [1.0, 0.0, 0.0, 1.0], Easing::Linear);
            t
        });

        timeline.tracks.insert("test_box".to_string(), track);
        timeline.root_nodes.push("test_box".to_string());
        timeline
    }

    #[test]
    fn evaluate_returns_scene_for_simple_timeline() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        let scene = timeline.evaluate(0.0, dimensions);

        // Should return a valid vello Scene (not empty, at least has background)
        // vello::Scene doesn't expose fraction() in all versions; just verify it doesn't panic
        let _ = scene;
    }

    #[test]
    fn evaluate_returns_scene_at_different_times() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        let scene_0 = timeline.evaluate(0.0, dimensions);
        let scene_5 = timeline.evaluate(5.0, dimensions);

        // Both should be valid scenes (no panic)
        let _ = scene_0;
        let _ = scene_5;
    }

    #[test]
    fn evaluate_with_empty_timeline_returns_scene() {
        let timeline = Timeline::new();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        let scene = timeline.evaluate(0.0, dimensions);
        // Should not panic
        let _ = scene;
    }

    #[test]
    fn frame_cache_caches_identical_evaluations() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        // First call should compute and cache
        let scene1 = timeline.evaluate(1.0, dimensions);
        let _ = scene1;

        // Verify the cache is populated
        let cache = timeline.eval_caches.frame_cache.borrow();
        assert!(cache.is_some(), "frame cache should be populated after evaluate");

        if let Some(ref entry) = *cache {
            assert_eq!(entry.time_ms, 1000, "cache should store time in ms (1.0s = 1000ms)");
            assert_eq!(entry.dimensions, dimensions);
        }

        // Second call with same params should use cache
        let scene2 = timeline.evaluate(1.0, dimensions);
        let _ = scene2;

        let cache2 = timeline.eval_caches.frame_cache.borrow();
        assert!(cache2.is_some(), "frame cache should still be populated");
        assert_eq!(cache2.as_ref().unwrap().time_ms, 1000);
    }

    #[test]
    fn program_cache_is_separate_from_scene_only_cache() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        let _scene = timeline.evaluate(1.0, dimensions);
        {
            let cache = timeline.eval_caches.frame_cache.borrow();
            let entry = cache.as_ref().expect("scene evaluation should populate cache");
            assert!(!entry.collect_items);
        }

        let program = timeline.evaluate_program_with_debug(
            1.0,
            dimensions,
            DebugRenderOptions::default(),
            &mut None,
        );
        assert!(!program.items.is_empty());
        {
            let cache = timeline.eval_caches.frame_cache.borrow();
            let entry = cache.as_ref().expect("program evaluation should populate cache");
            assert!(entry.collect_items);
        }
    }

    /// The frame cache is gated on this flag rather than on the backend's mere
    /// presence, so that a runtime which supplies a backend on every frame (the
    /// web player) can still cache. Getting the flag wrong in the permissive
    /// direction is the dangerous one: an effect scene would replay a cached
    /// frame with its composited regions silently missing.
    #[test]
    fn has_effect_scopes_tracks_what_the_scene_actually_contains() {
        let build = |src: &str| {
            let (ast, errors) = animatix_syntax::parser::parse_source(src);
            assert!(errors.is_empty(), "parse errors: {errors:?}");
            Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new())
                .output
        };

        let plain = build(
            "config { resolution: (100, 100) }\n\
             b: Rect, size: (20, 20), color: (1.0, 0.0, 0.0, 1.0), at: (10, 10)\n\
             #0s\nb.opacity = 1.0\n",
        );
        assert!(
            !plain.has_effect_scopes,
            "an effect-free scene reported effect scopes, which would keep the web player out of the frame cache"
        );

        let filtered = build(
            "config { resolution: (100, 100) }\n\
             f: Filter, at: (30, 30), size: (60, 60), blur: 4 {\n\
               b: Rect, size: (20, 20), color: (1.0, 0.0, 0.0, 1.0), at: (10, 10)\n\
             }\n\
             #0s\nf.opacity = 1.0\n",
        );
        assert!(
            filtered.has_effect_scopes,
            "a `Filter` scope was not detected; the frame cache would swallow its composites"
        );
    }

    #[test]
    fn frame_cache_restores_runtime_diagnostics_on_hit() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        let _program = timeline.evaluate_program_with_debug(
            1.0,
            dimensions,
            DebugRenderOptions::default(),
            &mut None,
        );
        timeline
            .eval_caches
            .frame_cache
            .borrow_mut()
            .as_mut()
            .expect("cache populated")
            .program
            .diagnostics
            .push(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::DiagnosticCode::ModifierRuntimeError,
                crate::diagnostics::DiagnosticPhase::Render,
                "simulated t=1 diagnostic".to_string(),
            ));

        // Scrub to the same frame again. The cache hit must restore the
        // frame's diagnostics instead of leaving the newly evaluated frame's
        // empty diagnostics in the timeline.
        let _hit = timeline.evaluate_program_with_debug(
            1.0,
            dimensions,
            DebugRenderOptions::default(),
            &mut None,
        );
        let diagnostics = timeline.runtime_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "simulated t=1 diagnostic");
    }

    #[test]
    fn frame_cache_misses_on_different_time() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        let _scene1 = timeline.evaluate(0.0, dimensions);
        let _scene2 = timeline.evaluate(2.0, dimensions);

        // Cache should contain the latest evaluation (t=2.0)
        let cache = timeline.eval_caches.frame_cache.borrow();
        assert!(cache.is_some(), "cache should be populated");
        assert_eq!(cache.as_ref().unwrap().time_ms, 2000, "cache should contain t=2.0");
    }

    #[test]
    fn frame_cache_misses_on_different_dimensions() {
        let timeline = make_minimal_timeline();

        let dims_1 = SceneDimensions {
            width: 800,
            height: 600,
        };
        let dims_2 = SceneDimensions {
            width: 1920,
            height: 1080,
        };

        let _scene1 = timeline.evaluate(0.0, dims_1);

        // Cache should have dims_1
        {
            let cache = timeline.eval_caches.frame_cache.borrow();
            assert_eq!(cache.as_ref().unwrap().dimensions, dims_1);
        }

        let _scene2 = timeline.evaluate(0.0, dims_2);

        // Cache should now have dims_2
        {
            let cache = timeline.eval_caches.frame_cache.borrow();
            assert_eq!(cache.as_ref().unwrap().dimensions, dims_2);
        }
    }

    #[test]
    fn hit_regions_are_populated_after_evaluate() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        // hit_regions should be empty before evaluate
        {
            let regions = timeline.eval_caches.hit_regions.borrow();
            assert!(regions.is_empty(), "hit_regions should be empty before evaluate");
        }

        let _scene = timeline.evaluate_with_debug(
            0.0,
            dimensions,
            DebugRenderOptions {
                draw_bounds: false,
                compute_hit_regions: true,
                ..Default::default()
            },
            &mut None,
        );

        // hit_regions should be populated after evaluate
        let regions = timeline.eval_caches.hit_regions.borrow();
        assert!(!regions.is_empty(), "hit_regions should be populated after evaluate");
        assert!(
            regions.iter().any(|(label, _)| label == "test_box"),
            "hit_regions should contain 'test_box'"
        );
    }

    #[test]
    fn hit_regions_contain_world_bounds() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        let _scene = timeline.evaluate_with_debug(
            0.0,
            dimensions,
            DebugRenderOptions {
                draw_bounds: false,
                compute_hit_regions: true,
                ..Default::default()
            },
            &mut None,
        );

        let regions = timeline.eval_caches.hit_regions.borrow();
        let (label, bounds) = regions
            .iter()
            .find(|(l, _)| l == "test_box")
            .expect("should find test_box in hit_regions");

        assert_eq!(label, "test_box");
        // The bounds should be valid rectangles (x0 < x1, y0 < y1)
        assert!(bounds.x0 < bounds.x1, "hit region x0 should be less than x1");
        assert!(bounds.y0 < bounds.y1, "hit region y0 should be less than y1");
    }

    #[test]
    fn hit_regions_cover_actors_without_draw_commands() {
        // Container shells return `Some(vec![])` and stay hit-testable at
        // their layout box; empty-content leaf actors (empty Text) return
        // `None` and intentionally record no hit region (see
        // `runtime_empty_text_override_clears_stale_glyphs`).
        let mut timeline = Timeline::new();

        // A Group container: evaluate() -> Some(vec![]) (empty command list).
        let mut group = AnimationTrack::placeholder("grp".to_string());
        group.first_seen_ms = 0;
        group.set_identity("Group");
        group.actor_type = "Group".to_string();
        group.geometry.size = {
            let mut t = PropertyTrack::new([40.0, 30.0]);
            t.add_keyframe(0, [40.0, 30.0], Easing::Linear);
            Some(t)
        };
        timeline.tracks.insert("grp".to_string(), group);

        // An empty Text actor: evaluate() -> Ok(None), no content glyphs.
        let mut empty_text = AnimationTrack::placeholder("empty".to_string());
        empty_text.first_seen_ms = 0;
        empty_text.set_identity("Text");
        empty_text.actor_type = "Text".to_string();
        empty_text.geometry.size = {
            let mut t = PropertyTrack::new([60.0, 20.0]);
            t.add_keyframe(0, [60.0, 20.0], Easing::Linear);
            Some(t)
        };
        timeline.tracks.insert("empty".to_string(), empty_text);

        timeline.root_nodes.push("grp".to_string());
        timeline.root_nodes.push("empty".to_string());

        let _scene = timeline.evaluate_with_debug(
            0.0,
            SceneDimensions {
                width: 800,
                height: 600,
            },
            DebugRenderOptions {
                draw_bounds: false,
                compute_hit_regions: true,
                ..Default::default()
            },
            &mut None,
        );

        let regions = timeline.eval_caches.hit_regions.borrow();
        let (found_grp, bounds_grp) = regions
            .iter()
            .find(|(l, _)| l == "grp")
            .expect("container shell should keep its hit region");
        assert_eq!(found_grp, "grp");
        assert!(bounds_grp.x0 < bounds_grp.x1, "grp hit region x0 < x1");
        assert!(bounds_grp.y0 < bounds_grp.y1, "grp hit region y0 < y1");
        assert!(
            !regions.iter().any(|(l, _)| l == "empty"),
            "empty-content actors must not record a hit region"
        );
    }

    #[test]
    fn evaluate_with_debug_options_skips_cache() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };
        let debug_opts = DebugRenderOptions {
            draw_bounds: true,
            compute_hit_regions: false,
            ..Default::default()
        };

        // Evaluate with debug options (should not cache)
        let _scene = timeline.evaluate_with_debug(0.0, dimensions, debug_opts, &mut None);

        // Cache should not be populated because debug_options != default
        let cache = timeline.eval_caches.frame_cache.borrow();
        assert!(
            cache.is_none(),
            "frame cache should not be populated with non-default debug options"
        );
    }

    #[test]
    fn mask_clips_children_to_own_bounds() {
        let mut timeline = Timeline::new();

        // Create child actor
        let mut child_track = AnimationTrack::placeholder("child".to_string());
        child_track.first_seen_ms = 0;
        child_track.shape.shape_type = Some({
            let mut t = PropertyTrack::new(ShapeType::Rect);
            t.add_keyframe(0, ShapeType::Rect, Easing::Linear);
            t
        });
        child_track.geometry.size = Some({
            let mut t = PropertyTrack::new([50.0, 50.0]);
            t.add_keyframe(0, [50.0, 50.0], Easing::Linear);
            t
        });
        child_track.style.color = Some({
            let mut t = PropertyTrack::new([1.0, 0.0, 0.0, 1.0]);
            t.add_keyframe(0, [1.0, 0.0, 0.0, 1.0], Easing::Linear);
            t
        });
        timeline.tracks.insert("child".to_string(), child_track);

        // Create Mask actor
        let mut mask_track = AnimationTrack::placeholder("mask".to_string());
        mask_track.first_seen_ms = 0;
        mask_track.set_identity("Mask");
        mask_track.geometry.size = Some({
            let mut t = PropertyTrack::new([100.0, 100.0]);
            t.add_keyframe(0, [100.0, 100.0], Easing::Linear);
            t
        });
        mask_track.children.push("child".to_string());
        timeline.tracks.insert("mask".to_string(), mask_track);

        timeline.root_nodes.push("mask".to_string());

        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };
        let scene = timeline.evaluate(0.0, dimensions);

        // Should not panic; returns a valid scene
        let _ = scene;
    }

    #[test]
    fn target_bounds_prefer_precise_command_bounds() {
        let timeline = make_minimal_timeline();
        // The declared size box is 100x100 centered at (0,0), but the precise
        // command bounds describe a different world AABB.
        timeline.ensure_bounds_registry();
        let slot = timeline.tracks.get("test_box").expect("track").bounds_slot.get();
        timeline
            .eval_caches
            .precise_bounds
            .borrow_mut()
            .write(slot, kurbo::Rect::new(50.0, 40.0, 100.0, 80.0));
        let (centre, half) = crate::timeline::callout_geometry::TargetResolver::target_bounds(
            &timeline,
            "test_box",
            0,
            SceneDimensions {
                width: 800,
                height: 600,
            },
        )
        .expect("target bounds");
        assert_eq!(centre, [75.0, 60.0]);
        assert_eq!(half, [25.0, 20.0]);
    }

    #[test]
    fn frame_cache_restores_precise_bounds_on_hit() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions {
            width: 800,
            height: 600,
        };

        let _scene = timeline.evaluate(0.0, dimensions);
        let cached = timeline
            .precise_bounds_for("test_box")
            .expect("precise bounds should be populated after evaluation");
        let _scene = timeline.evaluate(0.0, dimensions);
        assert_eq!(
            timeline.precise_bounds_for("test_box"),
            Some(cached),
            "frame-cache hit should restore precise bounds"
        );
    }

    #[test]
    fn equation_fragment_leading_marker_is_escaped() {
        // A fragment starting with '+' used to render as a Typst enum marker
        // ("1.") once fragments became separate #box() markup blocks.
        assert_eq!(equation_markup_escaped("+ sin(3x)/3"), "\\+ sin(3x)/3");
        assert_eq!(equation_markup_escaped("  + 0.55 dot "), "  \\+ 0.55 dot ");
        assert_eq!(equation_markup_escaped("- b"), "\\- b");
        assert_eq!(equation_markup_escaped("sin(x)"), "sin(x)");
        assert_eq!(equation_markup_escaped("y(x) = "), "y(x) = ");
    }

    fn make_root_track(label: &str) -> AnimationTrack {
        let mut track = AnimationTrack::placeholder(label.to_string());
        track.first_seen_ms = 0;
        track
    }

    /// A root-level filter may only take the zero-readback post-render blit
    /// when it is the LAST root node; otherwise later siblings would render
    /// underneath the blit. (Regression: the last-among-siblings check only
    /// ran for i > 0, so root filters always deferred and landed on top.)
    #[test]
    fn root_filter_post_composite_requires_last_root_node() {
        let mut timeline = Timeline::new();
        timeline.tracks.insert("fx".to_string(), make_root_track("fx"));
        timeline.tracks.insert("late".to_string(), make_root_track("late"));
        timeline.root_nodes.push("fx".to_string());
        timeline.root_nodes.push("late".to_string());

        assert!(
            !timeline.can_post_composite_filter("fx"),
            "filter followed by a later sibling must not post-composite"
        );

        let mut timeline = Timeline::new();
        timeline.tracks.insert("late".to_string(), make_root_track("late"));
        timeline.tracks.insert("fx".to_string(), make_root_track("fx"));
        timeline.root_nodes.push("late".to_string());
        timeline.root_nodes.push("fx".to_string());

        assert!(
            timeline.can_post_composite_filter("fx"),
            "a last root filter may post-composite"
        );
    }
}
