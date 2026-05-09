use animatix::timeline::{AnimationTrack, ShapeType, Timeline};
use egui::{Color32, RichText, ScrollArea, Vec2};

use super::workspace::UiActions;

mod property_groups;
mod keyframe_table;

use self::property_groups::*;
use self::keyframe_table::*;

/// Renders the actor inspector panel.
///
/// Shows:
/// - A collapsible tree view of all actors in the timeline
/// - Selected actor's properties grouped by category (editable)
/// - Keyframe list with current-time highlighting
pub(super) fn inspector_ui(
    ui: &mut egui::Ui,
    timeline: Option<&Timeline>,
    selected_actor: &mut Option<String>,
    current_time_s: f64,
    actions: &mut UiActions,
    keyframe_mode: bool,
) {
    ui.vertical(|ui| {
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
                    .size(11.0)
                    .color(Color32::from_rgb(90, 96, 110)),
            );
            return;
        };

        let root_nodes = timeline.root_actor_labels();
        if root_nodes.is_empty() {
            ui.add_space(20.0);
            ui.label(
                RichText::new("No actors in scene")
                    .size(11.0)
                    .color(Color32::from_rgb(90, 96, 110)),
            );
            return;
        }

        // Split: actor list on top, details on bottom
        let available = ui.available_size_before_wrap();
        let list_height = (available.y * 0.35).max(120.0);

        // Actor tree
        egui::Frame::NONE.show(ui, |ui| {
            ui.set_max_height(list_height);
            ScrollArea::vertical().show(ui, |ui| {
                let actor_count = count_all_actors(timeline, root_nodes);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ACTORS")
                            .size(10.0)
                            .color(Color32::from_rgb(90, 96, 110))
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(actor_count.to_string())
                                .size(10.0)
                                .color(Color32::from_rgb(90, 96, 110)),
                        );
                    });
                });
                ui.add_space(2.0);
                // Subtle separator
                let sep_rect = ui.allocate_space(Vec2::new(ui.available_width(), 1.0)).1;
                ui.painter().rect_filled(
                    sep_rect,
                    0.0,
                    Color32::from_rgb(40, 44, 52),
                );
                ui.add_space(4.0);

                for root_label in root_nodes {
                    render_actor_tree(ui, timeline, root_label, selected_actor, 0);
                }
            });
        });

        ui.add_space(4.0);
        let sep_rect = ui.allocate_space(Vec2::new(ui.available_width(), 1.0)).1;
        ui.painter().rect_filled(sep_rect, 0.0, Color32::from_rgb(40, 44, 52));
        ui.add_space(4.0);

        // Selected actor details
        if let Some(sel) = selected_actor.as_ref() {
            let Some(track) = timeline.get_track(sel) else {
                ui.label(
                    RichText::new("Actor not found")
                        .size(11.0)
                        .color(Color32::from_rgb(90, 96, 110)),
                );
                return;
            };

            ScrollArea::vertical().show(ui, |ui| {
                render_actor_details(ui, track, current_time_s, actions, keyframe_mode);
            });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    RichText::new("Select an actor to inspect")
                        .size(11.0)
                        .color(Color32::from_rgb(90, 96, 110)),
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
    let height = 20.0;
    let available = ui.available_width();

    let (rect, response) = ui.allocate_exact_size(Vec2::new(available, height), egui::Sense::click());

    // Background
    let bg_color = match (is_selected, response.hovered()) {
        (true, _) => Color32::from_rgb(55, 62, 75),
        (_, true) => Color32::from_rgb(32, 36, 44),
        _ => Color32::TRANSPARENT,
    };
    if bg_color != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 3.0, bg_color);
    }

    // Selected indicator (left amber bar)
    if is_selected {
        let indicator = egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.min.y + 2.0),
            egui::pos2(rect.min.x + 2.0, rect.max.y - 2.0),
        );
        ui.painter().rect_filled(indicator, 1.0, Color32::from_rgb(255, 196, 92));
    }

    // Label
    let display_label = if is_anonymous {
        format!("{} (anon)", label)
    } else {
        label.to_string()
    };

    let text_color = if is_selected {
        Color32::from_rgb(228, 232, 243)
    } else if is_anonymous {
        Color32::from_rgb(90, 96, 110)
    } else {
        Color32::from_rgb(150, 158, 175)
    };

    let text_pos = egui::pos2(rect.min.x + indent + 4.0, rect.center().y);
    ui.painter().text(
        text_pos,
        egui::Align2::LEFT_CENTER,
        &display_label,
        egui::TextStyle::Small.resolve(ui.style()),
        text_color,
    );

    // Shape type hint (right-aligned, muted)
    if let Some(shape) = shape_hint {
        ui.painter().text(
            egui::pos2(rect.max.x - 6.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            shape,
            egui::TextStyle::Small.resolve(ui.style()),
            Color32::from_rgb(90, 96, 110),
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

fn render_actor_details(
    ui: &mut egui::Ui,
    track: &AnimationTrack,
    current_time_s: f64,
    actions: &mut UiActions,
    keyframe_mode: bool,
) {
    let current_time_ms = (current_time_s * 1000.0) as u64;

    ui.add_space(2.0);

    // Actor header: label + shape type
    ui.horizontal(|ui| {
        ui.label(RichText::new(&track.label).strong().size(14.0).color(Color32::from_rgb(228, 232, 243)));
        if let Some(shape_pt) = &track.shape_type {
            let shape = shape_pt.evaluate(current_time_ms);
            ui.label(
                RichText::new(format!("{shape:?}"))
                    .size(11.0)
                    .color(Color32::from_rgb(137, 200, 235)),
            );
        }
    });

    // First seen time
    if track.first_seen_ms > 0 && track.first_seen_ms != u64::MAX {
        ui.label(
            RichText::new(format!("First seen: {:.2}s", track.first_seen_ms as f64 / 1000.0))
                .size(10.0)
                .color(Color32::from_rgb(90, 96, 110)),
        );
    }

    ui.add_space(6.0);

    // Property groups (editable)
    let groups = build_property_groups(track, current_time_ms);
    for group in &groups {
        render_property_group(ui, group, &track.label, actions, keyframe_mode);
    }

    ui.add_space(8.0);

    // Keyframe table
    let keyframes = collect_keyframes(track);
    if !keyframes.is_empty() {
        render_keyframe_table(ui, &keyframes, current_time_ms);
    } else {
        ui.label(
            RichText::new("No keyframes — default values only")
                .size(10.0)
                .color(Color32::from_rgb(90, 96, 110)),
        );
    }
}
