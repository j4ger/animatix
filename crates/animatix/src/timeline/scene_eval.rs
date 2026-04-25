use super::{
    DebugRenderOptions, PlacementMode, SceneDimensions, Timeline, Value, VectorShapeState,
    VectorShapeStyle, VelloPath, build_vector_shape_vello_path, resolve_bound_position,
    vector_shape_uses_custom_path,
};
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

impl Timeline {
    pub fn extract_all_glyphs(&self) -> Vec<TextPath> {
        let mut glyphs = Vec::new();
        for track in self.tracks.values() {
            for (_, (paths, _)) in &track.text_paths.keyframes {
                for glyph in paths {
                    glyphs.push(glyph.clone());
                }
            }
            for glyph in &track.text_paths.default_value {
                glyphs.push(glyph.clone());
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
    ) {
        let (global_transform, global_opacity) = if let Some(track) = self.tracks.get(node_label) {
            // Skip actors that haven't been declared yet
            if time_ms < track.first_seen_ms {
                // Still recurse into children so they are also hidden
                if let Some(node) = self.nodes.get(node_label) {
                    for child_label in &node.children {
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
                        );
                    }
                }
                return;
            }

            let placement_mode = track.placement_mode.evaluate(time_ms);
            let mut base_position = track.position.evaluate(time_ms);

            // If dynamic layout is enabled and this node has a computed layout position
            if self.dynamic_layout {
                if let Some(layout_pos) = layout_positions.get(node_label) {
                    if placement_mode == PlacementMode::LayoutManaged {
                        base_position = *layout_pos;
                    }
                }
            }
            let binding = track.position_binding.evaluate(time_ms);
            let mut position =
                resolve_bound_position(binding, base_position, parent_transform, scene_dimensions);
            let motion_offset = track.motion_offset.evaluate(time_ms);
            let mut rotation = track.rotation.evaluate(time_ms) as f64;
            let mut scale = track.scale.evaluate(time_ms) as f64;
            let mut opacity = track.opacity.evaluate(time_ms);
            let text_paths = track.evaluate_text_paths(time_ms);
            let points = track.points.evaluate(time_ms);
            let shape_type = track.shape_type.evaluate(time_ms);
            let mut vector_paths = track.evaluate_vector_paths(time_ms);
            let mut half_size = track.size.evaluate(time_ms);
            let mut line_from = track.line_from.evaluate(time_ms);
            let mut line_to = track.line_to.evaluate(time_ms);
            let mut arc_angles = track.arc_angles.evaluate(time_ms);
            let mut color = track.color.evaluate(time_ms);
            let mut stroke_width = track.stroke_width.evaluate(time_ms);
            let mut stroke_color = track.stroke_color.evaluate(time_ms);
            let mut fill_opacity = track.fill_opacity.evaluate(time_ms);

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

            if !vector_paths.is_empty() {
                vector_paths = if matches!(shape_type, super::SHAPE_GRAPH | super::SHAPE_PLOT) {
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
            let image = track.image.evaluate(time_ms);
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

            (local_transform, local_opacity)
        } else {
            (parent_transform, parent_opacity)
        };

        if let Some(node) = self.nodes.get(node_label) {
            // Compute dynamic layout for this container's children
            let child_layout_positions = if self.dynamic_layout {
                if let Some(metadata) = self.container_metadata.get(node_label) {
                    self.layout_engine.compute_layout_for_time(
                        node_label,
                        metadata,
                        time_ms,
                        &self.tracks,
                        &self.nodes,
                    )
                } else {
                    std::collections::BTreeMap::new()
                }
            } else {
                std::collections::BTreeMap::new()
            };

            for child in &node.children {
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
        let mut scene = vello::Scene::new();
        let bg_color = self.background_color.evaluate(time_ms);

        let mut overrides: std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        > = std::collections::HashMap::new();
        let mut frame_env = self.frame_eval_env(time_ms, scene_dimensions, &overrides);

        for modifier in &self.modifiers {
            self.apply_modifier_stmt(
                modifier,
                time_ms,
                scene_dimensions,
                &mut frame_env,
                &mut overrides,
            );
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
            );
        }

        scene
    }
}
