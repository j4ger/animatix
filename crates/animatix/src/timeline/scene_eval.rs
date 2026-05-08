use super::{
    DebugRenderOptions, PlacementMode, PositionBinding, SceneDimensions, ShapeType, Timeline, Value, VectorShapeState,
    VectorShapeStyle, VelloPath, build_vector_shape_vello_path, resolve_bound_position,
    vector_shape_uses_custom_path, TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE,
};
use crate::renderer::text::TextKind;
use crate::renderer::types::TextPath;
use kurbo::Shape;

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
    pub fn extract_all_glyphs(&self) -> Vec<TextPath> {
        let mut glyphs = Vec::new();
        for track in self.tracks.values() {
            if let Some(text_paths) = &track.text_paths {
                for (_, (paths, _)) in &text_paths.keyframes {
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
    ) {
        let (global_transform, global_opacity) = if let Some(track) = self.tracks.get(node_label) {
            // Skip actors that haven't been declared yet
            if time_ms < track.first_seen_ms {
                // Still compute hit region so the actor can be selected in the editor
                let half_size = track.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);
                let base_position = track.position.get(time_ms, [0.0, 0.0]);
                let binding = track.position_binding.get(time_ms, PositionBinding::Absolute);
                let position =
                    resolve_bound_position(binding, base_position, parent_transform, scene_dimensions);
                let rotation = track.rotation.get(time_ms, 0.0) as f64;
                let scale = track.scale.get(time_ms, 1.0) as f64;
                let local_transform = parent_transform
                    * kurbo::Affine::translate(kurbo::Vec2::new(
                        position[0] as f64,
                        position[1] as f64,
                    ))
                    * kurbo::Affine::rotate(rotation)
                    * kurbo::Affine::scale(scale);
                let default_bounds = kurbo::Rect::new(
                    (-half_size[0]) as f64,
                    (-half_size[1]) as f64,
                    half_size[0] as f64,
                    half_size[1] as f64,
                );
                let world_bounds = transform_rect_bbox(&local_transform, default_bounds);
                hit_regions.push((node_label.to_string(), world_bounds));

                // Still recurse into children so they are also hidden
                if let Some(track) = self.tracks.get(node_label) {
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
                        );
                    }
                }
                return;
            }

            let placement_mode = track.placement_mode.get(time_ms, PlacementMode::LayoutManaged);
            let mut base_position = track.position.get(time_ms, [0.0, 0.0]);

            // If dynamic layout is enabled and this node has a computed layout position
            if self.dynamic_layout {
                if let Some(layout_pos) = layout_positions.get(node_label) {
                    if placement_mode == PlacementMode::LayoutManaged {
                        base_position = *layout_pos;
                    }
                }
            }
            let binding = track.position_binding.get(time_ms, PositionBinding::Absolute);
            let mut position =
                resolve_bound_position(binding, base_position, parent_transform, scene_dimensions);
            let motion_offset = track.motion_offset.get(time_ms, [0.0, 0.0]);
            let mut rotation = track.rotation.get(time_ms, 0.0) as f64;
            let mut scale = track.scale.get(time_ms, 1.0) as f64;
            let mut opacity = track.opacity.get(time_ms, 1.0);

            let points = track.points.get(time_ms, Vec::new());
            let shape_type = track.shape_type.get(time_ms, ShapeType::Rect);
            let mut vector_paths = track.evaluate_vector_paths(time_ms);
            let mut half_size = track.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);
            let mut line_from = track.line_from.get(time_ms, [-50.0, 0.0]);
            let mut line_to = track.line_to.get(time_ms, [50.0, 0.0]);
            let mut arc_angles = track.arc_angles.get(time_ms, [0.0, std::f32::consts::PI]);
            let mut color = track.color.get(time_ms, [1.0, 1.0, 1.0, 1.0]);
            let mut stroke_width = track.stroke_width.get(time_ms, 2.0);
            let mut stroke_color = track.stroke_color.get(time_ms, [1.0, 1.0, 1.0, 1.0]);
            let mut fill_opacity = track.fill_opacity.get(time_ms, 1.0);

            if let Some(node_overrides) = overrides.get(node_label) {
                if let Some(Value::Vec2(pos)) = node_overrides.get("at") {
                    position = [pos[0] as f32, pos[1] as f32];
                }
                if let Some(Value::Num(op)) = node_overrides.get("opacity") {
                    opacity = *op as f32;
                }
                if let Some(Value::Vec2(size)) = node_overrides.get("size") {
                    half_size = [size[0] as f32 / 2.0, size[1] as f32 / 2.0];
                }
                if let Some(Value::Num(tip_length)) = node_overrides.get("tip_length") {
                    half_size[0] = *tip_length as f32;
                }
                if let Some(Value::Num(tip_width)) = node_overrides.get("tip_width") {
                    half_size[1] = *tip_width as f32;
                }
                if let Some(Value::Num(radius)) = node_overrides.get("radius") {
                    half_size = [*radius as f32, *radius as f32];
                }
                if let Some(Value::Num(radius_x)) = node_overrides.get("radius_x") {
                    half_size[0] = *radius_x as f32;
                }
                if let Some(Value::Num(radius_y)) = node_overrides.get("radius_y") {
                    half_size[1] = *radius_y as f32;
                }
                if let Some(Value::Vec2(from)) = node_overrides.get("from") {
                    line_from = [from[0] as f32, from[1] as f32];
                }
                if let Some(Value::Vec2(to)) = node_overrides.get("to") {
                    line_to = [to[0] as f32, to[1] as f32];
                }
                if let Some(Value::Num(start_angle)) = node_overrides.get("start_angle") {
                    arc_angles[0] = *start_angle as f32;
                }
                if let Some(Value::Num(sweep_angle)) = node_overrides.get("sweep_angle") {
                    arc_angles[1] = *sweep_angle as f32;
                }
                if let Some(Value::Color(c) | Value::Vec4(c)) = node_overrides.get("color") {
                    color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                }
                if let Some(Value::Color(c) | Value::Vec4(c)) = node_overrides
                    .get("stroke_color")
                    .or_else(|| node_overrides.get("stroke"))
                {
                    stroke_color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                }
                if let Some(Value::Num(width)) = node_overrides
                    .get("stroke_width")
                    .or_else(|| node_overrides.get("width"))
                {
                    stroke_width = *width as f32;
                }
                if let Some(Value::Num(opacity)) = node_overrides.get("fill_opacity") {
                    fill_opacity = *opacity as f32;
                }
                if let Some(Value::Num(angle)) = node_overrides.get("rotation") {
                    rotation = *angle;
                }
                if let Some(Value::Num(factor)) = node_overrides.get("scale") {
                    scale = *factor;
                }
            }

            // ── Runtime text recompilation (Phase 2) ──
            // If text content has changed since build time (e.g. via `always` blocks),
            // recompile glyph paths on-demand using the TextCompiler cache.
            let text_paths = {
                let node_overrides = overrides.get(node_label);
                let mut content = track.text_content.get(time_ms, String::new());
                let mut font_family = track.font_family.get(time_ms, String::new());
                let default_font_size = match track.kind {
                    super::ActorKindId::Code => 24.0,
                    _ => 48.0,
                };
                let mut font_size = track.font_size.get(time_ms, default_font_size);
                let mut color = track.color.get(time_ms, [1.0, 1.0, 1.0, 1.0]);

                // Apply overrides from always/reactive blocks
                if let Some(ov) = node_overrides {
                    if let Some(Value::Str(s)) = ov.get("text").or_else(|| ov.get("code")).or_else(|| ov.get("math")).or_else(|| ov.get("latex")) {
                        content = s.clone();
                    }
                    if let Some(Value::Str(s)) = ov.get("font_family") {
                        font_family = s.clone();
                    }
                    if let Some(Value::Num(n)) = ov.get("font_size") {
                        font_size = *n as f32;
                    }
                    if let Some(Value::Color(c) | Value::Vec4(c)) = ov.get("color") {
                        color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                    }
                }

                if !content.is_empty() {
                    let kind = match track.kind {
                        super::ActorKindId::Text => TextKind::Text,
                        super::ActorKindId::Math => TextKind::Math,
                        super::ActorKindId::Code => TextKind::Code,
                        _ => TextKind::Text,
                    };
                    self.text_compiler.borrow_mut().compile(
                        &content,
                        &font_family,
                        font_size,
                        color,
                        kind,
                    )
                } else {
                    track.evaluate_text_paths(time_ms)
                }
            };

            if !vector_paths.is_empty() {
                vector_paths = if matches!(shape_type, ShapeType::Graph | ShapeType::Plot) {
                    vector_paths
                } else {
                    let mut vector_shape_state =
                        VectorShapeState::new(half_size, line_from, line_to, arc_angles);
                    vector_shape_state.rotation = rotation as f32;
                    vector_shape_state.points = points.clone();
                    if vector_shape_uses_custom_path(shape_type) {
                        vector_shape_state.custom_path =
                            vector_paths.first().map(|vp| vp.path.clone());
                    }
                    build_vector_shape_vello_path(
                        shape_type,
                        &vector_shape_state,
                        VectorShapeStyle {
                            color,
                            stroke_width,
                            stroke_color,
                            fill_opacity,
                        },
                    )
                    .map(|path| vec![path])
                    .unwrap_or(vector_paths)
                };
            }

            let local_opacity = opacity * parent_opacity;
            let local_transform = parent_transform
                * kurbo::Affine::translate((
                    position[0] as f64 + motion_offset[0] as f64,
                    position[1] as f64 + motion_offset[1] as f64,
                ))
                * kurbo::Affine::rotate(rotation)
                * kurbo::Affine::scale(scale);
            let image = track.image.get(time_ms, None);
            let has_image = image.is_some();

            for vector_path in &vector_paths {
                let transform = local_transform;
                if let Some(mut fill_color) = vector_path.fill {
                    if local_opacity < 1.0 {
                        fill_color =
                            fill_color.with_alpha(fill_color.components[3] * local_opacity);
                    }
                    scene.fill(
                        vello::peniko::Fill::NonZero,
                        transform,
                        fill_color,
                        None,
                        &vector_path.path,
                    );
                }

                if let Some((mut stroke_color, stroke_width)) = vector_path.stroke {
                    if local_opacity < 1.0 {
                        stroke_color =
                            stroke_color.with_alpha(stroke_color.components[3] * local_opacity);
                    }
                    let stroke = vello::kurbo::Stroke::new(stroke_width as f64);
                    scene.stroke(&stroke, transform, stroke_color, None, &vector_path.path);
                }
            }

            for text_path in &text_paths {
                let color = match &text_path.color {
                    typst::visualize::Paint::Solid(color) => {
                        let rgba = color.to_vec4_u8();
                        vello::peniko::Color::from_rgba8(
                            rgba[0],
                            rgba[1],
                            rgba[2],
                            (rgba[3] as f32 * local_opacity) as u8,
                        )
                    }
                    _ => vello::peniko::Color::WHITE,
                };

                scene.fill(
                    vello::peniko::Fill::NonZero,
                    local_transform,
                    color,
                    None,
                    &text_path.path,
                );
            }

            for svg_path in &track.svg_paths {
                let transform = local_transform;
                if let Some(mut fill_color) = svg_path.fill {
                    if local_opacity < 1.0 {
                        fill_color =
                            fill_color.with_alpha(fill_color.components[3] * local_opacity);
                    }
                    scene.fill(
                        vello::peniko::Fill::NonZero,
                        transform,
                        fill_color,
                        None,
                        &svg_path.path,
                    );
                }

                if let Some((mut stroke_color, stroke_width)) = svg_path.stroke {
                    if local_opacity < 1.0 {
                        stroke_color =
                            stroke_color.with_alpha(stroke_color.components[3] * local_opacity);
                    }
                    let stroke = vello::kurbo::Stroke::new(stroke_width as f64);
                    scene.stroke(&stroke, transform, stroke_color, None, &svg_path.path);
                }
            }

            if let Some(image) = image {
                let [natural_width, natural_height] = image.natural_size;
                let display_width = half_size[0] * 2.0;
                let display_height = half_size[1] * 2.0;
                let image_transform = local_transform
                    * kurbo::Affine::scale_non_uniform(
                        (display_width / natural_width) as f64,
                        (display_height / natural_height) as f64,
                    );

                let brush = vello::peniko::ImageBrush::new(image.data.clone())
                    .with_extend(vello::peniko::Extend::Pad)
                    .with_quality(vello::peniko::ImageQuality::Medium)
                    .with_alpha(local_opacity);

                scene.draw_image(&brush, image_transform);
            }

            if debug_options.draw_bounds
                && let Some(local_bounds) = node_local_bounds(
                    &vector_paths,
                    &text_paths,
                    &track.svg_paths,
                    has_image.then_some(half_size),
                )
            {
                let stroke = vello::kurbo::Stroke::new(1.25);
                let debug_color = vello::peniko::Color::from_rgba8(255, 214, 102, 220);
                scene.stroke(&stroke, local_transform, debug_color, None, &local_bounds);
            }

            // Collect world-space hit region for click-to-select
            let lb = node_local_bounds(
                &vector_paths,
                &text_paths,
                &track.svg_paths,
                has_image.then_some(half_size),
            );
            let world_bounds = if let Some(local_bounds) = lb {
                transform_rect_bbox(&local_transform, local_bounds)
            } else {
                // Fall back to half-size based bounds
                let default_bounds = kurbo::Rect::new(
                    (-half_size[0]) as f64,
                    (-half_size[1]) as f64,
                    half_size[0] as f64,
                    half_size[1] as f64,
                );
                transform_rect_bbox(&local_transform, default_bounds)
            };
            hit_regions.push((node_label.to_string(), world_bounds));

            (local_transform, local_opacity)
        } else {
            (parent_transform, parent_opacity)
        };

        if let Some(track) = self.tracks.get(node_label) {
            // Compute dynamic layout for this container's children
            let child_layout_positions = if self.dynamic_layout {
                self.compute_animated_layout(node_label, time_ms)
            } else {
                std::collections::BTreeMap::new()
            };

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
                );
            }
        }
    }

    pub fn evaluate(&self, time_s: f64, scene_dimensions: SceneDimensions) -> vello::Scene {
        self.evaluate_with_debug(time_s, scene_dimensions, DebugRenderOptions::default())
    }

    pub fn evaluate_with_debug(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> vello::Scene {
        let time_ms = (time_s * 1000.0) as u64;

        // Check the frame cache: return cached scene if time and dimensions match
        // and the underlying modifiers/layout have not changed.
        let has_modifiers = !self.modifier_programs.is_empty() || !self.modifiers.is_empty();
        if debug_options == DebugRenderOptions::default() {
            if let Some(ref cached) = *self.frame_cache.borrow() {
                if cached.time_ms == time_ms
                    && cached.dimensions == scene_dimensions
                    && cached.has_modifiers == has_modifiers
                    && cached.has_dynamic_layout == self.dynamic_layout
                    && cached.has_child_orders == !self.child_orders.is_empty()
                {
                    return cached.scene.clone();
                }
            }
        }

        let mut scene = vello::Scene::new();
        let bg_color = self.background_color.evaluate(time_ms);

        // Collect actor world-space bounding boxes for click-to-select
        let mut hit_regions: Vec<(String, kurbo::Rect)> = Vec::new();

        let mut overrides: std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        > = std::collections::HashMap::new();
        let mut frame_env = self.frame_eval_env(time_ms, scene_dimensions, &overrides);

        // Use compiled IR programs for fast evaluation; fall back to AST if no programs exist
        if !self.modifier_programs.is_empty() {
            for program in &self.modifier_programs {
                let _ = self.apply_modifier_ir_program(
                    program,
                    time_ms,
                    scene_dimensions,
                    &mut frame_env,
                    &mut overrides,
                );
            }
        } else {
            // AST fallback path (used when IR compilation failed or no modifiers exist)
            for modifier in &self.modifiers {
                self.apply_modifier_stmt(
                    modifier,
                    time_ms,
                    scene_dimensions,
                    &mut frame_env,
                    &mut overrides,
                );
            }
        }

        let bg = vello::peniko::Color::new([
            bg_color[0] as f32,
            bg_color[1] as f32,
            bg_color[2] as f32,
            bg_color[3] as f32,
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
            );
        }

        // Store hit regions for click-to-select
        *self.hit_regions.borrow_mut() = hit_regions;

        // Store result in frame cache for fast lookup on next identical evaluation request
        if debug_options == DebugRenderOptions::default() {
            *self.frame_cache.borrow_mut() = Some(super::FrameCacheEntry {
                time_ms,
                dimensions: scene_dimensions,
                has_modifiers,
                has_dynamic_layout: self.dynamic_layout,
                has_child_orders: !self.child_orders.is_empty(),
                scene: scene.clone(),
            });
        }

        scene
    }
}
