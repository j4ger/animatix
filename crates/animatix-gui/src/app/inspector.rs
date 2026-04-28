use animatix::timeline::{AnimationTrack, ShapeType, Timeline};
use egui::{Color32, RichText, ScrollArea, Vec2};
use std::collections::BTreeMap;

/// Renders the actor inspector panel.
///
/// Shows:
/// - A collapsible tree view of all actors in the timeline
/// - Selected actor's properties and keyframes
pub(super) fn inspector_ui(
    ui: &mut egui::Ui,
    timeline: Option<&Timeline>,
    selected_actor: &mut Option<String>,
    current_time_s: f64,
) {
    ui.vertical(|ui| {
        ui.label(RichText::new("Inspector").strong());
        // Reset selection if actor no longer exists in timeline
        let should_reset = selected_actor
            .as_ref()
            .is_some_and(|sel| timeline.is_some_and(|t| !t.has_actor(sel)));
        if should_reset {
            *selected_actor = None;
        }

        let Some(timeline) = timeline else {
            ui.add_space(20.0);
            ui.label(
                RichText::new("No timeline loaded — rebuild to inspect")
                    .small()
                    .weak(),
            );
            return;
        };

        let root_nodes = timeline.root_actor_labels();
        if root_nodes.is_empty() {
            ui.add_space(20.0);
            ui.label(RichText::new("No actors in scene").small().weak());
            return;
        }

        ui.separator();

        // Split: actor list on top, details on bottom
        let available = ui.available_size_before_wrap();
        let list_height = (available.y * 0.38).max(120.0);

        // Actor tree
        egui::Frame::NONE.show(ui, |ui| {
            ui.set_max_height(list_height);
            ScrollArea::vertical().show(ui, |ui| {
                let actor_count = count_all_actors(timeline, root_nodes);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Actors — {actor_count}")).strong());
                });
                ui.add_space(4.0);

                for root_label in root_nodes {
                    render_actor_tree(ui, timeline, root_label, selected_actor, 0);
                }
            });
        });

        ui.separator();

        // Selected actor details
        if let Some(sel) = selected_actor.as_ref() {
            let Some(track) = timeline.get_track(sel) else {
                ui.label(RichText::new("Actor not found").small().weak());
                return;
            };

            ScrollArea::vertical().show(ui, |ui| {
                render_actor_details(ui, track, current_time_s);
            });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    RichText::new("Select an actor to inspect")
                        .small()
                        .weak(),
                );
            });
        }
    });
}

fn count_all_actors(timeline: &Timeline, root_nodes: &[String]) -> usize {
    let mut count = 0;
    for root in root_nodes {
        count += 1;
        if let Some(track) = timeline.get_track(root) {
            count += count_children(timeline, &track.children);
        }
    }
    count
}

fn count_children(timeline: &Timeline, children: &[String]) -> usize {
    let mut count = 0;
    for child in children {
        count += 1;
        if let Some(track) = timeline.get_track(child) {
            count += count_children(timeline, &track.children);
        }
    }
    count
}

fn render_actor_tree(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    label: &str,
    selected_actor: &mut Option<String>,
    depth: usize,
) {
    let Some(track) = timeline.get_track(label) else {
        return;
    };

    let is_selected = selected_actor.as_deref() == Some(label);
    let is_anonymous = label.starts_with("__anon");

    let shape_hint = shape_type_hint(track);

    let indent = depth as f32 * 12.0;
    let height = 22.0;
    let available = ui.available_width();

    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(available, height),
        egui::Sense::click(),
    );

    // Background
    let bg_color = match (is_selected, response.hovered()) {
        (true, _) => Color32::from_rgb(63, 81, 181),
        (_, true) => Color32::from_rgb(45, 45, 55),
        _ => Color32::TRANSPARENT,
    };
    if bg_color != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 2.0, bg_color);
    }

    // Label
    let label_text = if is_anonymous {
        format!("{:>width$}{} (anon)", "", label, width = depth * 2)
    } else {
        format!("{:>width$}{}", "", label, width = depth * 2)
    };

    let text_color = if is_selected {
        Color32::WHITE
    } else if is_anonymous {
        Color32::from_rgb(130, 130, 140)
    } else {
        Color32::from_rgb(210, 210, 220)
    };

    let text_pos = egui::pos2(rect.min.x + indent + 4.0, rect.center().y);
    ui.painter().text(
        text_pos,
        egui::Align2::LEFT_CENTER,
        &label_text,
        egui::TextStyle::Small.resolve(ui.style()),
        text_color,
    );

    // Shape type hint
    if let Some(shape) = shape_hint {
        let shape_text = RichText::new(shape).small().weak();
        let galley = ui
            .painter()
            .layout_no_wrap(shape_text.text().to_string(), egui::TextStyle::Small.resolve(ui.style()), text_color.linear_multiply(0.5));
        let shape_pos = egui::pos2(rect.max.x - galley.size().x - 6.0, rect.center().y);
        ui.painter().text(
            shape_pos,
            egui::Align2::LEFT_CENTER,
            shape,
            egui::TextStyle::Small.resolve(ui.style()),
            text_color.linear_multiply(0.5),
        );
    }

    if response.clicked() {
        *selected_actor = Some(label.to_string());
    }

    // Render children
    for child_label in &track.children {
        render_actor_tree(ui, timeline, child_label, selected_actor, depth + 1);
    }
}

fn shape_type_hint(track: &AnimationTrack) -> Option<&'static str> {
    track.shape_type.as_ref().map(|pt| {
        let current = pt.evaluate(0);
        match current {
            ShapeType::Rect => "Rect",
            ShapeType::Circle => "Circle",
            ShapeType::Line => "Line",
            ShapeType::Ellipse => "Ellipse",
            ShapeType::Arc => "Arc",
            ShapeType::Polygon => "Polygon",
            ShapeType::Path => "Path",
            ShapeType::Arrow => "Arrow",
            ShapeType::Graph => "Graph",
            ShapeType::Plot => "Plot",
        }
    })
}

fn render_actor_details(ui: &mut egui::Ui, track: &AnimationTrack, current_time_s: f64) {
    let current_time_ms = (current_time_s * 1000.0) as u64;

    ui.add_space(4.0);
    ui.label(RichText::new(&track.label).strong().size(16.0));

    // Shape type badge
    if let Some(shape_pt) = &track.shape_type {
        let shape = shape_pt.evaluate(current_time_ms);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Shape:").small().weak());
            ui.label(
                RichText::new(format!("{shape:?}"))
                    .small()
                    .color(Color32::from_rgb(137, 200, 235)),
            );
        });
    }

    // First seen time
    if track.first_seen_ms > 0 {
        ui.horizontal(|ui| {
            ui.label(RichText::new("First seen:").small().weak());
            ui.label(
                RichText::new(format!("{:.2}s", track.first_seen_ms as f64 / 1000.0)).small(),
            );
        });
    }

    // Children
    if !track.children.is_empty() {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Children:").small().weak());
            ui.label(
                RichText::new(track.children.join(", "))
                    .small()
                    .color(Color32::from_rgb(180, 180, 200)),
            );
        });
    }

    ui.add_space(8.0);

    // Properties table
    let properties = collect_properties(track, current_time_ms);
    if !properties.is_empty() {
        ui.label(RichText::new("Properties").strong());
        ui.add_space(2.0);

        for (name, value) in &properties {
            ui.horizontal(|ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("  {name}")).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(value).small().strong());
                });
            });
        }
    }

    ui.add_space(12.0);

    // Keyframe table
    let keyframes = collect_keyframes(track);
    if !keyframes.is_empty() {
        ui.label(RichText::new("Keyframes").strong());
        ui.add_space(4.0);

        // Table header
        ui.horizontal(|ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("  Time").small().weak());
            ui.label(RichText::new("Property").small().weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("Value").small().weak());
            });
        });
        ui.separator();

        for (time_s, property, value, _easing) in &keyframes {
            let is_current = (*time_s * 1000.0) as u64 == current_time_ms;
            let text_color = if is_current {
                Color32::from_rgb(255, 214, 102)
            } else {
                Color32::from_rgb(200, 200, 210)
            };

            ui.horizontal(|ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(format!("  {:.2}s", time_s))
                        .small()
                        .color(text_color),
                );
                ui.label(RichText::new(property).small().color(text_color));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(value).small().color(text_color));
                });
            });
        }
    } else {
        ui.label(RichText::new("No keyframes — default values only").small().weak());
    }
}

fn collect_properties(track: &AnimationTrack, time_ms: u64) -> Vec<(String, String)> {
    let mut props = Vec::new();

    if let Some(pt) = &track.position {
        let v = pt.evaluate(time_ms);
        props.push(("position".into(), format!("{:?}", v)));
    }
    if let Some(pt) = &track.size {
        let v = pt.evaluate(time_ms);
        props.push(("size".into(), format!("{:?}", v)));
    }
    if let Some(pt) = &track.scale {
        let v = pt.evaluate(time_ms);
        props.push(("scale".into(), format!("{:.2}", v)));
    }
    if let Some(pt) = &track.rotation {
        let v = pt.evaluate(time_ms);
        props.push(("rotation".into(), format!("{:.2}°", v.to_degrees())));
    }
    if let Some(pt) = &track.opacity {
        let v = pt.evaluate(time_ms);
        props.push(("opacity".into(), format!("{:.2}", v)));
    }
    if let Some(pt) = &track.color {
        let v = pt.evaluate(time_ms);
        props.push(("color".into(), format!("{:?}", v)));
    }
    if let Some(pt) = &track.stroke_width {
        let v = pt.evaluate(time_ms);
        props.push(("stroke_width".into(), format!("{:.2}", v)));
    }
    if let Some(pt) = &track.stroke_color {
        let v = pt.evaluate(time_ms);
        props.push(("stroke_color".into(), format!("{:?}", v)));
    }
    if let Some(pt) = &track.fill_opacity {
        let v = pt.evaluate(time_ms);
        props.push(("fill_opacity".into(), format!("{:.2}", v)));
    }
    if let Some(pt) = &track.text_content {
        let v = pt.evaluate(time_ms);
        props.push(("text_content".into(), format!("{:?}", v)));
    }

    props
}

fn collect_keyframes(track: &AnimationTrack) -> Vec<(f64, String, String, String)> {
    let mut all: Vec<(u64, &str, String)> = Vec::new();

    // Collect keyframes from each property
    fn push_keyframes_from<V: std::fmt::Debug, E>(
        all: &mut Vec<(u64, &str, String)>,
        name: &'static str,
        keyframes: &BTreeMap<u64, (V, E)>,
    ) {
        for (&time_ms, (value, _)) in keyframes {
            all.push((time_ms, name, format!("{value:?}")));
        }
    }

    if let Some(pt) = &track.position {
        push_keyframes_from(&mut all, "position", &pt.keyframes);
    }
    if let Some(pt) = &track.size {
        push_keyframes_from(&mut all, "size", &pt.keyframes);
    }
    if let Some(pt) = &track.scale {
        push_keyframes_from(&mut all, "scale", &pt.keyframes);
    }
    if let Some(pt) = &track.rotation {
        push_keyframes_from(&mut all, "rotation", &pt.keyframes);
    }
    if let Some(pt) = &track.opacity {
        push_keyframes_from(&mut all, "opacity", &pt.keyframes);
    }
    if let Some(pt) = &track.color {
        push_keyframes_from(&mut all, "color", &pt.keyframes);
    }
    if let Some(pt) = &track.stroke_width {
        push_keyframes_from(&mut all, "stroke_width", &pt.keyframes);
    }
    if let Some(pt) = &track.stroke_color {
        push_keyframes_from(&mut all, "stroke_color", &pt.keyframes);
    }
    if let Some(pt) = &track.fill_opacity {
        push_keyframes_from(&mut all, "fill_opacity", &pt.keyframes);
    }
    if let Some(pt) = &track.text_content {
        push_keyframes_from(&mut all, "text_content", &pt.keyframes);
    }

    all.sort_by_key(|(time, _, _)| *time);

    all.into_iter()
        .map(|(time_ms, property, value)| {
            (time_ms as f64 / 1000.0, property.to_string(), value, String::new())
        })
        .collect()
}

