use animatix::timeline::{AnimationTrack, ShapeType, Timeline};
use egui::{Color32, Id, RichText, ScrollArea, Vec2};

use crate::app::components;
use crate::app::theme::*;
use crate::app::workspace::UiActions;

mod property_groups;
mod keyframe_table;

use self::property_groups::*;
use self::keyframe_table::{render_dope_sheet, collect_all_keyframe_times, count_keyframes};

// ─── Main Entry Point ─────────────────────────────────────────────────────

/// Renders the unified actor inspector panel.
///
/// Layout (single scrollable panel, no tabs):
///   1. Actor Header
///   2. Active Properties (editable, native inputs)
///   3. Mini Timeline
///   4. Keyframes
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
        components::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No timeline loaded",
            "Open or create a scene to begin",
        );
        return;
    };

    let root_nodes = timeline.root_actor_labels();
    if root_nodes.is_empty() {
        components::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No actors in scene",
            "Add shapes or text to populate the stage",
        );
        return;
    }

    if let Some(sel) = selected_actor.as_ref() {
        let Some(track) = timeline.get_track(sel) else {
            components::empty_state(
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
                ui.add_space(SPACE_M);

                // ── Active Properties ──
                components::card(ui, |ui| {
                    components::section_header(
                        ui,
                        egui_phosphor::regular::WRENCH,
                        "Properties",
                        None,
                    );
                    let current_time_ms = (current_time_s * 1000.0) as u64;
                    let groups = build_property_groups(track, current_time_ms);
                    if groups.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(SPACE_M);
                            ui.add(
                                egui::Label::new(
                                    RichText::new("No editable properties")
                                        .size(FONT_SIZE_M)
                                        .color(TEXT_MUTED),
                                )
                                .selectable(false),
                            );
                        });
                    } else {
                        for group in &groups {
                            render_property_group(
                                ui,
                                group,
                                &track.label,
                                actions,
                                keyframe_mode,
                            );
                        }
                    }
                });

                ui.add_space(SPACE_M);

                // ── Mini Timeline ──
                components::card(ui, |ui| {
                    components::section_header(
                        ui,
                        egui_phosphor::regular::CLOCK,
                        "Timeline",
                        None,
                    );
                    let duration_s = timeline.duration_seconds().max(0.1);
                    let all_kf = collect_all_keyframe_times(track);
                    let strip = components::TimelineStrip {
                        duration_s,
                        current_time_s,
                        keyframes: &all_kf,
                        height: ROW_XS,
                    };
                    if let Some(scrub_t) = strip.show(ui, ui.id().with("mini_timeline")) {
                        actions.scrub_to = Some(scrub_t);
                    }
                });

                ui.add_space(SPACE_M);

                // ── Keyframes ──
                let kf_count = count_keyframes(track);
                components::card(ui, |ui| {
                    components::section_header(
                        ui,
                        egui_phosphor::regular::KEY,
                        "Keyframes",
                        Some(kf_count),
                    );
                    render_dope_sheet(
                        ui,
                        timeline,
                        track,
                        (current_time_s * 1000.0) as u64,
                        actions,
                    );
                });
            });
    } else {
        components::empty_state(
            ui,
            egui_phosphor::regular::MAGNIFYING_GLASS,
            "Select an actor to inspect",
            "Click an actor in the preview or Layers panel",
        );
    }
}

// ─── Actor Header ─────────────────────────────────────────────────────────

fn render_actor_header(ui: &mut egui::Ui, track: &AnimationTrack, current_time_s: f64) {
    let current_time_ms = (current_time_s * 1000.0) as u64;

    ui.horizontal(|ui| {
        if let Some(shape_pt) = &track.shape_type {
            let shape = shape_pt.evaluate(current_time_ms);
            ui.add(
                egui::Label::new(
                    RichText::new(shape_icon(shape)).size(FONT_SIZE_XL).color(AMBER),
                )
                .selectable(false),
            );
        }

        ui.add(
            egui::Label::new(
                RichText::new(&track.label)
                    .strong()
                    .size(FONT_SIZE_XL)
                    .color(TEXT_PRIMARY),
            )
            .selectable(false),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if track.first_seen_ms > 0 && track.first_seen_ms != u64::MAX {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "t = {:.2}s",
                            track.first_seen_ms as f64 / 1000.0
                        ))
                        .size(FONT_SIZE_XS)
                        .color(TEXT_MUTED),
                    )
                    .selectable(false),
                );
            }

            if let Some(shape_pt) = &track.shape_type {
                let shape = shape_pt.evaluate(current_time_ms);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{:?}", shape))
                            .size(FONT_SIZE_S)
                            .color(TEXT_MUTED),
                    )
                    .selectable(false),
                );
            }
        });
    });
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
