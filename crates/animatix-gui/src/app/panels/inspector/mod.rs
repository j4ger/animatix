use std::collections::{HashMap, HashSet};
use animatix::timeline::{AnimationTrack, Timeline, collect_all_keyframe_times};
use egui::{Color32, Pos2, RichText, ScrollArea, Vec2};

use crate::app::components::{layout, timeline};
use crate::app::icons::actor_icon_str;
use crate::app::design_tokens::*;
use crate::app::commands::{ActionQueue, Command, ShellAction, PropertyEdit, PropertyValue as GuiPropertyValue};
use crate::app::panels::panel_frame;
use crate::app::PreviewPaneState;

pub(crate) mod property_groups;
pub(crate) mod keyframe_table;
pub(crate) mod graph_editor;

use self::property_groups::*;
use self::keyframe_table::{render_dope_sheet, count_keyframes};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PropertyViewMode {
    Semantic,
    Intensity,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum KeyframeViewMode {
    List,
    Curve,
}

pub(crate) struct InspectorContext<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub selected_actors: &'a mut HashSet<String>,
    pub commands: &'a mut ActionQueue,
    pub keyframe_mode: bool,
    pub scene_dimensions: animatix::timeline::SceneDimensions,
    pub pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    pub property_view_mode: &'a mut PropertyViewMode,
    pub keyframe_view_mode: &'a mut KeyframeViewMode,
}

/// Renders the unified actor inspector panel (with frame).
pub(crate) fn inspector_panel_ui(ctx: &mut InspectorContext<'_>, ui: &mut egui::Ui) {
    panel_frame().show(ui, |ui| {
        // Scene-level inspector: when no actor is selected in a composition
        if ctx.selected_actors.is_empty() {
            if let (Some(comp), Some(active_scene)) = (ctx.composition, ctx.active_scene) {
                render_scene_inspector(ui, comp, active_scene, ctx.commands, ctx.preview);
                return;
            }
        }

        let current_time_s = ctx.preview.playback.current_time_s();
        let timeline = ctx.timeline.or_else(|| {
            let comp = ctx.composition?;
            let active_scene = ctx.active_scene?;
            let active_has_actor = ctx.selected_actors.iter().next().is_some_and(|sel| {
                comp.scenes.get(active_scene).is_some_and(|s| s.timeline.has_actor(sel))
            });
            if !active_has_actor {
                let (_, _, transition) = comp.evaluate(current_time_s);
                if transition.is_some() {
                    for (name, scene) in &comp.scenes {
                        if name != active_scene
                            && ctx.selected_actors.iter().any(|sel| scene.timeline.has_actor(sel))
                        {
                            return Some(&scene.timeline);
                        }
                    }
                }
            }
            comp.scenes.get(active_scene).map(|s| &s.timeline)
        });
        inspector_ui(
            ui,
            timeline,
            ctx.selected_actors,
            current_time_s,
            ctx.commands,
            ctx.keyframe_mode,
            ctx.scene_dimensions,
            ctx.pivot_offsets,
            ctx.property_view_mode,
            ctx.keyframe_view_mode,
        );
    });
}

/// Render scene-level inspector when no actor is selected.
fn render_scene_inspector(
    ui: &mut egui::Ui,
    composition: &animatix::composition::Composition,
    active_scene: &str,
    commands: &mut ActionQueue,
    _preview: &PreviewPaneState,
) {
    use crate::app::components::layout;

    let Some(scene) = composition.scenes.get(active_scene) else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::WARNING,
            "Scene not found",
            "The active scene no longer exists in the composition",
        );
        return;
    };

    ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // ── Scene Header ──
            let available = ui.available_width();
            let row_h = ROW_L;
            let (row_rect, _) = ui.allocate_exact_size(Vec2::new(available, row_h), egui::Sense::hover());
            ui.painter().text(
                Pos2::new(row_rect.min.x + SPACE_S, row_rect.center().y),
                egui::Align2::LEFT_CENTER,
                format!("{} {}", egui_phosphor::regular::FILM_STRIP, active_scene),
                egui::FontId::new(FONT_SIZE_XL, egui::FontFamily::Proportional),
                ACCENT_BLUE,
            );
            ui.add_space(SPACE_M);

            // ── Scene Properties ──
            layout::card(ui, |ui| {
                layout::section_header(ui, egui_phosphor::regular::WRENCH, "Properties", None);

                // Duration
                layout::labeled_row(ui, "Duration", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                    ui.add(egui::Label::new(
                        RichText::new(format!("{:.2} s", scene.duration_s))
                            .monospace()
                            .size(FONT_SIZE_S)
                            .color(TEXT_SECONDARY),
                    ).selectable(false));
                });

                // Start time
                let start_s = composition.scene_start_times.get(active_scene).copied().unwrap_or(0.0);
                layout::labeled_row(ui, "Start", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                    ui.add(egui::Label::new(
                        RichText::new(format!("{:.2} s", start_s))
                            .monospace()
                            .size(FONT_SIZE_S)
                            .color(TEXT_SECONDARY),
                    ).selectable(false));
                });

                // Background color
                let bg_color = scene.timeline.background_color_at(0);
                layout::labeled_row(ui, "Background", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                    let color_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        Vec2::new(24.0, 24.0),
                    );
                    ui.painter().rect_filled(
                        color_rect,
                        RADIUS_S,
                        egui::Color32::from_rgba_premultiplied(
                            (bg_color[0] * 255.0) as u8,
                            (bg_color[1] * 255.0) as u8,
                            (bg_color[2] * 255.0) as u8,
                            (bg_color[3] * 255.0) as u8,
                        ),
                    );
                    ui.add_space(32.0);
                    ui.add(egui::Label::new(
                        RichText::new(format!(
                            "({:.2}, {:.2}, {:.2}, {:.2})",
                            bg_color[0], bg_color[1], bg_color[2], bg_color[3]
                        ))
                        .monospace()
                        .size(FONT_SIZE_S)
                        .color(TEXT_MUTED),
                    ).selectable(false));
                });
            });

            ui.add_space(SPACE_M);

            // ── Play Edge ──
            if let Some(edge) = composition.edges.get(active_scene) {
                layout::card(ui, |ui| {
                    layout::section_header(ui, egui_phosphor::regular::ARROW_RIGHT, "Transition", None);

                    layout::labeled_row(ui, "Target", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                        ui.add(egui::Label::new(
                            RichText::new(&edge.to_scene)
                                .size(FONT_SIZE_S)
                                .color(TEXT_SECONDARY),
                        ).selectable(false));
                    });

                    layout::labeled_row(ui, "Type", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                        ui.add(egui::Label::new(
                            RichText::new(&edge.transition.id)
                                .size(FONT_SIZE_S)
                                .color(TEXT_SECONDARY),
                        ).selectable(false));
                    });

                    layout::labeled_row(ui, "Duration", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                        ui.add(egui::Label::new(
                            RichText::new(format!("{:.0} ms", edge.transition.duration_ms))
                                .monospace()
                                .size(FONT_SIZE_S)
                                .color(TEXT_SECONDARY),
                        ).selectable(false));
                    });

                    // Click to jump to target scene
                    if ui.button(
                        RichText::new(format!("{} Go to {}", egui_phosphor::regular::ARROW_RIGHT, edge.to_scene))
                            .size(FONT_SIZE_S)
                            .color(ACCENT_BLUE),
                    ).clicked() {
                        commands.push_back(ShellAction::Command(Command::SelectScene(edge.to_scene.clone())));
                    }
                });
            }

            ui.add_space(SPACE_M);

            // ── Scene List ──
            layout::card(ui, |ui| {
                layout::section_header(ui, egui_phosphor::regular::FILM_STRIP, "All Scenes", Some(composition.declaration_order.len()));
                for scene_name in &composition.declaration_order {
                    let is_active = scene_name == active_scene;
                    let response = ui.interact(
                        egui::Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), ROW_M)),
                        ui.id().with(format!("scene_list_{}", scene_name)),
                        egui::Sense::click(),
                    );
                    let bg = if is_active {
                        accent_selection()
                    } else if response.hovered() {
                        BG_HOVER
                    } else {
                        Color32::TRANSPARENT
                    };
                    if bg != Color32::TRANSPARENT {
                        ui.painter().rect_filled(response.rect, RADIUS_S, bg);
                    }
                    ui.painter().text(
                        Pos2::new(response.rect.min.x + SPACE_S, response.rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        scene_name,
                        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                        if is_active { ACCENT_BLUE } else { TEXT_SECONDARY },
                    );
                    if response.clicked() && !is_active {
                        commands.push_back(ShellAction::Command(Command::SelectScene(scene_name.clone())));
                    }
                    ui.allocate_rect(response.rect, egui::Sense::hover());
                }
            });
        });
}

// ─── Internal Entry Point ─────────────────────────────────────────────────

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
    selected_actors: &mut HashSet<String>,
    current_time_s: f64,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    scene_dimensions: animatix::timeline::SceneDimensions,
    pivot_offsets: &mut std::collections::HashMap<String, [f32; 2]>,
    property_view_mode: &mut PropertyViewMode,
    keyframe_view_mode: &mut KeyframeViewMode,
) {
    let should_reset = selected_actors
        .iter()
        .next()
        .is_some_and(|sel| timeline.is_some_and(|t| !t.has_actor(sel)));
    if should_reset {
        selected_actors.clear();
    }

    let Some(timeline) = timeline else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No timeline loaded",
            "Open or create a scene to begin",
        );
        return;
    };

    let root_nodes = timeline.root_actor_labels();
    if root_nodes.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(SPACE_XL * 3.0);
            ui.add(
                egui::Label::new(
                    RichText::new(egui_phosphor::regular::FILM_STRIP)
                        .size(layout::EMPTY_STATE_ICON_SIZE)
                        .color(TEXT_MUTED),
                )
                .selectable(false),
            );
            ui.add_space(SPACE_M);
            ui.add(
                egui::Label::new(
                    RichText::new("No actors in scene")
                        .size(FONT_SIZE_L)
                        .color(TEXT_SECONDARY),
                )
                .selectable(false),
            );
            ui.add_space(SPACE_L);
            if ui
                .button(
                    RichText::new(format!("{} Add Actor", egui_phosphor::regular::PLUS))
                        .size(FONT_SIZE_L)
                        .color(ACCENT_BLUE),
                )
                .on_hover_text("Add a new actor to the scene")
                .clicked()
            {
                let label = "rect1".to_string();
                let pos = [
                    scene_dimensions.width as f32 / 2.0,
                    scene_dimensions.height as f32 / 2.0,
                ];
                commands.push_back(ShellAction::Command(Command::CreateActor {
                    ty: super::default_actor_type().into(),
                    label,
                    position: pos,
                }));
            }
        });
        return;
    }

    if let Some(sel) = selected_actors.iter().next() {
        let Some(track) = timeline.get_track(sel) else {
            layout::empty_state(
                ui,
                egui_phosphor::regular::WARNING,
                "Actor not found",
                "The selected actor no longer exists in the timeline",
            );
            return;
        };

        let multi_count = selected_actors.len();

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if multi_count > 1 {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} actors selected", multi_count))
                                .size(FONT_SIZE_M)
                                .color(TEXT_SECONDARY),
                        );
                    });
                    ui.add_space(SPACE_S);
                }
                render_actor_header(ui, track, current_time_s, commands);
                ui.add_space(SPACE_M);

                // ── Parent ──
                render_parent_card(ui, timeline, sel, commands);
                ui.add_space(SPACE_M);

                // ── Active Properties ──
                layout::card(ui, |ui| {
                    let mut view_mode = *property_view_mode;

                    layout::section_header(
                        ui,
                        egui_phosphor::regular::WRENCH,
                        "Properties",
                        None,
                    );

                    // View-mode toggle button overlaid on the sticky header row
                    {
                        let clip = ui.clip_rect();
                        let row_top = clip.min.y + SPACE_M + 2.0 + SPACE_M; // matches header row y
                        let label = match view_mode {
                            PropertyViewMode::Semantic => format!("{} Semantic", egui_phosphor::regular::ROWS),
                            PropertyViewMode::Intensity => format!("{} Stream", egui_phosphor::regular::FIRE),
                        };
                        let btn_width = 110.0;
                        let btn_rect = egui::Rect::from_min_size(
                            egui::pos2(clip.max.x - SPACE_S - btn_width, row_top),
                            egui::Vec2::new(btn_width, ROW_S),
                        );
                        let mut btn_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(btn_rect)
                                .layout(egui::Layout::right_to_left(egui::Align::Center)),
                        );
                        let tooltip = match view_mode {
                            PropertyViewMode::Semantic => "Switch to stream view",
                            PropertyViewMode::Intensity => "Switch to semantic view",
                        };
                        if btn_ui
                            .button(RichText::new(&label).size(FONT_SIZE_XS).color(TEXT_MUTED))
                            .on_hover_text(tooltip)
                            .clicked()
                        {
                            view_mode = match view_mode {
                                PropertyViewMode::Semantic => PropertyViewMode::Intensity,
                                PropertyViewMode::Intensity => PropertyViewMode::Semantic,
                            };
                            *property_view_mode = view_mode;
                        }
                    }

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
                        match view_mode {
                            PropertyViewMode::Semantic => {
                                for group in &groups {
                                    render_property_group(
                                        ui,
                                        group,
                                        &track.label,
                                        commands,
                                        keyframe_mode,
                                        current_time_s,
                                    );
                                }
                            }
                            PropertyViewMode::Intensity => {
                                render_property_stream(ui, &groups, &track.label, commands, keyframe_mode, current_time_s, &mut view_mode);
                            }
                        }
                        // Persist any view-mode change made by the stream click handler
                        *property_view_mode = view_mode;
                    }
                });

                ui.add_space(SPACE_M);

                // ── Pivot ──
                if multi_count == 1 {
                    layout::card(ui, |ui| {
                        layout::section_header(
                            ui,
                            egui_phosphor::regular::CROSSHAIR,
                            "Pivot",
                            None,
                        );
                        let pivot = pivot_offsets.entry(sel.clone()).or_insert([0.0, 0.0]);
                        layout::labeled_row(ui, "X", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                            ui.add(egui::DragValue::new(&mut pivot[0]).speed(1.0).suffix(" px"));
                        });
                        layout::labeled_row(ui, "Y", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                            ui.add(egui::DragValue::new(&mut pivot[1]).speed(1.0).suffix(" px"));
                        });
                        if ui.button(RichText::new("Reset").size(FONT_SIZE_S).color(TEXT_MUTED))
                            .on_hover_text("Reset pivot to center")
                            .clicked()
                        {
                            *pivot = [0.0, 0.0];
                        }
                    });
                    ui.add_space(SPACE_M);
                }

                // ── Container Children ──
                if timeline.container_metadata().contains_key(sel) {
                    layout::card(ui, |ui| {
                        layout::section_header(
                            ui,
                            egui_phosphor::regular::ROWS,
                            "Children",
                            Some(timeline.layout_children_for(sel).len()),
                        );
                        let time_ms = (current_time_s * 1000.0) as u64;
                        let order = timeline.get_child_order(sel, time_ms);
                        render_container_children(ui, sel, &order, commands, keyframe_mode);
                    });
                    ui.add_space(SPACE_M);
                }

                // ── Mini Timeline ──
                layout::card(ui, |ui| {
                    layout::section_header(
                        ui,
                        egui_phosphor::regular::CLOCK,
                        "Timeline",
                        None,
                    );
                    let duration_s = timeline.duration_seconds().max(0.1);
                    let all_kf = collect_all_keyframe_times(track);
                    let strip = timeline::TimelineStrip {
                        duration_s,
                        current_time_s,
                        keyframes: &all_kf,
                        height: ROW_XS,
                    };
                    if let Some(scrub_t) = strip.show(ui) {
                        commands.push_back(ShellAction::Command(Command::ScrubTo(scrub_t)));
                    }
                });

                ui.add_space(SPACE_M);

                // ── Keyframes ──
                let kf_count = count_keyframes(track);
                layout::card(ui, |ui| {
                    let mut kf_view = *keyframe_view_mode;

                    layout::section_header(
                        ui,
                        egui_phosphor::regular::KEY,
                        "Keyframes",
                        Some(kf_count),
                    );

                    // View-mode toggle button overlaid on the sticky header row
                    {
                        let clip = ui.clip_rect();
                        let row_top = clip.min.y + SPACE_M + 2.0 + SPACE_M;
                        let label = match kf_view {
                            KeyframeViewMode::List => format!("{} List", egui_phosphor::regular::LIST),
                            KeyframeViewMode::Curve => format!("{} Curve", egui_phosphor::regular::CHART_LINE_UP),
                        };
                        let btn_width = 90.0;
                        let btn_rect = egui::Rect::from_min_size(
                            egui::pos2(clip.max.x - SPACE_S - btn_width, row_top),
                            egui::Vec2::new(btn_width, ROW_S),
                        );
                        let mut btn_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(btn_rect)
                                .layout(egui::Layout::right_to_left(egui::Align::Center)),
                        );
                        let kf_tooltip = match kf_view {
                            KeyframeViewMode::List => "Switch to curve view",
                            KeyframeViewMode::Curve => "Switch to list view",
                        };
                        if btn_ui
                            .button(RichText::new(&label).size(FONT_SIZE_XS).color(TEXT_MUTED))
                            .on_hover_text(kf_tooltip)
                            .clicked()
                        {
                            kf_view = match kf_view {
                                KeyframeViewMode::List => KeyframeViewMode::Curve,
                                KeyframeViewMode::Curve => KeyframeViewMode::List,
                            };
                            *keyframe_view_mode = kf_view;
                        }
                    }

                    match kf_view {
                        KeyframeViewMode::List => {
                            render_dope_sheet(
                                ui,
                                timeline,
                                track,
                                (current_time_s * 1000.0) as u64,
                                sel,
                                commands,
                            );
                        }
                        KeyframeViewMode::Curve => {
                            // Show F-curve for the first float property with keyframes
                            let indices = animatix::timeline::allowed_property_indices(track.kind);
                            let mut shown = false;
                            for &idx in &indices {
                                let schema = &animatix::timeline::PROPERTY_REGISTRY[idx];
                                if schema.value_type == animatix::timeline::ValueType::F32
                                    && animatix::timeline::property_has_keyframes(track, schema.field)
                                {
                                    graph_editor::render_fcurve(
                                        ui,
                                        track,
                                        schema.name,
                                        timeline.duration_seconds(),
                                        current_time_s,
                                        commands,
                                    );
                                    shown = true;
                                    break;
                                }
                            }
                            if !shown {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(SPACE_M);
                                    ui.label(RichText::new("No float property keyframes to graph").size(FONT_SIZE_S).color(TEXT_MUTED));
                                });
                            }
                        }
                    }
                });
            });
    } else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::MAGNIFYING_GLASS,
            "Select an actor to inspect",
            "Click an actor in the preview or Layers panel",
        );
    }
}

// ─── Property Stream (intensity-sorted flat list) ─────────────────────────

fn render_property_stream(
    ui: &mut egui::Ui,
    groups: &[PropertyGroup],
    _actor_label: &str,
    _commands: &mut ActionQueue,
    _keyframe_mode: bool,
    _current_time_s: f64,
    property_view_mode: &mut PropertyViewMode,
) {
    // Flatten all entries and sort by keyframe count descending
    let mut all_entries: Vec<(&PropertyGroup, &PropertyEntry)> = Vec::new();
    for group in groups {
        for entry in &group.properties {
            all_entries.push((group, entry));
        }
    }
    all_entries.sort_by(|a, b| b.1.keyframe_count.cmp(&a.1.keyframe_count));

    // Find max keyframe count for bar scaling
    let max_kf = all_entries.iter().map(|(_, e)| e.keyframe_count).max().unwrap_or(1).max(1);

    ui.spacing_mut().item_spacing = Vec2::new(0.0, SPACE_XS);
    for (group, entry) in &all_entries {
        let row_height = INSPECTOR_ROW_HEIGHT;
        let available = ui.available_width();
        let (row_rect, row_response) =
            ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

        if row_response.hovered() {
            ui.painter().rect_filled(row_rect, 0.0, BG_HOVER);
        }

        let baseline_y = row_rect.center().y;

        // Intensity bar (left side)
        let bar_max_w = 60.0;
        let bar_w = if entry.keyframe_count > 0 {
            (entry.keyframe_count as f32 / max_kf as f32 * bar_max_w).max(4.0)
        } else {
            0.0
        };
        if bar_w > 0.0 {
            let bar_rect = egui::Rect::from_min_max(
                egui::pos2(row_rect.min.x + SPACE_S, baseline_y - 3.0),
                egui::pos2(row_rect.min.x + SPACE_S + bar_w, baseline_y + 3.0),
            );
            let bar_color = if entry.keyframe_count >= max_kf / 2 {
                AMBER
            } else {
                TEXT_MUTED
            };
            ui.painter().rect_filled(bar_rect, RADIUS_S, bar_color);
        }

        // Icon + property name
        let name_x = row_rect.min.x + SPACE_S + bar_max_w + SPACE_S;
        ui.painter().text(
            egui::pos2(name_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            format!("{} {}", group.icon, entry.name),
            egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_SECONDARY,
        );

        // Current value (middle area)
        let value_text = format_property_value(&entry.kind);
        let value_x = name_x + 100.0;
        if !value_text.is_empty() {
            ui.painter().text(
                egui::pos2(value_x, baseline_y),
                egui::Align2::LEFT_CENTER,
                &value_text,
                egui::FontId::monospace(FONT_SIZE_XS),
                TEXT_MUTED,
            );
        }

        // Keyframe count badge (right)
        if entry.keyframe_count > 0 {
            let count_text = format!("{} {}", egui_phosphor::regular::DIAMOND, entry.keyframe_count);
            ui.painter().text(
                egui::pos2(row_rect.max.x - SPACE_S, baseline_y),
                egui::Align2::RIGHT_CENTER,
                count_text,
                egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
                TEXT_MUTED,
            );
        }

        // Click to jump to the property in semantic view
        if row_response.clicked() {
            *property_view_mode = PropertyViewMode::Semantic;
        }
    }
    ui.spacing_mut().item_spacing = Vec2::new(0.0, SPACE_S);

    // Divider between animated and non-animated
    let animated_count = all_entries.iter().filter(|(_, e)| e.keyframe_count > 0).count();
    if animated_count > 0 && animated_count < all_entries.len() {
        ui.add_space(SPACE_S);
        let divider_rect = ui.available_rect_before_wrap();
        if divider_rect.width() > 0.0 {
            ui.painter().line_segment(
                [
                    egui::pos2(divider_rect.min.x + SPACE_S, divider_rect.min.y + 4.0),
                    egui::pos2(divider_rect.max.x - SPACE_S, divider_rect.min.y + 4.0),
                ],
                egui::Stroke::new(STROKE_WIDTH, BORDER),
            );
        }
        ui.add_space(SPACE_S);
    }
}

// ─── Actor Header ─────────────────────────────────────────────────────────

fn render_actor_header(
    ui: &mut egui::Ui,
    track: &AnimationTrack,
    current_time_s: f64,
    commands: &mut ActionQueue,
) {
    let current_time_ms = (current_time_s * 1000.0) as u64;
    let available = ui.available_width();
    let row_h = ROW_L;
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(available, row_h), egui::Sense::hover());

    // ── Left side: icon + name ──
    let left_rect = egui::Rect::from_min_max(
        row_rect.min,
        egui::pos2(row_rect.center().x, row_rect.max.y),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(left_rect), |ui| {
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
            |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(actor_icon_str(track.kind))
                            .size(FONT_SIZE_XL)
                            .color(AMBER),
                    )
                    .selectable(false),
                );
                ui.add_space(SPACE_S);

                // Actor label (click to rename)
                let edit_id = ui.id().with("actor_name_edit");
                let is_editing: bool = ui.data(|d| d.get_temp(edit_id)).unwrap_or(false);
                let mut edit_buffer: String =
                    ui.data(|d| d.get_temp(edit_id.with("buf")))
                        .unwrap_or_else(|| track.label.clone());

                if is_editing {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut edit_buffer)
                            .font(egui::FontId::new(FONT_SIZE_XL, egui::FontFamily::Proportional))
                            .text_color(TEXT_PRIMARY)
                            .desired_width(120.0),
                    );
                    response.request_focus();
                    if response.lost_focus() {
                        ui.data_mut(|d| d.insert_temp(edit_id, false));
                        if edit_buffer != track.label && !edit_buffer.is_empty() {
                            commands.push_back(ShellAction::Command(Command::RenameActor {
                                old_label: track.label.clone(),
                                new_label: edit_buffer.clone(),
                            }));
                        }
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        ui.data_mut(|d| d.insert_temp(edit_id, false));
                        edit_buffer = track.label.clone();
                    }
                    ui.data_mut(|d| d.insert_temp(edit_id.with("buf"), edit_buffer));
                } else {
                    let label_response = ui.add(
                        egui::Label::new(
                            RichText::new(&track.label)
                                .size(FONT_SIZE_XL)
                                .color(TEXT_PRIMARY),
                        )
                        .selectable(false)
                        .sense(egui::Sense::click()),
                    );
                    if label_response.clicked() {
                        ui.data_mut(|d| {
                            d.insert_temp(edit_id, true);
                            d.insert_temp(edit_id.with("buf"), track.label.clone());
                        });
                    }
                }
            },
        );
    });

    // ── Right side: shape type + first seen time ──
    let right_rect = egui::Rect::from_min_max(
        egui::pos2(row_rect.center().x, row_rect.min.y),
        row_rect.max,
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(right_rect), |ui| {
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(false),
            |ui| {
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
                    ui.add_space(SPACE_M);
                }

                if let Some(shape_pt) = &track.shape_type {
                    let shape = shape_pt.evaluate(current_time_ms);
                    ui.add(
                        egui::Label::new(
                            RichText::new(shape.to_string())
                                .size(FONT_SIZE_S)
                                .color(TEXT_MUTED),
                        )
                        .selectable(false),
                    );
                }
            },
        );
    });
}



// ─── Parent Card ──────────────────────────────────────────────────────────

fn render_parent_card(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    actor: &str,
    commands: &mut ActionQueue,
) {
    use crate::app::components::layout;

    // Find current parent
    let current_parent = timeline.tracks().iter()
        .find(|(_, track)| track.children.iter().any(|c| c == actor))
        .map(|(label, _)| label.clone());

    layout::card(ui, |ui| {
        layout::section_header(ui, egui_phosphor::regular::TREE_STRUCTURE, "Hierarchy", None);

        layout::labeled_row(ui, "Parent", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
            let all_labels: Vec<String> = timeline.tracks()
                .iter()
                .map(|(label, _)| label.clone())
                .filter(|label| label != actor)
                .collect();

            let current_display = current_parent.as_deref().unwrap_or("None (root)");
            egui::ComboBox::from_id_salt(ui.id().with("parent_dropdown"))
                .selected_text(current_display)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    if ui.selectable_label(current_parent.is_none(), "None (root)").clicked() {
                        if current_parent.is_some() {
                            commands.push_back(ShellAction::Command(Command::ReparentActor {
                                actor: actor.to_string(),
                                new_parent: None,
                            }));
                        }
                    }
                    for label in &all_labels {
                        let is_selected = current_parent.as_deref() == Some(label.as_str());
                        if ui.selectable_label(is_selected, label).clicked() && !is_selected {
                            commands.push_back(ShellAction::Command(Command::ReparentActor {
                                actor: actor.to_string(),
                                new_parent: Some(label.clone()),
                            }));
                        }
                    }
                });
        });
    });
}

// ─── Container Children Reorder ───────────────────────────────────────────

fn render_container_children(
    ui: &mut egui::Ui,
    container: &str,
    order: &[String],
    commands: &mut ActionQueue,
    keyframe_mode: bool,
) {
    ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, SPACE_S);
    for (i, label) in order.iter().enumerate() {
        let row_id = ui.id().with(format!("child_{}", i));
        let available = ui.available_width();
        let (row_rect, _) = ui.allocate_exact_size(Vec2::new(available, ROW_M), egui::Sense::hover());

        // Background
        let bg = if ui.rect_contains_pointer(row_rect) {
            BG_HOVER
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(row_rect, RADIUS_M, bg);
        }

        let baseline_y = row_rect.center().y;
        let mut cursor_x = row_rect.min.x + SPACE_M;

        // Index badge
        let badge_text = format!("{}", i + 1);
        crate::app::utils::draw_badge(
            ui.painter(),
            egui::pos2(cursor_x, baseline_y - 9.0),
            &badge_text,
            BG_WIDGET,
            TEXT_MUTED,
            None,
        );
        cursor_x += ROW_S + SPACE_S;

        // Label
        ui.painter().text(
            egui::pos2(cursor_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_SECONDARY,
        );

        // Up / Down buttons (right-aligned)
        let btn_size = Vec2::new(ROW_S, ROW_S);
        let btn_y = row_rect.min.y + (row_rect.height() - btn_size.y) * 0.5;
        let mut btn_x = row_rect.max.x - SPACE_S - btn_size.x;

        // Down button
        let down_rect = egui::Rect::from_min_size(egui::pos2(btn_x, btn_y), btn_size);
        let down_resp = ui.interact(down_rect, row_id.with("down"), egui::Sense::click())
            .on_hover_text("Move down");
        let down_color = if i + 1 >= order.len() {
            TEXT_DISABLED
        } else if down_resp.hovered() {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        };
        ui.painter().text(
            down_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::CARET_DOWN,
            egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            down_color,
        );
        btn_x -= btn_size.x + SPACE_XS;

        // Up button
        let up_rect = egui::Rect::from_min_size(egui::pos2(btn_x, btn_y), btn_size);
        let up_resp = ui.interact(up_rect, row_id.with("up"), egui::Sense::click())
            .on_hover_text("Move up");
        let up_color = if i == 0 {
            TEXT_DISABLED
        } else if up_resp.hovered() {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        };
        ui.painter().text(
            up_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::CARET_UP,
            egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            up_color,
        );

        // Emit reorder on click
        if up_resp.clicked() && i > 0 {
            let mut new_order = order.to_vec();
            new_order.swap(i, i - 1);
            commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                actor: container.to_string(),
                property: "child_order".into(),
                value: GuiPropertyValue::StringList(new_order),
                create_keyframe: keyframe_mode,
            })));
        }
        if down_resp.clicked() && i + 1 < order.len() {
            let mut new_order = order.to_vec();
            new_order.swap(i, i + 1);
            commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                actor: container.to_string(),
                property: "child_order".into(),
                value: GuiPropertyValue::StringList(new_order),
                create_keyframe: keyframe_mode,
            })));
        }
    }
}

/// Format a property value for display in the intensity stream view.
fn format_property_value(kind: &PropertyKind) -> String {
    match kind {
        PropertyKind::Vec2 { x, y } => format!("({:.1}, {:.1})", x, y),
        PropertyKind::Float(v) => format!("{:.2}", v),
        PropertyKind::U32(v) => format!("{}", v),
        PropertyKind::Color(rgba) => {
            let r = (rgba[0] * 255.0).round() as u8;
            let g = (rgba[1] * 255.0).round() as u8;
            let b = (rgba[2] * 255.0).round() as u8;
            if rgba[3] >= 0.999 {
                format!("#{:02x}{:02x}{:02x}", r, g, b)
            } else {
                format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, (rgba[3] * 255.0) as u8)
            }
        }
        PropertyKind::Text(s) => {
            if s.len() > 16 {
                format!("{}…", &s[..15])
            } else {
                s.clone()
            }
        }
    }
}
