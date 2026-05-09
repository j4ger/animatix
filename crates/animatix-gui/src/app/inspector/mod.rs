use animatix::timeline::{AnimationTrack, ShapeType, Timeline};
use egui::{Color32, Id, RichText, ScrollArea, Vec2};

use super::widgets;
use super::workspace::UiActions;

mod property_groups;
mod keyframe_table;

use self::property_groups::*;
use self::keyframe_table::*;

// ─── Palette (mirrors runtime.rs theme) ─────────────────────────────────────

const BG_BASE: Color32 = Color32::from_rgb(12, 14, 18);
const BG_PANEL: Color32 = Color32::from_rgb(18, 20, 24);
const BG_SURFACE: Color32 = Color32::from_rgb(24, 27, 33);
const BG_WIDGET: Color32 = Color32::from_rgb(32, 36, 44);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(228, 232, 243);
const TEXT_SECONDARY: Color32 = Color32::from_rgb(150, 158, 175);
const TEXT_MUTED: Color32 = Color32::from_rgb(90, 96, 110);
const BORDER_FOCUS: Color32 = Color32::from_rgb(84, 110, 255);
const AMBER: Color32 = Color32::from_rgb(255, 196, 92);

// ─── Inspector Tabs ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum InspectorTab {
    Properties,
    Keyframes,
}

// ─── Main Entry Point ───────────────────────────────────────────────────────

/// Renders the actor inspector panel (properties only — actor tree is in Layers tab).
pub(super) fn inspector_ui(
    ui: &mut egui::Ui,
    timeline: Option<&Timeline>,
    selected_actor: &mut Option<String>,
    current_time_s: f64,
    actions: &mut UiActions,
    keyframe_mode: bool,
) {
    let should_reset = selected_actor
        .as_ref()
        .is_some_and(|sel| timeline.is_some_and(|t| !t.has_actor(sel)));
    if should_reset {
        *selected_actor = None;
    }

    let Some(timeline) = timeline else {
        render_empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No timeline loaded",
            "Open or create a scene to begin",
        );
        return;
    };

    let root_nodes = timeline.root_actor_labels();
    if root_nodes.is_empty() {
        render_empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No actors in scene",
            "Add shapes or text to populate the stage",
        );
        return;
    }

    if let Some(sel) = selected_actor.as_ref() {
        let Some(track) = timeline.get_track(sel) else {
            render_empty_state(
                ui,
                egui_phosphor::regular::WARNING,
                "Actor not found",
                "The selected actor no longer exists in the timeline",
            );
            return;
        };

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                render_actor_header(ui, track, current_time_s);
                ui.add_space(6.0);

                let tab_id = ui.id().with("inspector_tab");
                let mut active_tab = ui
                    .data(|d| d.get_temp::<InspectorTab>(tab_id))
                    .unwrap_or(InspectorTab::Properties);

                render_tab_bar(ui, &mut active_tab);
                ui.add_space(6.0);

                match active_tab {
                    InspectorTab::Properties => {
                        let current_time_ms = (current_time_s * 1000.0) as u64;
                        let groups = build_property_groups(track, current_time_ms);
                        for group in &groups {
                            render_property_group(
                                ui,
                                group,
                                &track.label,
                                actions,
                                keyframe_mode,
                            );
                        }

                        if groups.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(20.0);
                                ui.label(
                                    RichText::new(egui_phosphor::regular::SLIDERS)
                                        .size(22.0)
                                        .color(TEXT_MUTED),
                                );
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new("No editable properties")
                                        .size(11.0)
                                        .color(TEXT_MUTED),
                                );
                            });
                        }
                    }
                    InspectorTab::Keyframes => {
                        let keyframes = collect_keyframes(track);
                        render_keyframe_table(
                            ui,
                            &keyframes,
                            (current_time_s * 1000.0) as u64,
                        );
                    }
                }

                ui.data_mut(|d| d.insert_temp(tab_id, active_tab));
            });
    } else {
        ui.vertical_centered(|ui| {
            ui.add_space(32.0);
            ui.label(
                RichText::new(egui_phosphor::regular::MAGNIFYING_GLASS)
                    .size(28.0)
                    .color(TEXT_MUTED),
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new("Select an actor to inspect")
                    .size(12.0)
                    .color(TEXT_SECONDARY),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("Click an actor in the preview or Layers panel")
                    .size(10.0)
                    .color(TEXT_MUTED),
            );
        });
    }
}

// ─── Empty State ────────────────────────────────────────────────────────────

fn render_empty_state(ui: &mut egui::Ui, icon: &str, title: &str, subtitle: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(36.0);
        ui.label(RichText::new(icon).size(28.0).color(TEXT_MUTED));
        ui.add_space(10.0);
        ui.label(RichText::new(title).size(12.0).color(TEXT_SECONDARY));
        ui.add_space(4.0);
        ui.label(RichText::new(subtitle).size(10.0).color(TEXT_MUTED));
    });
}

// ─── Actor Header ───────────────────────────────────────────────────────────

fn render_actor_header(ui: &mut egui::Ui, track: &AnimationTrack, current_time_s: f64) {
    let current_time_ms = (current_time_s * 1000.0) as u64;

    ui.horizontal(|ui| {
        // Shape icon
        if let Some(shape_pt) = &track.shape_type {
            let shape = shape_pt.evaluate(current_time_ms);
            ui.label(
                RichText::new(shape_icon(shape))
                    .size(14.0)
                    .color(AMBER),
            );
        }

        // Name
        ui.label(
            RichText::new(&track.label)
                .strong()
                .size(14.0)
                .color(TEXT_PRIMARY),
        );

        // Right side: shape type + first-seen tag
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if track.first_seen_ms > 0 && track.first_seen_ms != u64::MAX {
                ui.label(
                    RichText::new(format!(
                        "t = {:.2}s",
                        track.first_seen_ms as f64 / 1000.0
                    ))
                    .size(9.0)
                    .color(TEXT_MUTED),
                );
            }

            if let Some(shape_pt) = &track.shape_type {
                let shape = shape_pt.evaluate(current_time_ms);
                ui.label(
                    RichText::new(format!("{:?}", shape))
                        .size(10.0)
                        .color(TEXT_MUTED),
                );
            }
        });
    });
}

// ─── Tab Bar ────────────────────────────────────────────────────────────────

fn render_tab_bar(ui: &mut egui::Ui, active_tab: &mut InspectorTab) {
    let tabs = [
        (InspectorTab::Properties, egui_phosphor::regular::WRENCH, "Properties"),
        (InspectorTab::Keyframes, egui_phosphor::regular::KEY, "Keyframes"),
    ];
    if let Some(new_tab) = widgets::pill_tab_bar(ui, *active_tab, &tabs) {
        *active_tab = new_tab;
    }
}

fn shape_icon(shape: ShapeType) -> &'static str {
    match shape {
        ShapeType::Rect => egui_phosphor::regular::SQUARE,
        ShapeType::Circle => egui_phosphor::regular::CIRCLE,
        ShapeType::Line => egui_phosphor::regular::MINUS,
        ShapeType::Ellipse => egui_phosphor::regular::CIRCLE_NOTCH,
        ShapeType::Arc => egui_phosphor::regular::ARROWS_CLOCKWISE,
        ShapeType::Polygon => egui_phosphor::regular::HEXAGON,
        ShapeType::Path => egui_phosphor::regular::PEN,
        ShapeType::Arrow => egui_phosphor::regular::ARROW_RIGHT,
        ShapeType::Graph => egui_phosphor::regular::CHART_BAR,
        ShapeType::Plot => egui_phosphor::regular::DOTS_THREE_OUTLINE,
    }
}
