pub mod actions;
pub mod kurbo_shapes;
pub mod morph;
pub mod svg;
pub mod track;
pub mod utils;
pub mod vello_path;

use actions::process_action;
pub use kurbo_shapes::{morph_kurbo_shapes, morph_kurbo_shapes_default, KurboShape_};
pub use svg::parse_svg;
pub use track::{AnimationTrack, Interpolate, PropertyTrack};
pub use utils::{parse_color, time_to_ms};
pub use vello_path::VelloPath;

use crate::ast::{Expr, Stmt};
use crate::easing::*;
use std::collections::BTreeMap;

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
        }
    }

    pub fn build(ast: &[Stmt]) -> Self {
        let mut timeline = Self::new();
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
                                if let Expr::Num(s) = prop.value {
                                    font_size = s as f32;
                                }
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
                                if let Expr::Tuple(arr) = &prop.value {
                                    if arr.len() == 2 {
                                        let mut pos = track.position.last_value();
                                        if let Expr::Num(x) = arr[0] {
                                            pos[0] = x as f32;
                                        }
                                        if let Expr::Num(y) = arr[1] {
                                            pos[1] = y as f32;
                                        }
                                        track.position.add_keyframe(
                                            time_ms as u64,
                                            pos,
                                            Easing::Linear,
                                        );
                                    }
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
                                if let Expr::Num(s) = prop.value {
                                    font_size = s as f32;
                                }
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
                                if let Expr::Tuple(arr) = &prop.value {
                                    if arr.len() == 2 {
                                        let mut pos = track.position.last_value();
                                        if let Expr::Num(x) = arr[0] {
                                            pos[0] = x as f32;
                                        }
                                        if let Expr::Num(y) = arr[1] {
                                            pos[1] = y as f32;
                                        }
                                        track.position.add_keyframe(
                                            time_ms as u64,
                                            pos,
                                            Easing::Linear,
                                        );
                                    }
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
                                if let Expr::Tuple(arr) = &prop.value {
                                    if arr.len() == 2 {
                                        if let Expr::Num(x) = arr[0] {
                                            position[0] = x as f32;
                                        }
                                        if let Expr::Num(y) = arr[1] {
                                            position[1] = y as f32;
                                        }
                                    }
                                }
                            }
                            "radius" => {
                                if let Expr::Num(r) = prop.value {
                                    size = [r as f32, r as f32];
                                }
                            }
                            "size" => {
                                if let Expr::Tuple(arr) = &prop.value {
                                    if arr.len() == 2 {
                                        if let Expr::Num(w) = arr[0] {
                                            size[0] = w as f32 / 2.0;
                                        }
                                        if let Expr::Num(h) = arr[1] {
                                            size[1] = h as f32 / 2.0;
                                        }
                                    }
                                }
                            }
                            "color" => {
                                color = parse_color(&prop.value);
                            }
                            "stroke_width" => {
                                if let Expr::Num(w) = prop.value {
                                    stroke_width = w as f32;
                                }
                            }
                            "stroke_color" => {
                                stroke_color = parse_color(&prop.value);
                            }
                            "stroke_progress" => {
                                if let Expr::Num(w) = prop.value {
                                    stroke_progress = w as f32;
                                }
                            }
                            "fill_opacity" => {
                                if let Expr::Num(w) = prop.value {
                                    fill_opacity = w as f32;
                                }
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

                    let t_ms = time_ms as u64;
                    track
                        .vector_paths
                        .add_keyframe(t_ms, vec![vello_path], easing);
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
                            let mut target_width = track.stroke_width.last_value();
                            if let Expr::Num(w) = value {
                                target_width = *w as f32;
                            }
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
                            let mut target_val = track.stroke_progress.last_value();
                            if let Expr::Num(w) = value {
                                target_val = *w as f32;
                            }
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
                            let mut target_val = track.fill_opacity.last_value();
                            if let Expr::Num(w) = value {
                                target_val = *w as f32;
                            }
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
                            let mut target_size = track.size.last_value();
                            if let Expr::Tuple(arr) = value {
                                if arr.len() == 2 {
                                    if let Expr::Num(w) = arr[0] {
                                        target_size[0] = w as f32 / 2.0;
                                    }
                                    if let Expr::Num(h) = arr[1] {
                                        target_size[1] = h as f32 / 2.0;
                                    }
                                }
                            } else if let Expr::Num(r) = value {
                                target_size = [*r as f32, *r as f32];
                            }
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        "position" | "at" => {
                            let mut target_pos = track.position.last_value();
                            if let Expr::Tuple(arr) = value {
                                if arr.len() == 2 {
                                    if let Expr::Num(x) = arr[0] {
                                        target_pos[0] = x as f32;
                                    }
                                    if let Expr::Num(y) = arr[1] {
                                        target_pos[1] = y as f32;
                                    }
                                }
                            }
                            if duration_ms > 0.0 {
                                let start_val = track.position.evaluate(t_start_ms);
                                track
                                    .position
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.position.add_keyframe(t_end_ms, target_pos, easing);
                        }
                        "radius" => {
                            let mut target_size = track.size.last_value();
                            if let Expr::Num(r) = value {
                                target_size = [*r as f32, *r as f32];
                            }
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
