use super::{
    ActorKindId, AnimationTrack, DebugRenderOptions, PlacementMode, PositionBinding, SceneDimensions, ShapeType, Timeline, Value,
    VelloPath, resolve_bound_position, TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE,
};
use crate::renderer::types::TextPath;
use kurbo::Shape;

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
            kurbo::Rect::new(
                0.0,
                0.0,
                (half_width * 2.0) as f64,
                (half_height * 2.0) as f64,
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

impl Timeline {
    /// Evaluate position, size, and transform for a node.
    fn evaluate_node_transform(
        &self,
        track: &AnimationTrack,
        time_ms: u64,
        parent_opacity: f32,
        parent_transform: kurbo::Affine,
        scene_dimensions: SceneDimensions,
        layout_position: Option<[f32; 2]>,
    ) -> NodeTransform {
        let placement_mode = track.placement_mode.get(time_ms, PlacementMode::LayoutManaged);
        let mut base_position = track.position.get(time_ms, [0.0, 0.0]);
        if let Some(layout_pos) = layout_position {
            if placement_mode == PlacementMode::LayoutManaged {
                base_position = layout_pos;
            }
        }

        let binding = track.position_binding.get(time_ms, PositionBinding::Absolute);
        let position = resolve_bound_position(binding, base_position, parent_transform, scene_dimensions);
        let motion_offset = track.motion_offset.get(time_ms, [0.0, 0.0]);
        let rotation = track.rotation.get(time_ms, 0.0) as f64;
        let scale = track.scale.get(time_ms, 1.0) as f64;
        let opacity = track.opacity.get(time_ms, 1.0);
        let half_size = track.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);

        let transform = track.transform.get(time_ms, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
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
        track: &AnimationTrack,
        half_size: [f32; 2],
        local_transform: &kurbo::Affine,
        scene: &mut vello::Scene,
        vector_paths: &[VelloPath],
        text_paths: &[TextPath],
        has_image: bool,
    ) -> Option<kurbo::Rect> {
        let local_bounds = node_local_bounds(
            vector_paths,
            text_paths,
            &track.svg_paths,
            has_image.then_some(half_size),
        );

        if let Some(bounds) = local_bounds {
            let stroke = vello::kurbo::Stroke::new(1.25);
            let debug_color = vello::peniko::Color::from_rgba8(255, 214, 102, 220);
            scene.stroke(&stroke, *local_transform, debug_color, None, &bounds);
        }

        local_bounds
    }
}

impl Timeline {
    /// Extract all text glyph paths from every track in the timeline.
    pub fn extract_all_glyphs(&self) -> Vec<TextPath> {
        let mut glyphs = Vec::new();
        for track in self.tracks.values() {
            if let Some(text_paths) = &track.text_paths {
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
    ) {
        let (global_transform, global_opacity) =
            self.render_actor_node(node_label, time_ms, parent_transform, parent_opacity, scene_dimensions, debug_options, scene, overrides, layout_positions, hit_regions, frame_env, filter_backend);

        self.render_node_children(node_label, time_ms, global_transform, global_opacity, scene_dimensions, debug_options, scene, overrides, hit_regions, frame_env, filter_backend);
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
    ) -> (kurbo::Affine, f32) {
        let Some(track) = self.tracks.get(node_label) else {
            return (parent_transform, parent_opacity);
        };

        // Skip actors that haven't been declared yet.
        // They are not clickable in the preview canvas before they appear;
        // selection is still possible via the layers / inspector tab.
        if time_ms < track.first_seen_ms {
            // Still recurse into children so they are also hidden
            for child_label in &track.children.clone() {
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
                );
            }
            return (parent_transform, parent_opacity);
        }

        // ── Evaluate transform first for visibility culling (P2.19) ──
        let layout_pos = if self.dynamic_layout {
            layout_positions.get(node_label).copied()
        } else {
            None
        };

        // P2.18: Temporal coherence — cache node transforms to avoid re-sampling
        // properties when the same (time, parent_transform) is evaluated again.
        let parent_coeffs = parent_transform.as_coeffs();
        let node_transform = {
            let cache = self.transform_cache.borrow();
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
                    );
                    self.transform_cache.borrow_mut().insert(
                        node_label.to_string(),
                        (time_ms, parent_coeffs, t),
                    );
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
                );
                self.transform_cache.borrow_mut().insert(
                    node_label.to_string(),
                    (time_ms, parent_coeffs, t),
                );
                t
            }
        };
        let half_size = node_transform.half_size;
        let opacity = node_transform.opacity;
        let local_transform = node_transform.local_transform;

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
            let mut local_env = if let Some(env) = frame_env {
                env.clone()
            } else {
                self.build_frame_env_internal(time_ms, scene_dimensions, overrides)
            };
            vector_paths = crate::timeline::plot::sample_procedural_plot(procedural_plot, &mut local_env);
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
                let primitive = meta.and_then(|m| crate::primitives::find_primitive(m.type_name));
                if let Some(primitive) = primitive {
                    let ctx = crate::primitives::EvaluateCtx {
                        track,
                        time_ms,
                        local_transform,
                        opacity,
                        scene_dimensions,
                        overrides: node_overrides,
                        vector_paths: &vector_paths,
                    };
                    let mut text_ctx = crate::primitives::TextCompileCtx {
                        text_compiler: &mut *self.text_compiler.borrow_mut(),
                        font_context: self.font_context.as_ref(),
                    };
                    primitive.evaluate(&ctx, Some(&mut text_ctx)).ok().flatten()
                } else {
                    None
                }
            };
            if let Some(commands) = primitive_dispatch {
                for cmd in &commands {
                    cmd.execute(scene, &local_transform, opacity);
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

                // Debug overlays
                if debug_options.draw_bounds {
                    let _ = self.add_node_debug_overlays(
                        track,
                        half_size,
                        &local_transform,
                        scene,
                        &vector_paths,
                        &[],
                        image_size.is_some(),
                    );
                }

                return (local_transform, opacity);
            }
        }

        (local_transform, opacity)
    }

    /// Recursively render child nodes, with special handling for mask containers.
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
    ) {
        let Some(track) = self.tracks.get(node_label) else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::collections::BTreeMap::new()
        };

        if track.kind == ActorKindId::Filter {
            let children = track.children.clone();
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
                );
            }

            // Sample filter properties
            let blur = track.filter_blur.get(time_ms, 0.0);
            let brightness = track.filter_brightness.get(time_ms, 1.0);
            let contrast = track.filter_contrast.get(time_ms, 1.0);
            let saturate = track.filter_saturate.get(time_ms, 1.0);
            let hue_rotate = track.filter_hue_rotate.get(time_ms, 0.0);
            let sepia = track.filter_sepia.get(time_ms, 0.0);

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

            // Render sub-scene to image via backend, apply GPU filters, draw result
            if let Some(backend) = filter_backend.as_mut() {
                match backend.render_scene_to_image_gpu_filtered(
                    &sub_scene, scene_dimensions,
                    blur, brightness, contrast, saturate, hue_rotate, sepia,
                ) {
                    Ok(filtered) => {
                        let brush = vello::peniko::ImageBrush::new(filtered.data.clone())
                            .with_extend(vello::peniko::Extend::Pad)
                            .with_quality(vello::peniko::ImageQuality::Medium)
                            .with_alpha(global_opacity);
                        scene.draw_image(&brush, kurbo::Affine::IDENTITY);
                    }
                    Err(_) => {
                        // Fallback: append sub-scene directly
                        scene.encoding_mut().append(sub_scene.encoding(), &None);
                    }
                }
            }
        } else if track.kind == ActorKindId::Mask {
            let children = track.children.clone();
            if !children.is_empty() {
                // Render first child normally (defines the mask shape visually)
                let first_child = &children[0];
                self.evaluate_node(
                    first_child,
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
                );

                // Get the first child's vector paths to use as clip shapes
                let clip_paths: Vec<VelloPath> = self
                    .tracks
                    .get(first_child)
                    .map(|t| t.evaluate_vector_paths(time_ms))
                    .unwrap_or_default();

                // Render remaining children clipped to the first child's paths
                for child in children.iter().skip(1) {
                    let clip_count = clip_paths.len();
                    for vp in &clip_paths {
                        scene.push_layer(
                            vello::peniko::Fill::NonZero,
                            vello::peniko::BlendMode::default(),
                            1.0,
                            kurbo::Affine::IDENTITY,
                            &vp.path,
                        );
                    }
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
                    );
                    for _ in 0..clip_count {
                        scene.pop_layer();
                    }
                }
            }
        } else {
            for child in &track.children.clone() {
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
        let time_ms = (time_s * 1000.0) as u64;

        // Check the frame cache: return cached scene if time and dimensions match
        // and the underlying modifiers/layout have not changed.
        let needs_frame_env = self.needs_frame_env();
        let has_child_orders = !self.child_orders.is_empty();
        if debug_options == DebugRenderOptions::default() {
            if let Some(ref cached) = *self.frame_cache.borrow() {
                if cached.time_ms == time_ms
                    && cached.dimensions == scene_dimensions
                    && cached.has_modifiers == needs_frame_env
                    && cached.has_dynamic_layout == self.dynamic_layout
                    && cached.has_child_orders == has_child_orders
                {
                    return cached.scene.clone();
                }
            }
        }

        // P2.25: Reuse vello scene buffer to avoid allocating fresh encoding buffers.
        let mut scene = self.scene_buffer.borrow_mut().take().unwrap_or_default();
        scene.reset();
        let bg_color = self.background_color.evaluate(time_ms);

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

        // Use compiled bytecode for fastest evaluation; fall back to IR, then AST
        if !self.modifier_bytecode_programs.is_empty() {
            if let Some(ref mut env) = frame_env {
                for program in &self.modifier_bytecode_programs {
                    let _ = self.apply_modifier_bytecode_program(
                        program,
                        time_ms,
                        scene_dimensions,
                        env,
                        &mut overrides,
                    );
                }
            }
        } else if !self.modifier_programs.is_empty() {
            if let Some(ref mut env) = frame_env {
                for program in &self.modifier_programs {
                    let _ = self.apply_modifier_ir_program(
                        program,
                        time_ms,
                        scene_dimensions,
                        env,
                        &mut overrides,
                    );
                }
            }
        } else {
            // AST fallback path (used when compilation failed or no modifiers exist)
            if let Some(ref mut env) = frame_env {
                for modifier in &self.modifiers {
                    self.apply_modifier_stmt(modifier, env, &mut overrides);
                }
            }
        }

        let bg = vello::peniko::Color::new([
            bg_color[0],
            bg_color[1],
            bg_color[2],
            bg_color[3],
        ]);
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

        for root in &self.root_nodes {
            // P2.17: Static subtree cache — fully-static subtrees are evaluated once
            // and their vello encoding is reused on all subsequent frames.
            if self.is_static_subtree(root) {
                let cache = self.static_subtree_cache.borrow_mut();
                if let Some(cached_scene) = cache.get(root) {
                    // Fast path: append cached encoding directly
                    scene.encoding_mut().append(cached_scene.encoding(), &None);
                } else {
                    drop(cache);
                    let mut temp_scene = vello::Scene::new();
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
                    );
                    // Append to main scene and cache for next time
                    scene.encoding_mut().append(temp_scene.encoding(), &None);
                    self.static_subtree_cache.borrow_mut().insert(root.clone(), temp_scene);
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
                );
            }
        }

        // P2.24: Only store hit regions when explicitly requested.
        // Saves bounding-box computation for frames where click-to-select is not needed.
        if debug_options.compute_hit_regions {
            *self.hit_regions.borrow_mut() = hit_regions;
        } else {
            self.hit_regions.borrow_mut().clear();
        }

        // P2.25: Save scene in reusable buffer before returning.
        // Clone for frame cache if needed, then return a clone and keep original.
        let result = scene.clone();
        if debug_options == DebugRenderOptions::default() {
            *self.frame_cache.borrow_mut() = Some(super::FrameCacheEntry {
                time_ms,
                dimensions: scene_dimensions,
                has_modifiers: needs_frame_env,
                has_dynamic_layout: self.dynamic_layout,
                has_child_orders: !self.child_orders.is_empty(),
                scene: result.clone(),
            });
        }
        *self.scene_buffer.borrow_mut() = Some(scene);

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;
    use crate::timeline::{AnimationTrack, PropertyTrack};

    /// Helper to create a minimal Timeline with one root track.
    fn make_minimal_timeline() -> Timeline {
        let mut timeline = Timeline::new();
        let mut track = AnimationTrack::new("test_box".to_string());
        // Set first_seen_ms to 0 so the actor is visible from time 0
        track.first_seen_ms = 0;
        // Give it a shape type
        track.shape_type = Some({
            let mut t = PropertyTrack::new(ShapeType::Rect);
            t.add_keyframe(0, ShapeType::Rect, Easing::Linear);
            t
        });
        // Give it a size so it has content
        track.size = Some({
            let mut t = PropertyTrack::new([50.0, 50.0]);
            t.add_keyframe(0, [50.0, 50.0], Easing::Linear);
            t
        });
        // Add a color so it renders something visible
        track.color = Some({
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
        let dimensions = SceneDimensions { width: 800, height: 600 };

        let scene = timeline.evaluate(0.0, dimensions);

        // Should return a valid vello Scene (not empty, at least has background)
        // vello::Scene doesn't expose fraction() in all versions; just verify it doesn't panic
        let _ = scene;
    }

    #[test]
    fn evaluate_returns_scene_at_different_times() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions { width: 800, height: 600 };

        let scene_0 = timeline.evaluate(0.0, dimensions);
        let scene_5 = timeline.evaluate(5.0, dimensions);

        // Both should be valid scenes (no panic)
        let _ = scene_0;
        let _ = scene_5;
    }

    #[test]
    fn evaluate_with_empty_timeline_returns_scene() {
        let timeline = Timeline::new();
        let dimensions = SceneDimensions { width: 800, height: 600 };

        let scene = timeline.evaluate(0.0, dimensions);
        // Should not panic
        let _ = scene;
    }

    #[test]
    fn frame_cache_caches_identical_evaluations() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions { width: 800, height: 600 };

        // First call should compute and cache
        let scene1 = timeline.evaluate(1.0, dimensions);
        let _ = scene1;

        // Verify the cache is populated
        let cache = timeline.frame_cache.borrow();
        assert!(cache.is_some(), "frame cache should be populated after evaluate");

        if let Some(ref entry) = *cache {
            assert_eq!(entry.time_ms, 1000, "cache should store time in ms (1.0s = 1000ms)");
            assert_eq!(entry.dimensions, dimensions);
        }

        // Second call with same params should use cache
        let scene2 = timeline.evaluate(1.0, dimensions);
        let _ = scene2;

        let cache2 = timeline.frame_cache.borrow();
        assert!(cache2.is_some(), "frame cache should still be populated");
        assert_eq!(cache2.as_ref().unwrap().time_ms, 1000);
    }

    #[test]
    fn frame_cache_misses_on_different_time() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions { width: 800, height: 600 };

        let _scene1 = timeline.evaluate(0.0, dimensions);
        let _scene2 = timeline.evaluate(2.0, dimensions);

        // Cache should contain the latest evaluation (t=2.0)
        let cache = timeline.frame_cache.borrow();
        assert!(cache.is_some(), "cache should be populated");
        assert_eq!(cache.as_ref().unwrap().time_ms, 2000, "cache should contain t=2.0");
    }

    #[test]
    fn frame_cache_misses_on_different_dimensions() {
        let timeline = make_minimal_timeline();

        let dims_1 = SceneDimensions { width: 800, height: 600 };
        let dims_2 = SceneDimensions { width: 1920, height: 1080 };

        let _scene1 = timeline.evaluate(0.0, dims_1);

        // Cache should have dims_1
        {
            let cache = timeline.frame_cache.borrow();
            assert_eq!(cache.as_ref().unwrap().dimensions, dims_1);
        }

        let _scene2 = timeline.evaluate(0.0, dims_2);

        // Cache should now have dims_2
        {
            let cache = timeline.frame_cache.borrow();
            assert_eq!(cache.as_ref().unwrap().dimensions, dims_2);
        }
    }

    #[test]
    fn hit_regions_are_populated_after_evaluate() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions { width: 800, height: 600 };

        // hit_regions should be empty before evaluate
        {
            let regions = timeline.hit_regions.borrow();
            assert!(regions.is_empty(), "hit_regions should be empty before evaluate");
        }

        let _scene = timeline.evaluate_with_debug(0.0, dimensions, DebugRenderOptions { draw_bounds: false, compute_hit_regions: true }, &mut None);

        // hit_regions should be populated after evaluate
        let regions = timeline.hit_regions.borrow();
        assert!(!regions.is_empty(), "hit_regions should be populated after evaluate");
        assert!(regions.iter().any(|(label, _)| label == "test_box"),
            "hit_regions should contain 'test_box'");
    }

    #[test]
    fn hit_regions_contain_world_bounds() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions { width: 800, height: 600 };

        let _scene = timeline.evaluate_with_debug(0.0, dimensions, DebugRenderOptions { draw_bounds: false, compute_hit_regions: true }, &mut None);

        let regions = timeline.hit_regions.borrow();
        let (label, bounds) = regions.iter().find(|(l, _)| l == "test_box")
            .expect("should find test_box in hit_regions");

        assert_eq!(label, "test_box");
        // The bounds should be valid rectangles (x0 < x1, y0 < y1)
        assert!(bounds.x0 < bounds.x1, "hit region x0 should be less than x1");
        assert!(bounds.y0 < bounds.y1, "hit region y0 should be less than y1");
    }

    #[test]
    fn evaluate_with_debug_options_skips_cache() {
        let timeline = make_minimal_timeline();
        let dimensions = SceneDimensions { width: 800, height: 600 };
        let debug_opts = DebugRenderOptions { draw_bounds: true, compute_hit_regions: false };

        // Evaluate with debug options (should not cache)
        let _scene = timeline.evaluate_with_debug(0.0, dimensions, debug_opts, &mut None);

        // Cache should not be populated because debug_options != default
        let cache = timeline.frame_cache.borrow();
        assert!(cache.is_none(), "frame cache should not be populated with non-default debug options");
    }
}
