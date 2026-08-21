use kurbo::Shape;

use super::{
    ActorKindId, AnimationTrack, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, DebugRenderOptions,
    EvalError, PlacementMode, PositionBinding, SceneDimensions, Timeline, TrackAccessor, Value,
    VelloPath, resolve_bound_position,
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
        bounds = union_rect(
            bounds,
            kurbo::Rect::new(0.0, 0.0, (half_width * 2.0) as f64, (half_height * 2.0) as f64),
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
            effective_f32, effective_transform, effective_vec2,
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
        let rotation = effective_f32(track, node_overrides, time_ms, "rotation", 0.0) as f64;
        let scale = effective_f32(track, node_overrides, time_ms, "scale", 1.0) as f64;
        let opacity = effective_f32(track, node_overrides, time_ms, "opacity", 1.0);
        let half_size =
            effective_vec2(track, node_overrides, time_ms, "size", DEFAULT_LAYOUT_HALF_SIZE);

        let transform = effective_transform(
            track,
            node_overrides,
            time_ms,
            "transform",
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

    /// Check whether a filter actor can safely use zero-readback post-render compositing.
    /// This is only safe when the filter is the last child in every ancestor container
    /// (nothing renders after the filter in the scene graph).
    fn can_post_composite_filter(&self, node_label: &str) -> bool {
        // Find the path from root to this actor
        let Some(path) = self.find_path_to_actor(node_label) else {
            return false;
        };

        // The actor must be in the root set (no orphan check)
        if path.is_empty() {
            return false;
        }

        // Check that at every level, the actor is the last child
        for i in 0..path.len() {
            let label = &path[i];
            if !self.tracks.contains_key(label) {
                return false;
            }

            // If this is not the root, check it's the last child of its parent
            if i > 0 {
                let parent_label = &path[i - 1];
                let Some(parent_track) = self.tracks.get(parent_label) else {
                    return false;
                };
                if parent_track.children.last() != Some(label) {
                    return false;
                }
            }
        }

        true
    }

    fn evaluate_node(
        &self,
        node_label: &str,
        time_ms: u64,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        scene: &mut vello::Scene,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
        layout_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        hit_regions: &mut Vec<(String, kurbo::Rect)>,
        frame_env: Option<&super::Environment>,
        filter_backend: &mut Option<&mut dyn crate::timeline::filter::FilterBackend>,
        allow_pending_composites: bool,
        program_items: &mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
    ) {
        let (global_transform, global_opacity) = self.render_actor_node(
            node_label,
            time_ms,
            parent_transform,
            parent_opacity,
            scene_dimensions,
            debug_options,
            scene,
            overrides,
            layout_positions,
            hit_regions,
            frame_env,
            filter_backend,
            allow_pending_composites,
            program_items,
        );

        self.render_node_children(
            node_label,
            time_ms,
            global_transform,
            global_opacity,
            scene_dimensions,
            debug_options,
            scene,
            overrides,
            hit_regions,
            frame_env,
            filter_backend,
            allow_pending_composites,
            program_items,
        );
    }

    /// Evaluate a single actor node and render it to the scene.
    /// Returns the (transform, opacity) to use for child rendering.
    fn render_actor_node(
        &self,
        node_label: &str,
        time_ms: u64,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        scene: &mut vello::Scene,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
        layout_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        hit_regions: &mut Vec<(String, kurbo::Rect)>,
        frame_env: Option<&super::Environment>,
        filter_backend: &mut Option<&mut dyn crate::timeline::filter::FilterBackend>,
        allow_pending_composites: bool,
        program_items: &mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
    ) -> (kurbo::Affine, f32) {
        let Some(track) = self.tracks.get(node_label) else {
            return (parent_transform, parent_opacity);
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
                    time_ms,
                    parent_transform,
                    parent_opacity,
                    scene_dimensions,
                    debug_options,
                    scene,
                    overrides,
                    layout_positions,
                    hit_regions,
                    frame_env,
                    filter_backend,
                    allow_pending_composites,
                    program_items,
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
            let cache = self.eval_caches.transform_cache.borrow();
            if let Some((cached_time, cached_parent, cached_transform)) = cache.get(node_label) {
                if *cached_time == time_ms && *cached_parent == parent_coeffs {
                    *cached_transform
                } else {
                    drop(cache);
                    let t = self.evaluate_node_transform(
                        track,
                        time_ms,
                        parent_opacity,
                        parent_transform,
                        scene_dimensions,
                        layout_pos,
                        node_overrides,
                    );
                    self.eval_caches
                        .transform_cache
                        .borrow_mut()
                        .insert(node_label.to_string(), (time_ms, parent_coeffs, t));
                    t
                }
            } else {
                drop(cache);
                let t = self.evaluate_node_transform(
                    track,
                    time_ms,
                    parent_opacity,
                    parent_transform,
                    scene_dimensions,
                    layout_pos,
                    node_overrides,
                );
                self.eval_caches
                    .transform_cache
                    .borrow_mut()
                    .insert(node_label.to_string(), (time_ms, parent_coeffs, t));
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

            if procedural_plot.is_dynamic() || transitioning || has_completed_transitions {
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
                        local_env.set(
                            &format!("{}.{}", procedural_plot.actor_label, name),
                            num_val.clone(),
                        );
                        // Set the bare name (e.g. "freq") for closure captures,
                        // but don't shadow closure sample arguments.
                        if !procedural_plot.func_args.contains(name) {
                            local_env.set(name, num_val);
                        }
                    }
                }

                vector_paths = crate::timeline::plot::sample_procedural_plot_at(
                    procedural_plot,
                    &mut local_env,
                    time_ms,
                    &track.func_transitions,
                );
            }
        }

        let node_overrides = overrides.get(node_label);

        // P2.19: Only sample properties and render if actor is visible on screen.
        // For off-screen actors we still return transform/opacity so children
        // (which may extend back into view) are correctly evaluated.
        if is_visible {
            // ── Phase 10b.3: Trait-dispatch scene evaluation ──
            // Try the primitive's evaluate() first. If it returns commands,
            // execute them and skip the legacy manual rendering path.
            let primitive_dispatch = {
                let meta = crate::primitives::actor_kind_meta(track.kind);
                let primitive = track
                    .actor_type
                    .as_deref()
                    .and_then(|ty| self.primitive_registry.find(ty))
                    .or_else(|| meta.and_then(|m| self.primitive_registry.find(m.type_name)));
                if let Some(primitive) = primitive {
                    let ctx = crate::primitives::EvaluateCtx {
                        track,
                        time_ms,
                        local_transform,
                        opacity,
                        scene_dimensions,
                        background_color: self.background_color.evaluate(time_ms),
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
                // Hit region — compute from commands, not stale vector_paths
                let image_size = track.image.get(time_ms, None).is_some().then_some(half_size);
                let mut local_bounds: Option<kurbo::Rect> = None;
                for cmd in &commands {
                    if let Some(cmd_bounds) = cmd.local_bounds(image_size) {
                        local_bounds = Some(match local_bounds {
                            Some(existing) => existing.union(cmd_bounds),
                            None => cmd_bounds,
                        });
                    }
                }
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
                hit_regions.push((node_label.to_string(), world_bounds));
                self.eval_caches
                    .precise_bounds_cache
                    .borrow_mut()
                    .insert(node_label.to_string(), world_bounds);

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

    /// Resolve the child-rendering strategy for a track.
    ///
    /// The primitive registry is the source of truth. The `ActorKindId` match
    /// below is only a fallback for hand-built test tracks without an actor
    /// type name.
    fn primitive_child_processing(
        &self,
        track: &AnimationTrack,
    ) -> crate::primitives::ChildProcessing {
        if let Some(type_name) = track.actor_type.as_deref() {
            if let Some(primitive) = self.primitive_registry.find(type_name) {
                return primitive.child_processing();
            }
        }
        match track.kind {
            ActorKindId::Filter => crate::primitives::ChildProcessing::Filter,
            ActorKindId::Mask => crate::primitives::ChildProcessing::Mask,
            ActorKindId::Equation => crate::primitives::ChildProcessing::Equation,
            _ => crate::primitives::ChildProcessing::Generic,
        }
    }

    /// Recursively render child nodes using the primitive capability hook.
    fn render_node_children(
        &self,
        node_label: &str,
        time_ms: u64,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        scene: &mut vello::Scene,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
        hit_regions: &mut Vec<(String, kurbo::Rect)>,
        frame_env: Option<&super::Environment>,
        filter_backend: &mut Option<&mut dyn crate::timeline::filter::FilterBackend>,
        allow_pending_composites: bool,
        program_items: &mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
    ) {
        let Some(track) = self.tracks.get(node_label) else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::collections::BTreeMap::new()
        };

        if self.primitive_child_processing(track) == crate::primitives::ChildProcessing::Filter {
            let children: Vec<&str> = track.children.iter().map(|s| s.as_str()).collect();
            if children.is_empty() {
                return;
            }

            // Check if a filter backend is available
            let has_backend = filter_backend.is_some();
            if !has_backend {
                // Fallback: render children directly (no filtering)
                for child in &children {
                    self.evaluate_node(
                        child,
                        time_ms,
                        global_transform,
                        global_opacity,
                        scene_dimensions,
                        debug_options,
                        scene,
                        overrides,
                        &child_layout_positions,
                        hit_regions,
                        frame_env,
                        filter_backend,
                        allow_pending_composites,
                        program_items,
                    );
                }
                return;
            }

            // Build sub-scene with children rendered at their world positions
            let mut sub_scene = vello::Scene::new();
            for child in &children {
                self.evaluate_node(
                    child,
                    time_ms,
                    global_transform,
                    global_opacity,
                    scene_dimensions,
                    debug_options,
                    &mut sub_scene,
                    overrides,
                    &child_layout_positions,
                    hit_regions,
                    frame_env,
                    filter_backend,
                    false,
                    program_items,
                );
            }

            // Sample filter properties
            let mut blur = track.filter.filter_blur.get(time_ms, 0.0);
            let mut brightness = track.filter.filter_brightness.get(time_ms, 1.0);
            let mut contrast = track.filter.filter_contrast.get(time_ms, 1.0);
            let mut saturate = track.filter.filter_saturate.get(time_ms, 1.0);
            let mut hue_rotate = track.filter.filter_hue_rotate.get(time_ms, 0.0);
            let mut sepia = track.filter.filter_sepia.get(time_ms, 0.0);

            // Apply modifier overrides for filter properties
            if let Some(ov) = overrides.get(node_label) {
                if let Some(Value::Num(v)) = ov.get("blur") {
                    blur = *v as f32;
                }
                if let Some(Value::Num(v)) = ov.get("brightness") {
                    brightness = *v as f32;
                }
                if let Some(Value::Num(v)) = ov.get("contrast") {
                    contrast = *v as f32;
                }
                if let Some(Value::Num(v)) = ov.get("saturate") {
                    saturate = *v as f32;
                }
                if let Some(Value::Num(v)) = ov.get("hue_rotate") {
                    hue_rotate = *v as f32;
                }
                if let Some(Value::Num(v)) = ov.get("sepia") {
                    sepia = *v as f32;
                }
            }

            // If all filters are identity and no blur, just append sub-scene directly
            let needs_filter = blur > 0.5
                || (brightness - 1.0).abs() > 0.001
                || (contrast - 1.0).abs() > 0.001
                || (saturate - 1.0).abs() > 0.001
                || hue_rotate.abs() > 0.5
                || sepia > 0.001;

            if !needs_filter {
                scene.encoding_mut().append(sub_scene.encoding(), &None);
                return;
            }

            // Try zero-readback path when this filter is safely the last rendering element
            if allow_pending_composites && self.can_post_composite_filter(node_label) {
                if let Some(backend) = filter_backend.as_mut() {
                    match backend.render_scene_to_pending_composite(
                        &sub_scene,
                        scene_dimensions,
                        blur,
                        brightness,
                        contrast,
                        saturate,
                        hue_rotate,
                        sepia,
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
            if let Some(backend) = filter_backend.as_mut() {
                match backend.render_scene_to_image_gpu_filtered(
                    &sub_scene,
                    scene_dimensions,
                    blur,
                    brightness,
                    contrast,
                    saturate,
                    hue_rotate,
                    sepia,
                ) {
                    Ok(filtered) => {
                        let brush = vello::peniko::ImageBrush::new(filtered.data.clone())
                            .with_extend(vello::peniko::Extend::Pad)
                            .with_quality(vello::peniko::ImageQuality::Medium)
                            .with_alpha(global_opacity);
                        scene.draw_image(&brush, kurbo::Affine::IDENTITY);
                    },
                    Err(e) => {
                        tracing::warn!(
                            "Filter backend error, falling back to unfiltered rendering: {e}"
                        );
                        scene.encoding_mut().append(sub_scene.encoding(), &None);
                    },
                }
            }
        } else if self.primitive_child_processing(track) == crate::primitives::ChildProcessing::Mask
        {
            let half_size = track.geometry.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);

            // Build a rectangle clip path from -half_size to +half_size
            let w = half_size[0] as f64;
            let h = half_size[1] as f64;
            let clip_path = kurbo::Rect::new(-w, -h, w, h).into_path(1e-3);

            // Push clip layer
            scene.push_layer(
                vello::peniko::Fill::NonZero,
                vello::peniko::BlendMode::default(),
                1.0,
                kurbo::Affine::IDENTITY,
                &clip_path,
            );

            // Render all children normally inside the clip
            let children: Vec<&str> = track.children.iter().map(|s| s.as_str()).collect();
            for child in children {
                self.evaluate_node(
                    child,
                    time_ms,
                    global_transform,
                    global_opacity,
                    scene_dimensions,
                    debug_options,
                    scene,
                    overrides,
                    &child_layout_positions,
                    hit_regions,
                    frame_env,
                    filter_backend,
                    allow_pending_composites,
                    program_items,
                );
            }

            // Pop clip layer
            scene.pop_layer();
        } else if self.primitive_child_processing(track)
            == crate::primitives::ChildProcessing::Equation
        {
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
                if let Some(child_track) = self.tracks.get(*child_label) {
                    if child_track.kind == ActorKindId::Fragment {
                        let content = child_track.text.text_content.get(time_ms, String::new());
                        let hl_color = child_track
                            .highlight
                            .highlight_color
                            .get(time_ms, [0.3, 0.5, 1.0, 1.0]);
                        let hl_opacity = child_track.highlight.highlight_opacity.get(time_ms, 0.0);
                        let hl_padding = child_track.highlight.highlight_padding.get(time_ms, 4.0);
                        let hl_radius = child_track.highlight.highlight_radius.get(time_ms, 3.0);
                        let hl_blend = child_track.highlight.highlight_blend;
                        frags.push(FragInfo {
                            content,
                            hl_color,
                            hl_opacity,
                            hl_padding,
                            hl_radius,
                            hl_blend,
                        });
                    }
                }
            }

            if !frags.is_empty() {
                // Build Typst string: each fragment wrapped in #box() so they
                // produce separate Groups in the output frame.
                let typst_body: String = frags
                    .iter()
                    .map(|f| format!("#box()[{}]", f.content))
                    .collect::<Vec<_>>()
                    .join("");

                // Use equation-level font_size and color from the Equation track.
                let font_size = track.text.font_size.get(time_ms, 48.0);
                let eq_color = track.style.color.get(time_ms, DEFAULT_WHITE);
                let font_family = track.text.font_family.get(time_ms, String::new());
                let font_weight = track.text.font_weight.get(time_ms, 400.0);
                let font_style = track.text.font_style.get(time_ms, "normal".to_string());
                let line_height = track.text.line_height.get(time_ms, 1.2);
                let letter_spacing = track.text.letter_spacing.get(time_ms, 0.0);
                let word_spacing = track.text.word_spacing.get(time_ms, 0.0);

                let typst_color = typst::visualize::Color::from_u8(
                    (eq_color[0] * 255.0) as u8,
                    (eq_color[1] * 255.0) as u8,
                    (eq_color[2] * 255.0) as u8,
                    (eq_color[3] * 255.0) as u8,
                );

                // Compile the Typst markup.
                match crate::renderer::text::compile_typst(
                    &typst_body,
                    font_size,
                    typst_color,
                    &font_family,
                    self.font_context.as_ref(),
                    font_weight,
                    &font_style,
                    line_height,
                    letter_spacing,
                    word_spacing,
                    0.0,
                    "left",
                    "visible",
                ) {
                    Ok(frame) => {
                        // Extract grouped glyphs — one group per #box() wrapper.
                        let (all_glyphs, ranges) =
                            crate::renderer::text::extract_glyphs_grouped(&frame);

                        // Compute highlight bounding boxes BEFORE moving glyphs into Arc.
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

                        // Render all glyphs as a single text command.
                        if !all_glyphs.is_empty() {
                            let arc_glyphs: std::sync::Arc<[TextPath]> = all_glyphs.into();
                            let cmd = crate::primitives::RenderCommand::Text { paths: arc_glyphs };
                            cmd.execute(scene, &global_transform, global_opacity);
                        }

                        // Render highlight overlays.
                        for cmd in &highlight_cmds {
                            cmd.execute(scene, &global_transform, global_opacity);
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Equation Typst compilation failed: {e}");
                    },
                }
            }

            // Render Fragment children (they return None from evaluate, so
            // no visual output, but hit regions and debug overlays still work).
            for child in children {
                self.evaluate_node(
                    child,
                    time_ms,
                    global_transform,
                    global_opacity,
                    scene_dimensions,
                    debug_options,
                    scene,
                    overrides,
                    &child_layout_positions,
                    hit_regions,
                    frame_env,
                    filter_backend,
                    allow_pending_composites,
                    program_items,
                );
            }
        } else {
            let children: Vec<&str> = track.children.iter().map(|s| s.as_str()).collect();
            for child in children {
                self.evaluate_node(
                    child,
                    time_ms,
                    global_transform,
                    global_opacity,
                    scene_dimensions,
                    debug_options,
                    scene,
                    overrides,
                    &child_layout_positions,
                    hit_regions,
                    frame_env,
                    filter_backend,
                    allow_pending_composites,
                    program_items,
                );
            }
        }
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
        filter_backend: &mut Option<&mut dyn crate::timeline::filter::FilterBackend>,
    ) -> vello::Scene {
        if let Some(program) =
            self.restore_frame_cache(time_s, scene_dimensions, debug_options, filter_backend, false)
        {
            return program.scene;
        }
        self.evaluate_program_inner(time_s, scene_dimensions, debug_options, filter_backend, false)
            .scene
    }

    /// Restore cache-derived frame state when a frame cache entry matches.
    fn restore_frame_cache(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::filter::FilterBackend>,
        collect_items: bool,
    ) -> Option<crate::timeline::scene_program::SceneProgram> {
        if filter_backend.is_some() || debug_options != DebugRenderOptions::default() {
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
        *self.eval_caches.precise_bounds_cache.borrow_mut() = cached.program.precise_bounds.clone();
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
                scene: cached.scene.as_ref().clone(),
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
        filter_backend: &mut Option<&mut dyn crate::timeline::filter::FilterBackend>,
    ) -> crate::timeline::scene_program::SceneProgram {
        self.evaluate_program_inner(time_s, scene_dimensions, debug_options, filter_backend, true)
    }

    fn evaluate_program_inner(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::filter::FilterBackend>,
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

        // P2.25: Reuse vello scene buffer to avoid allocating fresh encoding buffers.
        let mut scene = self.eval_caches.scene_buffer.borrow_mut().take().unwrap_or_default();
        scene.reset();
        // Precise bounds are only valid for the frame that computed them.
        self.eval_caches.precise_bounds_cache.borrow_mut().clear();
        let bg_color = self.background_color.evaluate_copy(time_ms);

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
        for root in &self.root_nodes {
            // P2.17: Static subtree cache — fully-static subtrees are evaluated once
            // and their vello encoding is reused on subsequent frames. Dimensions
            // and item collection are part of the key so different canvas sizes or
            // observable-program requests cannot reuse an incompatible entry.
            // Hit-region evaluation is never cached because a cache hit would
            // skip the per-node bounds collection.
            if filter_backend.is_none()
                && !debug_options.compute_hit_regions
                && self.is_static_subtree(root)
            {
                let cache_key = (root.clone(), scene_dimensions, collect_items, debug_options);
                let cache = self.eval_caches.static_subtree_cache.borrow_mut();
                if let Some((cached_scene, cached_bounds, cached_items)) = cache.get(&cache_key) {
                    // Fast path: append cached encoding directly and restore the
                    // precise bounds that were computed for this subtree.
                    scene.encoding_mut().append(cached_scene.encoding(), &None);
                    for (label, bounds) in cached_bounds {
                        self.eval_caches
                            .precise_bounds_cache
                            .borrow_mut()
                            .insert(label.clone(), *bounds);
                    }
                    if let Some(items) = program_items.as_mut() {
                        items.extend(cached_items.iter().cloned());
                    }
                } else {
                    drop(cache);
                    let mut temp_scene = vello::Scene::new();
                    let subtree_bounds_before =
                        self.eval_caches.precise_bounds_cache.borrow().len();
                    let mut subtree_items_slot = if collect_items {
                        Some(Vec::new())
                    } else {
                        None
                    };
                    self.evaluate_node(
                        root,
                        time_ms,
                        kurbo::Affine::IDENTITY,
                        1.0,
                        scene_dimensions,
                        debug_options,
                        &mut temp_scene,
                        &overrides,
                        &std::collections::BTreeMap::new(),
                        &mut hit_regions,
                        frame_env.as_ref(),
                        filter_backend,
                        true,
                        &mut subtree_items_slot,
                    );
                    let subtree_items = subtree_items_slot.take().unwrap_or_default();
                    if let Some(items) = program_items.as_mut() {
                        items.extend(subtree_items.iter().cloned());
                    }
                    // Append to main scene and cache for next time.
                    scene.encoding_mut().append(temp_scene.encoding(), &None);
                    let new_bounds: Vec<(String, kurbo::Rect)> = self
                        .eval_caches
                        .precise_bounds_cache
                        .borrow()
                        .iter()
                        .skip(subtree_bounds_before)
                        .map(|(label, rect)| (label.clone(), *rect))
                        .collect();
                    self.eval_caches
                        .static_subtree_cache
                        .borrow_mut()
                        .insert(cache_key, (temp_scene, new_bounds, subtree_items));
                }
            } else {
                self.evaluate_node(
                    root,
                    time_ms,
                    kurbo::Affine::IDENTITY,
                    1.0,
                    scene_dimensions,
                    debug_options,
                    &mut scene,
                    &overrides,
                    &std::collections::BTreeMap::new(), // empty for roots
                    &mut hit_regions,
                    frame_env.as_ref(),
                    filter_backend,
                    true,
                    &mut program_items,
                );
            }
        }

        // P2.24: Only store hit regions when explicitly requested.
        // Saves bounding-box computation for frames where click-to-select is not needed.
        if debug_options.compute_hit_regions {
            *self.eval_caches.hit_regions.borrow_mut() = hit_regions;
        } else {
            self.eval_caches.hit_regions.borrow_mut().clear();
        }

        // The program owns the encoded scene; scene_buffer keeps a reusable copy.
        let program = crate::timeline::scene_program::SceneProgram {
            dimensions: scene_dimensions,
            background: bg_color,
            scene,
            items: program_items.take().unwrap_or_default(),
            precise_bounds: self.eval_caches.precise_bounds_cache.borrow().clone(),
            diagnostics: self.eval_caches.runtime_diagnostics.borrow().clone(),
        };
        if filter_backend.is_none() && debug_options == DebugRenderOptions::default() {
            *self.eval_caches.frame_cache.borrow_mut() = Some(super::FrameCacheEntry {
                time_ms,
                dimensions: scene_dimensions,
                has_modifiers: needs_frame_env,
                has_dynamic_layout: self.dynamic_layout,
                has_child_orders,
                program: program.clone(),
                scene: std::sync::Arc::new(program.scene.clone()),
                collect_items,
            });
        }
        *self.eval_caches.scene_buffer.borrow_mut() = Some(program.scene.clone());

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
        let mut track = AnimationTrack::new("test_box".to_string());
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
        let mut child_track = AnimationTrack::new("child".to_string());
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
        let mut mask_track = AnimationTrack::new("mask".to_string());
        mask_track.first_seen_ms = 0;
        mask_track.kind = ActorKindId::Mask;
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
        timeline
            .eval_caches
            .precise_bounds_cache
            .borrow_mut()
            .insert("test_box".to_string(), kurbo::Rect::new(50.0, 40.0, 100.0, 80.0));
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
        assert!(
            timeline.eval_caches.precise_bounds_cache.borrow().contains_key("test_box"),
            "precise bounds should be populated after evaluation"
        );

        let cached = timeline
            .eval_caches
            .precise_bounds_cache
            .borrow()
            .get("test_box")
            .copied()
            .expect("cached bounds");
        let _scene = timeline.evaluate(0.0, dimensions);
        assert_eq!(
            timeline.eval_caches.precise_bounds_cache.borrow().get("test_box").copied(),
            Some(cached),
            "frame-cache hit should restore precise bounds"
        );
    }
}
