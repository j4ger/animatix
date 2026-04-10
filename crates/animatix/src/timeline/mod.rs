pub mod actions;
pub mod env;
pub mod kurbo_shapes;
pub mod morph;
pub mod svg;
pub mod track;
pub mod utils;
pub mod vello_path;

use actions::process_action;
pub use env::{load_standard_library, Environment, EvalError, Value};
pub use kurbo_shapes::{morph_kurbo_shapes, morph_kurbo_shapes_default, KurboShape_};
pub use svg::parse_svg;
pub use track::{AnimationTrack, Interpolate, PropertyTrack};
pub use utils::{evaluate_expr, parse_color, time_to_ms};
pub use vello_path::VelloPath;

use crate::ast::{Expr, Stmt};
use crate::easing::*;
use std::collections::BTreeMap;

fn sample_recursive(
    min_t: f64,
    max_t: f64,
    p0: (f64, f64),
    p1: (f64, f64),
    depth: usize,
    ty: &str,
    env: &mut Environment,
    arg_name: &str,
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    path: &mut kurbo::BezPath,
) {
    if depth > 10 {
        path.line_to(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    env.set(arg_name, Value::Num(mid_t));
    let val = evaluate_expr(body, env).unwrap_or(Value::Num(0.0)).as_num();

    let (math_x, math_y) = if ty == "CartesianPlot" {
        (mid_t, val)
    } else {
        (val * mid_t.cos(), val * mid_t.sin())
    };

    let screen_x = -(p_size[0] / 2.0)
        + p_size[0] * ((math_x - p_x_domain[0]) / (p_x_domain[1] - p_x_domain[0]));
    let screen_y = (p_size[1] / 2.0)
        - p_size[1] * ((math_y - p_y_domain[0]) / (p_y_domain[1] - p_y_domain[0]));

    let p_mid = (screen_x, screen_y);

    let expected_mid_x = (p0.0 + p1.0) / 2.0;
    let expected_mid_y = (p0.1 + p1.1) / 2.0;
    let dist_sq = (p_mid.0 - expected_mid_x).powi(2) + (p_mid.1 - expected_mid_y).powi(2);

    if dist_sq > 0.5 || depth < 3 {
        sample_recursive(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            ty,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            path,
        );
        sample_recursive(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            ty,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            path,
        );
    } else {
        path.line_to(p1);
    }
}

#[derive(Debug, Clone)]
pub struct SceneNode {
    pub label: String,
    pub children: Vec<String>,
}

#[derive(Clone)]
pub struct Timeline {
    pub tracks: BTreeMap<String, AnimationTrack>,
    pub background_color: PropertyTrack<[f32; 4]>,
    pub nodes: BTreeMap<String, SceneNode>,
    pub root_nodes: Vec<String>,
    pub anon_counter: usize,
    pub env: Environment,
}

impl Timeline {
    pub fn new() -> Self {
        let mut bg_track = PropertyTrack::new([0.0, 0.0, 0.0, 1.0]);
        bg_track.add_keyframe(0, [0.0, 0.0, 0.0, 1.0], Easing::Linear);
        Self {
            tracks: BTreeMap::new(),
            background_color: bg_track,
            nodes: BTreeMap::new(),
            root_nodes: Vec::new(),
            anon_counter: 0,
            env: Environment::raw_new(),
        }
    }

    pub fn build(ast: &[Stmt]) -> Self {
        let mut timeline = Self::new();
        load_standard_library(&mut timeline.env);
        let mut current_time_ms = 0.0;

        for stmt in ast {
            match stmt {
                Stmt::Keyframe { time, body } => {
                    current_time_ms = time_to_ms(time);
                    timeline.process_body(current_time_ms, body, None);
                }
                Stmt::RelativeKeyframe { offset, body } => {
                    current_time_ms += time_to_ms(offset);
                    timeline.process_body(current_time_ms, body, None);
                }
                Stmt::ActorDecl { .. } | Stmt::Assignment { .. } => {
                    timeline.process_body(current_time_ms, &[stmt.clone()], None);
                }
                _ => {}
            }
        }
        timeline
    }

    fn add_node(&mut self, label: String, parent_label: Option<&str>) {
        if !self.nodes.contains_key(&label) {
            self.nodes.insert(
                label.clone(),
                SceneNode {
                    label: label.clone(),
                    children: Vec::new(),
                },
            );
            if let Some(parent) = parent_label {
                if let Some(p) = self.nodes.get_mut(parent) {
                    if !p.children.contains(&label) {
                        p.children.push(label.clone());
                    }
                }
            } else {
                if !self.root_nodes.contains(&label) {
                    self.root_nodes.push(label.clone());
                }
            }
        }
    }

    /// Apply layout algorithm for Row and Col containers.
    /// Computes and sets child positions based on container type, gap, and alignment.
    ///
    /// - `gap`: spacing between children (default 0.0)
    /// - `align`: alignment perpendicular to the layout axis.
    ///   For Row: "center" (default), "start" (top), "end" (bottom)
    ///   For Col: "center" (default), "start" (left), "end" (right)
    fn apply_container_layout(
        &mut self,
        container_label: &str,
        container_ty: &str,
        time_ms: f64,
        gap: f32,
        align: Option<&str>,
    ) {
        let container_pos = if let Some(track) = self.tracks.get(container_label) {
            track.position.last_value()
        } else {
            [0.0, 0.0]
        };

        let container_x = container_pos[0];
        let container_y = container_pos[1];

        let children = if let Some(node) = self.nodes.get(container_label) {
            node.children.clone()
        } else {
            return;
        };

        let is_row = container_ty == "Row";
        let is_col = container_ty == "Col";

        if !is_row && !is_col {
            return; // Group, Stack, Grid don't use auto-layout yet
        }

        // Pre-compute total content extent to support alignment.
        // For Row: total width; for Col: total height.
        let mut total_extent = 0.0f32;
        let mut max_cross_extent = 0.0f32; // max height for Row, max width for Col
        let child_extents: Vec<(f32, f32)> = children
            .iter()
            .filter_map(|cl| {
                self.tracks.get(cl).map(|t| {
                    let s = t.size.last_value();
                    let w = s[0] * 2.0;
                    let h = s[1] * 2.0;
                    if is_row {
                        total_extent += w;
                        if max_cross_extent < h {
                            max_cross_extent = h;
                        }
                    } else {
                        total_extent += h;
                        if max_cross_extent < w {
                            max_cross_extent = w;
                        }
                    }
                    (w, h)
                })
            })
            .collect();

        // Add gaps between children
        if !children.is_empty() && children.len() > 1 {
            total_extent += gap * (children.len() as f32 - 1.0);
        }

        // Determine the offset for the perpendicular axis alignment
        let cross_offset = match align.unwrap_or("center") {
            "start" => {
                if is_row {
                    // Align to top (lower Y is top in canvas coords)
                    container_y - max_cross_extent / 2.0
                } else {
                    // Align to left (lower X)
                    container_x - max_cross_extent / 2.0
                }
            }
            "end" => {
                if is_row {
                    // Align to bottom (higher Y)
                    container_y + max_cross_extent / 2.0
                } else {
                    // Align to right (higher X)
                    container_x + max_cross_extent / 2.0
                }
            }
            _ /* "center" or unknown */ => {
                // Center on the container axis
                if is_row {
                    container_y
                } else {
                    container_x
                }
            }
        };

        // Compute the starting offset along the main axis (centered within container)
        let main_start = if is_row {
            container_x - total_extent / 2.0
        } else {
            container_y - total_extent / 2.0
        };

        let mut offset = 0.0f32;
        let t_ms = time_ms as u64;

        for (i, child_label) in children.iter().enumerate() {
            if let Some(track) = self.tracks.get_mut(child_label) {
                let (child_w, child_h) = child_extents[i];

                let (x, y) = if is_row {
                    // Main axis: X; cross axis: Y
                    let cx = main_start + offset + child_w / 2.0;
                    offset += child_w;
                    if i < children.len() - 1 {
                        offset += gap;
                    }
                    let cy = match align.unwrap_or("center") {
                        "start" => cross_offset + child_h / 2.0, // top
                        "end" => cross_offset - child_h / 2.0,   // bottom
                        _ => cross_offset,                       // center
                    };
                    (cx, cy)
                } else {
                    // Col: main axis: Y; cross axis: X
                    let cy = main_start + offset + child_h / 2.0;
                    offset += child_h;
                    if i < children.len() - 1 {
                        offset += gap;
                    }
                    let cx = match align.unwrap_or("center") {
                        "start" => cross_offset + child_w / 2.0, // left
                        "end" => cross_offset - child_w / 2.0,   // right
                        _ => cross_offset,                       // center
                    };
                    (cx, cy)
                };

                let current_pos = track.position.last_value();
                if current_pos[0] == 0.0 && current_pos[1] == 0.0 {
                    track.position.add_keyframe(t_ms, [x, y], Easing::Linear);
                }
            }
        }
    }

    fn process_inline_items(
        &mut self,
        time_ms: f64,
        items: &[crate::ast::InlineItem],
        parent_label: &str,
    ) {
        for item in items {
            match item {
                crate::ast::InlineItem::Anonymous {
                    ty,
                    props,
                    modifiers,
                    children,
                } => {
                    let id = format!("__anon_{}", self.anon_counter);
                    self.anon_counter += 1;
                    let stmt = Stmt::ActorDecl {
                        is_pub: false,
                        label: id.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: children.clone(),
                    };
                    self.process_body(time_ms, &[stmt], Some(parent_label));
                }
                crate::ast::InlineItem::Labeled {
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                } => {
                    let stmt = Stmt::ActorDecl {
                        is_pub: false,
                        label: label.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: children.clone(),
                    };
                    self.process_body(time_ms, &[stmt], Some(parent_label));
                }
            }
        }
    }

    fn process_body(&mut self, time_ms: f64, body: &[Stmt], parent_label: Option<&str>) {
        for stmt in body {
            match stmt {
                Stmt::Text {
                    label,
                    props,
                    modifiers,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_text".to_string());
                    self.add_node(label_str.clone(), parent_label);
                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str));

                    let mut text_content = String::new();
                    let mut font_size = 48.0;
                    let mut color = typst::visualize::Color::from_u8(255, 255, 255, 255);

                    for prop in props {
                        match prop.name.as_str() {
                            "text" => {
                                if let Expr::Str(s) = &prop.value {
                                    text_content = s.clone();
                                }
                            }
                            "font_size" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                font_size = v.as_num() as f32;
                            }
                            "color" => {
                                let c = parse_color(&prop.value);
                                color = typst::visualize::Color::from_u8(
                                    (c[0] * 255.0) as u8,
                                    (c[1] * 255.0) as u8,
                                    (c[2] * 255.0) as u8,
                                    (c[3] * 255.0) as u8,
                                );
                                let t_ms = time_ms as u64;
                                track.color.add_keyframe(t_ms, c, Easing::Linear);
                            }
                            "at" => {
                                let pos_val = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([x, y]) = pos_val {
                                    track.position.add_keyframe(
                                        time_ms as u64,
                                        [x as f32, y as f32],
                                        Easing::Linear,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }

                    let frame =
                        crate::renderer::text::compile_math(&text_content, font_size, color);
                    let new_paths = crate::renderer::text::extract_glyphs(&frame);

                    let mut duration_ms = 0.0;
                    let mut easing = Easing::Linear;

                    for modifier in modifiers {
                        if modifier.name.as_deref() == Some("ease") {
                            if let Expr::Ident(val) = &modifier.value {
                                match val.as_str() {
                                    "ease-in" => easing = Easing::EaseIn,
                                    "ease-out" => easing = Easing::EaseOut,
                                    "ease-in-out" => easing = Easing::EaseInOut,
                                    "bounce" => easing = Easing::Bounce,
                                    "linear" => easing = Easing::Linear,
                                    _ => {}
                                }
                            }
                        } else if modifier.name.is_none() {
                            if let Expr::Ident(val) = &modifier.value {
                                if val.ends_with("ms") {
                                    if let Ok(ms) = val.trim_end_matches("ms").parse::<f64>() {
                                        duration_ms = ms;
                                    }
                                } else if val.ends_with('s') {
                                    if let Ok(s) = val.trim_end_matches('s').parse::<f64>() {
                                        duration_ms = s * 1000.0;
                                    }
                                }
                            }
                        }
                    }

                    let t_start_ms = time_ms as u64;
                    let t_end_ms = (time_ms + duration_ms) as u64;

                    if duration_ms > 0.0 {
                        let start_val = track.text_paths.evaluate(t_start_ms);
                        track
                            .text_paths
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    }
                    track.text_paths.add_keyframe(t_end_ms, new_paths, easing);
                }
                Stmt::Math {
                    label,
                    props,
                    modifiers,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_math".to_string());
                    self.add_node(label_str.clone(), parent_label);
                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str));

                    let mut latex_content = String::new();
                    let mut font_size = 48.0;
                    let mut color = typst::visualize::Color::from_u8(255, 255, 255, 255);

                    for prop in props {
                        match prop.name.as_str() {
                            "latex" | "math" => {
                                if let Expr::Str(s) = &prop.value {
                                    latex_content = s.clone();
                                }
                            }
                            "font_size" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                font_size = v.as_num() as f32;
                            }
                            "color" => {
                                let c = parse_color(&prop.value);
                                color = typst::visualize::Color::from_u8(
                                    (c[0] * 255.0) as u8,
                                    (c[1] * 255.0) as u8,
                                    (c[2] * 255.0) as u8,
                                    (c[3] * 255.0) as u8,
                                );
                                let t_ms = time_ms as u64;
                                track.color.add_keyframe(t_ms, c, Easing::Linear);
                            }
                            "at" => {
                                let pos_val = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([x, y]) = pos_val {
                                    track.position.add_keyframe(
                                        time_ms as u64,
                                        [x as f32, y as f32],
                                        Easing::Linear,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }

                    let frame =
                        crate::renderer::text::compile_math(&latex_content, font_size, color);
                    let new_paths = crate::renderer::text::extract_glyphs(&frame);

                    let mut duration_ms = 0.0;
                    let mut easing = Easing::Linear;

                    for modifier in modifiers {
                        if modifier.name.as_deref() == Some("ease") {
                            if let Expr::Ident(val) = &modifier.value {
                                match val.as_str() {
                                    "ease-in" => easing = Easing::EaseIn,
                                    "ease-out" => easing = Easing::EaseOut,
                                    "ease-in-out" => easing = Easing::EaseInOut,
                                    "bounce" => easing = Easing::Bounce,
                                    "linear" => easing = Easing::Linear,
                                    _ => {}
                                }
                            }
                        } else if modifier.name.is_none() {
                            if let Expr::Ident(val) = &modifier.value {
                                if val.ends_with("ms") {
                                    if let Ok(ms) = val.trim_end_matches("ms").parse::<f64>() {
                                        duration_ms = ms;
                                    }
                                } else if val.ends_with('s') {
                                    if let Ok(s) = val.trim_end_matches('s').parse::<f64>() {
                                        duration_ms = s * 1000.0;
                                    }
                                }
                            }
                        }
                    }

                    let t_start_ms = time_ms as u64;
                    let t_end_ms = (time_ms + duration_ms) as u64;

                    if duration_ms > 0.0 {
                        let start_val = track.text_paths.evaluate(t_start_ms);
                        track
                            .text_paths
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    }
                    track.text_paths.add_keyframe(t_end_ms, new_paths, easing);
                }
                Stmt::Svg {
                    label,
                    url,
                    at,
                    scale,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_svg".to_string());
                    self.add_node(label_str.clone(), parent_label);
                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str));

                    track
                        .position
                        .add_keyframe(time_ms as u64, [at.0, at.1], Easing::Linear);

                    let svg_content = std::fs::read_to_string(url).unwrap_or_else(|e| {
                        eprintln!("Failed to read SVG file {}: {}", url, e);
                        String::new()
                    });

                    if !svg_content.is_empty() {
                        let mut parsed_paths = crate::timeline::svg::parse_svg(&svg_content);
                        if *scale != 1.0 {
                            let affine = kurbo::Affine::scale(*scale as f64);
                            for vp in &mut parsed_paths {
                                vp.path.apply_affine(affine);
                            }
                        }
                        track.svg_paths = parsed_paths;
                    }
                }
                Stmt::ActorDecl {
                    is_pub: _,
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                } => {
                    self.add_node(label.clone(), parent_label);

                    let mut x_domain = [-10.0, 10.0];
                    let mut y_domain = [-10.0, 10.0];
                    let mut t_domain = [0.0, std::f64::consts::TAU];
                    let mut func = None;
                    let mut initial_size = [50.0, 50.0];

                    for prop in props {
                        match prop.name.as_str() {
                            "size" => {
                                let size_val = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([w, h]) = size_val {
                                    initial_size[0] = w as f32 / 2.0;
                                    initial_size[1] = h as f32 / 2.0;
                                }
                            }
                            "radius" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                let r = v.as_num() as f32;
                                initial_size = [r, r];
                            }
                            "x_domain" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    x_domain = [min, max];
                                }
                            }
                            "y_domain" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    y_domain = [min, max];
                                }
                            }
                            "t_domain" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    t_domain = [min, max];
                                }
                            }
                            "func" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Closure(args, body) = v {
                                    func = Some((args, body));
                                }
                            }
                            _ => {}
                        }
                    }

                    if ty == "Graph" {
                        self.env
                            .set(&format!("{}_x_domain", label), Value::Vec2(x_domain));
                        self.env
                            .set(&format!("{}_y_domain", label), Value::Vec2(y_domain));
                        self.env.set(
                            &format!("{}_size", label),
                            Value::Vec2([
                                initial_size[0] as f64 * 2.0,
                                initial_size[1] as f64 * 2.0,
                            ]),
                        );
                    }

                    self.process_inline_items(time_ms, children, label);

                    let track = self
                        .tracks
                        .entry(label.clone())
                        .or_insert_with(|| AnimationTrack::new(label.clone()));

                    let mut position = track.position.last_value();
                    let mut size = track.size.last_value();
                    let mut color = track.color.last_value();
                    let shape_type = if ty == "Circle" { 1 } else { 0 };
                    let opacity = track.opacity.last_value();
                    let mut stroke_width = track.stroke_width.last_value();
                    let mut stroke_color = track.stroke_color.last_value();
                    let mut stroke_progress = track.stroke_progress.last_value();
                    let mut fill_opacity = track.fill_opacity.last_value();

                    let mut easing = Easing::Linear;
                    for modifier in modifiers {
                        if modifier.name.as_deref() == Some("ease") || modifier.name.is_none() {
                            if let Expr::Ident(val) = &modifier.value {
                                match val.as_str() {
                                    "ease-in" => easing = Easing::EaseIn,
                                    "ease-out" => easing = Easing::EaseOut,
                                    "ease-in-out" => easing = Easing::EaseInOut,
                                    "bounce" => easing = Easing::Bounce,
                                    "linear" => easing = Easing::Linear,
                                    _ => {}
                                }
                            }
                        }
                    }

                    for prop in props {
                        match prop.name.as_str() {
                            "at" => {
                                let pos_val = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([x, y]) = pos_val {
                                    position[0] = x as f32;
                                    position[1] = y as f32;
                                }
                            }
                            "radius" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                let r = v.as_num() as f32;
                                size = [r, r];
                            }
                            "size" => {
                                let size_val = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([w, h]) = size_val {
                                    size[0] = w as f32 / 2.0;
                                    size[1] = h as f32 / 2.0;
                                }
                            }
                            "color" => {
                                color = parse_color(&prop.value);
                            }
                            "stroke_width" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                stroke_width = v.as_num() as f32;
                            }
                            "stroke_color" => {
                                stroke_color = parse_color(&prop.value);
                            }
                            "stroke_progress" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                stroke_progress = v.as_num() as f32;
                            }
                            "fill_opacity" => {
                                let v = evaluate_expr(&prop.value, &self.env)
                                    .unwrap_or(Value::Num(0.0));
                                fill_opacity = v.as_num() as f32;
                            }
                            _ => {}
                        }
                    }

                    let shape = match shape_type {
                        1 => crate::timeline::kurbo_shapes::KurboShape_::Circle {
                            center: kurbo::Point::new(0.0, 0.0),
                            radius: size[0] as f64,
                        },
                        _ => crate::timeline::kurbo_shapes::KurboShape_::Rect {
                            x0: -(size[0] as f64),
                            y0: -(size[1] as f64),
                            x1: size[0] as f64,
                            y1: size[1] as f64,
                        },
                    };

                    let mut vello_paths = vec![];

                    if ty == "Graph" {
                        let mut path = kurbo::BezPath::new();
                        // X axis
                        let x_axis_y = if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
                            size[1] as f64
                                * (1.0 - 2.0 * (0.0 - y_domain[0]) / (y_domain[1] - y_domain[0]))
                        } else {
                            size[1] as f64
                        };
                        path.move_to((-(size[0] as f64), x_axis_y));
                        path.line_to((size[0] as f64, x_axis_y));

                        // Y axis
                        let y_axis_x = if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
                            size[0] as f64
                                * (-1.0 + 2.0 * (0.0 - x_domain[0]) / (x_domain[1] - x_domain[0]))
                        } else {
                            -(size[0] as f64)
                        };
                        path.move_to((y_axis_x, -(size[1] as f64)));
                        path.line_to((y_axis_x, size[1] as f64));

                        // Simple grid
                        let grid_steps = 10;
                        let mut grid_path = kurbo::BezPath::new();
                        for i in 0..=grid_steps {
                            let f = i as f64 / grid_steps as f64;
                            let gx = -(size[0] as f64) + 2.0 * (size[0] as f64) * f;
                            grid_path.move_to((gx, -(size[1] as f64)));
                            grid_path.line_to((gx, size[1] as f64));

                            let gy = -(size[1] as f64) + 2.0 * (size[1] as f64) * f;
                            grid_path.move_to((-(size[0] as f64), gy));
                            grid_path.line_to((size[0] as f64, gy));
                        }

                        vello_paths.push(crate::timeline::vello_path::VelloPath {
                            path: grid_path,
                            fill: None,
                            stroke: Some((
                                vello::peniko::Color::from_rgba8(255, 255, 255, 50),
                                1.0,
                            )),
                        });

                        vello_paths.push(crate::timeline::vello_path::VelloPath {
                            path,
                            fill: None,
                            stroke: Some((
                                vello::peniko::Color::from_rgba8(255, 255, 255, 255),
                                2.0,
                            )),
                        });
                    } else if ty == "CartesianPlot" || ty == "PolarPlot" {
                        let p_label = parent_label.unwrap_or("").to_string();
                        let mut p_x_domain = [-10.0, 10.0];
                        let mut p_y_domain = [-10.0, 10.0];
                        let mut p_size = [500.0, 500.0];

                        if let Some(Value::Vec2(xd)) =
                            self.env.get(&format!("{}_x_domain", p_label))
                        {
                            p_x_domain = xd;
                        }
                        if let Some(Value::Vec2(yd)) =
                            self.env.get(&format!("{}_y_domain", p_label))
                        {
                            p_y_domain = yd;
                        }
                        if let Some(Value::Vec2(sz)) = self.env.get(&format!("{}_size", p_label)) {
                            p_size = sz;
                        }

                        if let Some((args, body)) = func {
                            let mut path = kurbo::BezPath::new();

                            let mut env_copy = self.env.clone();
                            let arg_name = if !args.is_empty() {
                                args[0].clone()
                            } else {
                                "x".to_string()
                            };

                            let (min_t, max_t) = if ty == "CartesianPlot" {
                                (p_x_domain[0], p_x_domain[1])
                            } else {
                                (t_domain[0], t_domain[1])
                            };

                            env_copy.set(&arg_name, Value::Num(min_t));
                            let start_val = evaluate_expr(&body, &env_copy)
                                .unwrap_or(Value::Num(0.0))
                                .as_num();
                            let (start_math_x, start_math_y) = if ty == "CartesianPlot" {
                                (min_t, start_val)
                            } else {
                                (start_val * min_t.cos(), start_val * min_t.sin())
                            };
                            let start_screen_x = -(p_size[0] / 2.0)
                                + p_size[0]
                                    * ((start_math_x - p_x_domain[0])
                                        / (p_x_domain[1] - p_x_domain[0]));
                            let start_screen_y = (p_size[1] / 2.0)
                                - p_size[1]
                                    * ((start_math_y - p_y_domain[0])
                                        / (p_y_domain[1] - p_y_domain[0]));

                            path.move_to((start_screen_x, start_screen_y));

                            env_copy.set(&arg_name, Value::Num(max_t));
                            let end_val = evaluate_expr(&body, &env_copy)
                                .unwrap_or(Value::Num(0.0))
                                .as_num();
                            let (end_math_x, end_math_y) = if ty == "CartesianPlot" {
                                (max_t, end_val)
                            } else {
                                (end_val * max_t.cos(), end_val * max_t.sin())
                            };
                            let end_screen_x = -(p_size[0] / 2.0)
                                + p_size[0]
                                    * ((end_math_x - p_x_domain[0])
                                        / (p_x_domain[1] - p_x_domain[0]));
                            let end_screen_y = (p_size[1] / 2.0)
                                - p_size[1]
                                    * ((end_math_y - p_y_domain[0])
                                        / (p_y_domain[1] - p_y_domain[0]));

                            let p0 = (start_screen_x, start_screen_y);
                            let p1 = (end_screen_x, end_screen_y);

                            sample_recursive(
                                min_t,
                                max_t,
                                p0,
                                p1,
                                0,
                                ty,
                                &mut env_copy,
                                &arg_name,
                                &body,
                                &p_x_domain,
                                &p_y_domain,
                                &p_size,
                                &mut path,
                            );
                            vello_paths.push(crate::timeline::vello_path::VelloPath {
                                path,
                                fill: None,
                                stroke: if stroke_width > 0.0 {
                                    Some((
                                        vello::peniko::Color::from_rgba8(
                                            (stroke_color[0] * 255.0) as u8,
                                            (stroke_color[1] * 255.0) as u8,
                                            (stroke_color[2] * 255.0) as u8,
                                            (stroke_color[3] * 255.0) as u8,
                                        ),
                                        stroke_width,
                                    ))
                                } else {
                                    None
                                },
                            });
                        }
                    } else {
                        let vello_path = crate::timeline::vello_path::VelloPath {
                            path: shape.to_path_default(),
                            fill: if fill_opacity > 0.0 {
                                Some(vello::peniko::Color::from_rgba8(
                                    (color[0] * 255.0) as u8,
                                    (color[1] * 255.0) as u8,
                                    (color[2] * 255.0) as u8,
                                    (color[3] * 255.0 * fill_opacity) as u8,
                                ))
                            } else {
                                None
                            },
                            stroke: if stroke_width > 0.0 {
                                Some((
                                    vello::peniko::Color::from_rgba8(
                                        (stroke_color[0] * 255.0) as u8,
                                        (stroke_color[1] * 255.0) as u8,
                                        (stroke_color[2] * 255.0) as u8,
                                        (stroke_color[3] * 255.0) as u8,
                                    ),
                                    stroke_width,
                                ))
                            } else {
                                None
                            },
                        };
                        vello_paths.push(vello_path);
                    }

                    let t_ms = time_ms as u64;
                    track.vector_paths.add_keyframe(t_ms, vello_paths, easing);
                    track.position.add_keyframe(t_ms, position, easing);
                    track.size.add_keyframe(t_ms, size, easing);
                    track.color.add_keyframe(t_ms, color, easing);
                    track.shape_type.add_keyframe(t_ms, shape_type, easing);
                    track.opacity.add_keyframe(t_ms, opacity, easing);
                    track.stroke_width.add_keyframe(t_ms, stroke_width, easing);
                    track.stroke_color.add_keyframe(t_ms, stroke_color, easing);
                    track
                        .stroke_progress
                        .add_keyframe(t_ms, stroke_progress, easing);
                    track.fill_opacity.add_keyframe(t_ms, fill_opacity, easing);

                    if ty == "Row" || ty == "Col" {
                        let mut gap = 0.0f32;
                        let mut align: Option<String> = None;
                        for prop in props {
                            match prop.name.as_str() {
                                "gap" => {
                                    let v = evaluate_expr(&prop.value, &self.env)
                                        .unwrap_or(Value::Num(0.0));
                                    gap = v.as_num() as f32;
                                }
                                "align" => {
                                    if let Expr::Str(s) = &prop.value {
                                        align = Some(s.clone());
                                    } else if let Expr::Ident(s) = &prop.value {
                                        align = Some(s.clone());
                                    }
                                }
                                _ => {}
                            }
                        }
                        self.apply_container_layout(label, ty, time_ms, gap, align.as_deref());
                    }
                }
                Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                } => {
                    let mut duration_ms = 0.0;
                    let mut easing = Easing::Linear;

                    for modifier in modifiers {
                        if modifier.name.as_deref() == Some("ease") {
                            if let Expr::Ident(val) = &modifier.value {
                                match val.as_str() {
                                    "ease-in" => easing = Easing::EaseIn,
                                    "ease-out" => easing = Easing::EaseOut,
                                    "ease-in-out" => easing = Easing::EaseInOut,
                                    "bounce" => easing = Easing::Bounce,
                                    "linear" => easing = Easing::Linear,
                                    _ => {}
                                }
                            }
                        } else if modifier.name.is_none() {
                            // Try to parse duration (e.g., 2s, 500ms)
                            if let Expr::Ident(val) = &modifier.value {
                                if val.ends_with("ms") {
                                    if let Ok(ms) = val.trim_end_matches("ms").parse::<f64>() {
                                        duration_ms = ms;
                                    }
                                } else if val.ends_with('s') {
                                    if let Ok(s) = val.trim_end_matches('s').parse::<f64>() {
                                        duration_ms = s * 1000.0;
                                    }
                                }
                            }
                        }
                    }

                    let t_start_ms = time_ms as u64;
                    let t_end_ms = (time_ms + duration_ms) as u64;

                    if target == "scene" {
                        if property == "background_color" {
                            let target_color = parse_color(value);
                            if duration_ms > 0.0 {
                                let start_val = self.background_color.evaluate(t_start_ms);
                                self.background_color.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            self.background_color
                                .add_keyframe(t_end_ms, target_color, easing);
                        }
                        continue;
                    }

                    let track = self
                        .tracks
                        .entry(target.clone())
                        .or_insert_with(|| AnimationTrack::new(target.clone()));

                    match property.as_str() {
                        "color" => {
                            let target_color = parse_color(value);
                            if duration_ms > 0.0 {
                                let start_val = track.color.evaluate(t_start_ms);
                                track
                                    .color
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.color.add_keyframe(t_end_ms, target_color, easing);
                        }
                        "stroke_width" => {
                            let target_width = evaluate_expr(value, &self.env)
                                .unwrap_or(Value::Num(0.0))
                                .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.stroke_width.evaluate(t_start_ms);
                                track.stroke_width.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            track
                                .stroke_width
                                .add_keyframe(t_end_ms, target_width, easing);
                        }
                        "stroke_color" => {
                            let target_color = parse_color(value);
                            if duration_ms > 0.0 {
                                let start_val = track.stroke_color.evaluate(t_start_ms);
                                track.stroke_color.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            track
                                .stroke_color
                                .add_keyframe(t_end_ms, target_color, easing);
                        }
                        "stroke_progress" => {
                            let target_val = evaluate_expr(value, &self.env)
                                .unwrap_or(Value::Num(0.0))
                                .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.stroke_progress.evaluate(t_start_ms);
                                track.stroke_progress.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            track
                                .stroke_progress
                                .add_keyframe(t_end_ms, target_val, easing);
                        }
                        "fill_opacity" => {
                            let target_val = evaluate_expr(value, &self.env)
                                .unwrap_or(Value::Num(0.0))
                                .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.fill_opacity.evaluate(t_start_ms);
                                track.fill_opacity.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            track
                                .fill_opacity
                                .add_keyframe(t_end_ms, target_val, easing);
                        }
                        "size" => {
                            let size_val =
                                evaluate_expr(value, &self.env).unwrap_or(Value::Num(0.0));
                            let target_size = if let Value::Vec2([w, h]) = size_val {
                                [w as f32 / 2.0, h as f32 / 2.0]
                            } else {
                                track.size.last_value()
                            };
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        "position" | "at" => {
                            let pos_val =
                                evaluate_expr(value, &self.env).unwrap_or(Value::Num(0.0));
                            let target_pos = if let Value::Vec2([x, y]) = pos_val {
                                [x as f32, y as f32]
                            } else {
                                track.position.last_value()
                            };
                            if duration_ms > 0.0 {
                                let start_val = track.position.evaluate(t_start_ms);
                                track
                                    .position
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.position.add_keyframe(t_end_ms, target_pos, easing);
                        }
                        "radius" => {
                            let r = evaluate_expr(value, &self.env)
                                .unwrap_or(Value::Num(0.0))
                                .as_num() as f32;
                            let target_size = [r, r];
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        _ => {}
                    }

                    let shape_type = track.shape_type.last_value();
                    let size = track.size.last_value();
                    let color = track.color.last_value();
                    let stroke_width = track.stroke_width.last_value();
                    let stroke_color = track.stroke_color.last_value();
                    let fill_opacity = track.fill_opacity.last_value();

                    let shape = match shape_type {
                        1 => crate::timeline::kurbo_shapes::KurboShape_::Circle {
                            center: kurbo::Point::new(0.0, 0.0),
                            radius: size[0] as f64,
                        },
                        _ => crate::timeline::kurbo_shapes::KurboShape_::Rect {
                            x0: -(size[0] as f64),
                            y0: -(size[1] as f64),
                            x1: size[0] as f64,
                            y1: size[1] as f64,
                        },
                    };

                    let target_vello_path = crate::timeline::vello_path::VelloPath {
                        path: shape.to_path_default(),
                        fill: if fill_opacity > 0.0 {
                            Some(vello::peniko::Color::from_rgba8(
                                (color[0] * 255.0) as u8,
                                (color[1] * 255.0) as u8,
                                (color[2] * 255.0) as u8,
                                (color[3] * 255.0 * fill_opacity) as u8,
                            ))
                        } else {
                            None
                        },
                        stroke: if stroke_width > 0.0 {
                            Some((
                                vello::peniko::Color::from_rgba8(
                                    (stroke_color[0] * 255.0) as u8,
                                    (stroke_color[1] * 255.0) as u8,
                                    (stroke_color[2] * 255.0) as u8,
                                    (stroke_color[3] * 255.0) as u8,
                                ),
                                stroke_width,
                            ))
                        } else {
                            None
                        },
                    };

                    if duration_ms > 0.0 {
                        let start_val = track.vector_paths.evaluate(t_start_ms);
                        track
                            .vector_paths
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    }
                    track
                        .vector_paths
                        .add_keyframe(t_end_ms, vec![target_vello_path], easing);
                }
                Stmt::Action(action) => {
                    process_action(action, time_ms, self);
                }
                _ => {}
            }
        }
    }

    pub fn extract_all_glyphs(&self) -> Vec<crate::renderer::text::TextPath> {
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
        scene: &mut vello::Scene,
    ) {
        let (global_transform, global_opacity) = if let Some(track) = self.tracks.get(node_label) {
            let position = track.position.evaluate(time_ms);
            let opacity = track.opacity.evaluate(time_ms);
            let text_paths = track.text_paths.evaluate(time_ms);
            let vector_paths = track.vector_paths.evaluate(time_ms);

            let local_opacity = opacity * parent_opacity;
            let local_transform = parent_transform
                * kurbo::Affine::translate((position[0] as f64, position[1] as f64));

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

            (local_transform, local_opacity)
        } else {
            (parent_transform, parent_opacity)
        };

        if let Some(node) = self.nodes.get(node_label) {
            for child in &node.children {
                self.evaluate_node(child, time_ms, global_transform, global_opacity, scene);
            }
        }
    }

    pub fn evaluate(&self, time_s: f64) -> vello::Scene {
        let time_ms = (time_s * 1000.0) as u64;
        let mut scene = vello::Scene::new();
        let bg_color = self.background_color.evaluate(time_ms);

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
            &kurbo::Rect::new(0.0, 0.0, 1920.0, 1080.0),
        );

        for root in &self.root_nodes {
            self.evaluate_node(root, time_ms, kurbo::Affine::IDENTITY, 1.0, &mut scene);
        }

        scene
    }
}
impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}
