use super::{
    AnimationTrack, DebugRenderOptions, PlacementMode, PositionBinding, SceneDimensions, ShapeType, Timeline, Value, VectorShapeState,
    VectorShapeStyle, VelloPath, build_vector_shape_vello_path, resolve_bound_position,
    vector_shape_uses_custom_path, TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE,
};
use crate::renderer::text::TextKind;
use crate::renderer::types::TextPath;
use kurbo::Shape;

#[derive(Clone, Copy)]
struct NodeTransform {
    position: [f32; 2],
    half_size: [f32; 2],
    opacity: f32,
    rotation: f64,
    scale: f64,
    motion_offset: [f32; 2],
    local_transform: kurbo::Affine,
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

        let local_transform = parent_transform
            * kurbo::Affine::translate((
                position[0] as f64 + motion_offset[0] as f64,
                position[1] as f64 + motion_offset[1] as f64,
            ))
            * kurbo::Affine::rotate(rotation)
            * kurbo::Affine::scale(scale);

        NodeTransform {
            position,
            half_size,
            opacity: opacity * parent_opacity,
            rotation,
            scale,
            motion_offset,
            local_transform,
        }
    }

    /// Build vector paths for a shape actor.
    fn build_shape_vector_paths(
        &self,
        track: &AnimationTrack,
        time_ms: u64,
        half_size: [f32; 2],
        _line_from: [f32; 2],
        _line_to: [f32; 2],
        _arc_angles: [f32; 2],
        color: [f32; 4],
        stroke_width: f32,
        stroke_color: [f32; 4],
        fill_opacity: f32,
        shape_type: ShapeType,
        vector_paths: &mut Vec<VelloPath>,
    ) {
        if vector_paths.is_empty() || matches!(shape_type, ShapeType::Graph | ShapeType::Plot) {
            return;
        }

        let mut vector_shape_state = VectorShapeState::new(shape_type, half_size);
        match &mut vector_shape_state {
            VectorShapeState::Line(line_state) => {
                line_state.line_from = track.line_from.get(time_ms, [-50.0, 0.0]);
                line_state.line_to = track.line_to.get(time_ms, [50.0, 0.0]);
            }
            VectorShapeState::Ellipse(ellipse) => {
                ellipse.arc_angles = track.arc_angles.get(time_ms, [0.0, 0.0]);
                let rot = track.rotation.get(time_ms, 0.0);
                if rot != 0.0 {
                    ellipse.rotation = rot;
                }
            }
            VectorShapeState::Polygon(poly) => {
                poly.points = track.points.get(time_ms, Vec::new());
                let rot = track.rotation.get(time_ms, 0.0);
                if rot != 0.0 {
                    poly.rotation = rot;
                }
            }
            _ => {}
        }
        if vector_shape_uses_custom_path(shape_type) {
            if let Some(first_path) = vector_paths.first().map(|vp| vp.path.clone()) {
                match &mut vector_shape_state {
                    VectorShapeState::Polygon(poly) => poly.custom_path = Some(first_path),
                    VectorShapeState::Path(path_state) => path_state.custom_path = Some(first_path),
                    _ => {}
                }
            }
        }
        if let Some(path) = build_vector_shape_vello_path(
            shape_type,
            &vector_shape_state,
            VectorShapeStyle {
                color,
                stroke_width,
                stroke_color,
                fill_opacity,
            },
        ) {
            *vector_paths = vec![path];
        }
    }

    fn evaluate_text_node(
        &self,
        track: &AnimationTrack,
        time_ms: u64,
        node_overrides: Option<&std::collections::HashMap<String, Value>>,
        _local_transform: &kurbo::Affine,
        _opacity: f32,
        _scene: &mut vello::Scene,
        _diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
    ) -> Vec<TextPath> {
        let mut content = track.text_content.get(time_ms, String::new());
        let mut font_family = track.font_family.get(time_ms, String::new());
        let default_font_size = match track.kind {
            super::ActorKindId::Code => 24.0,
            _ => 48.0,
        };
        let mut font_size = track.font_size.get(time_ms, default_font_size);
        let mut color = track.color.get(time_ms, DEFAULT_WHITE);

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
            self.text_compiler
                .borrow_mut()
                .compile(&content, &font_family, font_size, color, kind, &self.font_context)
        } else {
            track.evaluate_text_paths(time_ms)
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
            // Skip actors that haven't been declared yet.
            // They are not clickable in the preview canvas before they appear;
            // selection is still possible via the layers / inspector tab.
            if time_ms < track.first_seen_ms {
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

            let shape_type = track.shape_type.get(time_ms, ShapeType::Rect);
            let mut vector_paths = track.evaluate_vector_paths(time_ms);

            // Re-sample procedural plots at frame time so they can reference `t`.
            if let Some(procedural_plot) = track.procedural_plot.as_ref() {
                let frame_env = self.frame_eval_env(time_ms, scene_dimensions, &overrides);
                vector_paths = crate::timeline::plot::sample_procedural_plot(procedural_plot, &frame_env);
            }
            let layout_pos = if self.dynamic_layout {
                layout_positions.get(node_label).copied()
            } else {
                None
            };
            let node_overrides = overrides.get(node_label);
            let node_transform = self.evaluate_node_transform(
                track,
                time_ms,
                parent_opacity,
                parent_transform,
                scene_dimensions,
                layout_pos,
            );
            let _position = node_transform.position;
            let half_size = node_transform.half_size;
            let opacity = node_transform.opacity;
            let _rotation = node_transform.rotation;
            let _scale = node_transform.scale;
            let _motion_offset = node_transform.motion_offset;
            let local_transform = node_transform.local_transform;

            let _points = track.points.get(time_ms, Vec::new());
            let mut line_from = track.line_from.get(time_ms, [-50.0, 0.0]);
            let mut line_to = track.line_to.get(time_ms, [50.0, 0.0]);
            let arc_angles = track.arc_angles.get(time_ms, [0.0, std::f32::consts::PI]);
            let mut color = track.color.get(time_ms, DEFAULT_WHITE);
            let mut stroke_width = track.stroke_width.get(time_ms, 2.0);
            let mut stroke_color = track.stroke_color.get(time_ms, DEFAULT_WHITE);
            let mut fill_opacity = track.fill_opacity.get(time_ms, 1.0);

            // ── Effects properties ──
            let mut shadow_offset = track.shadow_offset.get(time_ms, [0.0, 0.0]);
            let mut shadow_blur = track.shadow_blur.get(time_ms, 0.0);
            let mut shadow_color_val = track.shadow_color.get(time_ms, [0.0, 0.0, 0.0, 0.0]);
            let mut glow_radius = track.glow_radius.get(time_ms, 0.0);
            let mut glow_color_val = track.glow_color.get(time_ms, [0.0, 0.0, 0.0, 0.0]);

            if let Some(node_overrides) = node_overrides {
                if let Some(Value::Vec2(from)) = node_overrides.get("from") {
                    line_from = [from[0] as f32, from[1] as f32];
                }
                if let Some(Value::Vec2(to)) = node_overrides.get("to") {
                    line_to = [to[0] as f32, to[1] as f32];
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

                // Effects overrides
                if let Some(Value::Vec2(off)) = node_overrides.get("shadow_offset") {
                    shadow_offset = [off[0] as f32, off[1] as f32];
                }
                if let Some(Value::Num(blur)) = node_overrides.get("shadow_blur") {
                    shadow_blur = *blur as f32;
                }
                if let Some(Value::Color(c) | Value::Vec4(c)) = node_overrides.get("shadow_color") {
                    shadow_color_val = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                }
                if let Some(Value::Num(radius)) = node_overrides.get("glow_radius") {
                    glow_radius = *radius as f32;
                }
                if let Some(Value::Color(c) | Value::Vec4(c)) = node_overrides.get("glow_color") {
                    glow_color_val = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                }
            }

            // ── Runtime text recompilation (Phase 2) ──
            // If text content has changed since build time (e.g. via `always` blocks),
            // recompile glyph paths on-demand using the TextCompiler cache.
            let text_paths = self.evaluate_text_node(
                track,
                time_ms,
                node_overrides,
                &local_transform,
                opacity,
                scene,
                &mut Vec::new(),
            );

            self.build_shape_vector_paths(
                track,
                time_ms,
                half_size,
                line_from,
                line_to,
                arc_angles,
                color,
                stroke_width,
                stroke_color,
                fill_opacity,
                shape_type,
                &mut vector_paths,
            );

            let local_opacity = opacity;
            let image = track.image.get(time_ms, None);
            let has_image = image.is_some();

            // ── Drop shadow rendering ──
            // Draw a semi-transparent copy of each vector path offset by shadow_offset
            if shadow_color_val[3] > 0.0 || shadow_blur > 0.0 {
                let shadow_transform = local_transform
                    * kurbo::Affine::translate((shadow_offset[0] as f64, shadow_offset[1] as f64));
                let shadow_alpha = (shadow_color_val[3] * 0.5).clamp(0.0, 1.0);
                let mut sc = vello::peniko::Color::from_rgba8(
                    (shadow_color_val[0] * 255.0) as u8,
                    (shadow_color_val[1] * 255.0) as u8,
                    (shadow_color_val[2] * 255.0) as u8,
                    (shadow_alpha * 255.0) as u8,
                );
                if local_opacity < 1.0 {
                    sc = sc.with_alpha(sc.components[3] * local_opacity);
                }
                for vector_path in &vector_paths {
                    // Draw shadow fill
                    scene.fill(
                        vello::peniko::Fill::NonZero,
                        shadow_transform,
                        sc,
                        None,
                        &vector_path.path,
                    );
                    // Draw shadow stroke if the path has one
                    if let Some((_, stroke_width)) = vector_path.stroke {
                        let stroke = vello::kurbo::Stroke::new(stroke_width as f64);
                        scene.stroke(&stroke, shadow_transform, sc, None, &vector_path.path);
                    }
                }
            }

            // ── Glow rendering ──
            // Draw a semi-transparent expanded copy of each vector path
            if glow_color_val[3] > 0.0 && glow_radius > 0.0 {
                let glow_alpha = (glow_color_val[3] * 0.4).clamp(0.0, 1.0);
                let glow_expand = glow_radius as f64;
                let mut gc = vello::peniko::Color::from_rgba8(
                    (glow_color_val[0] * 255.0) as u8,
                    (glow_color_val[1] * 255.0) as u8,
                    (glow_color_val[2] * 255.0) as u8,
                    (glow_alpha * 255.0) as u8,
                );
                if local_opacity < 1.0 {
                    gc = gc.with_alpha(gc.components[3] * local_opacity);
                }
                for vector_path in &vector_paths {
                    // Use a thick stroke to approximate glow expansion around the path
                    let glow_stroke = vello::kurbo::Stroke::new(glow_expand * 2.0);
                    scene.stroke(&glow_stroke, local_transform, gc, None, &vector_path.path);
                }
            }

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
                            (rgba[3] as f32 * local_opacity * text_path.opacity) as u8,
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

            if debug_options.draw_bounds {
                let _ = self.add_node_debug_overlays(
                    track,
                    half_size,
                    &local_transform,
                    scene,
                    &vector_paths,
                    &text_paths,
                    has_image,
                );
            }

            // Collect world-space hit region for click-to-select
            let lb = node_local_bounds(&vector_paths, &text_paths, &track.svg_paths, has_image.then_some(half_size));
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
