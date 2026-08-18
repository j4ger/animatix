use std::collections::{HashMap, HashSet};

use animatix::timeline::{AnimationTrack, Timeline, collect_all_keyframe_times};
use egui::{Color32, Pos2, RichText, ScrollArea, Vec2};
use eparts::widget::UiExt;

use crate::app::PreviewPaneState;
use crate::app::commands::{
    ActionQueue, ActorCommand, DocumentCommand, PlaybackCommand, PropertyEdit,
    PropertyValue as GuiPropertyValue, SceneCommand,
};
use crate::app::components::easing_curve_editor::EasingCurveState;
use crate::app::components::{Badge, TabBar, text_tooltip};
use crate::app::components::{easing_curve_editor, layout, timeline};
use crate::app::design_tokens::spatial::inspector::INPUT_WIDTH_FLOAT as INSPECTOR_INPUT_WIDTH_FLOAT;
use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_S, STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::icons::actor_icon_str;
use crate::app::panels::panel_frame;
use crate::app::utils::text as app_text;

pub(crate) mod graph_editor;
pub(crate) mod keyframe_table;
pub(crate) mod property_groups;
pub(crate) mod spreadsheet;

use self::keyframe_table::{count_keyframes, render_dope_sheet};
use self::property_groups::*;
use self::spreadsheet::render_property_spreadsheet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PropertyViewMode {
    Semantic,
    Intensity,
    Spreadsheet,
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
        let timeline = ctx.timeline;
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
            ctx.active_scene,
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
    let theme = eparts::theme(ui);
    let sp = spatial(ui);

    let Some(scene) = composition.scenes.get(active_scene) else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::WARNING,
            "Scene not found",
            "The active scene no longer exists in the composition",
        );
        return;
    };

    ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        // ── Scene Header ──
        let available = ui.available_width();
        let row_h = sp.base.row_l;
        let (row_rect, _) =
            ui.allocate_exact_size(Vec2::new(available, row_h), egui::Sense::hover());
        ui.painter().text(
            Pos2::new(row_rect.min.x + sp.base.space_2, row_rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{} {}", egui_phosphor::regular::FILM_STRIP, active_scene),
            TextRole::Heading.font_id(),
            theme.accent.primary,
        );
        ui.add_space(sp.base.space_3);

        // ── Scene Properties ──
        layout::group_box(ui, format!("{} Properties", egui_phosphor::regular::WRENCH), |ui| {
            // Duration (editable — explicit or inferred from timeline keyframes)
            layout::labeled_row(ui, "Duration", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                let is_explicit = scene.explicit_duration_s.is_some();
                let mut duration_val = scene.duration_s;
                let drag = ui.add(
                    egui::DragValue::new(&mut duration_val)
                        .speed(0.1)
                        .suffix(" s")
                        .max_decimals(2)
                        .range(0.01..=600.0),
                );
                let tooltip = if is_explicit {
                    "Explicit duration (click ⨯ to revert to auto)"
                } else {
                    "Auto-detected from keyframes (drag to set explicit)"
                };
                let changed = drag.changed();
                text_tooltip(ui, ui.id().with("scene_duration_tooltip"), &drag, tooltip);
                if is_explicit {
                    // Small button to remove explicit duration
                    let revert = ui.small_button("⨯");
                    text_tooltip(
                        ui,
                        ui.id().with("scene_duration_revert_tooltip"),
                        &revert,
                        "Revert to auto-detected duration",
                    );
                    if revert.clicked() {
                        commands.push_back(
                            SceneCommand::SetSceneDuration {
                                scene: active_scene.to_string(),
                                duration_s: None,
                            }
                            .into(),
                        );
                    }
                }
                // Only emit edit if value changed meaningfully
                if changed && (duration_val - scene.duration_s).abs() > 0.001 {
                    commands.push_back(
                        SceneCommand::SetSceneDuration {
                            scene: active_scene.to_string(),
                            duration_s: Some(duration_val),
                        }
                        .into(),
                    );
                }
            });

            // Start time
            let start_s = composition.scene_start_times.get(active_scene).copied().unwrap_or(0.0);
            layout::labeled_row(ui, "Start", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{:.2} s", start_s))
                            .monospace()
                            .size(TextRole::BodyS.size())
                            .color(theme.text.secondary),
                    )
                    .selectable(false),
                );
            });

            // Background color
            let bg_color = scene.timeline.background_color_at(0);
            layout::labeled_row(ui, "Background", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                let center_y = ui.cursor().min.y + ui.available_height() / 2.0;
                let color_rect = egui::Rect::from_center_size(
                    egui::pos2(ui.cursor().min.x + 12.0, center_y),
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
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "({:.2}, {:.2}, {:.2}, {:.2})",
                            bg_color[0], bg_color[1], bg_color[2], bg_color[3]
                        ))
                        .monospace()
                        .size(TextRole::BodyS.size())
                        .color(theme.text.muted),
                    )
                    .selectable(false),
                );
            });
        });

        ui.add_space(sp.base.space_3);

        // ── Play Edge ──
        if let Some(edge) = composition.edges.get(active_scene) {
            layout::group_box(
                ui,
                format!("{} Transition", egui_phosphor::regular::ARROW_RIGHT),
                |ui| {
                    // Target scene dropdown
                    let other_scenes: Vec<&String> = composition
                        .declaration_order
                        .iter()
                        .filter(|s| *s != active_scene)
                        .collect();
                    layout::labeled_row(ui, "Target", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                        egui::ComboBox::from_id_salt(ui.id().with("transition_target"))
                            .selected_text(&edge.to_scene)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for scene_name in &other_scenes {
                                    if ui
                                        .stable_selectable_label(
                                            *scene_name == &edge.to_scene,
                                            *scene_name,
                                        )
                                        .clicked()
                                    {
                                        commands.push_back(
                                            SceneCommand::SetPlayTarget {
                                                from_scene: active_scene.to_string(),
                                                target: Some((*scene_name).clone()),
                                            }
                                            .into(),
                                        );
                                    }
                                }
                            });
                    });

                    // Transition type dropdown
                    let registry = animatix_syntax::transition_registry::REGISTRY;
                    layout::labeled_row(ui, "Type", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                        egui::ComboBox::from_id_salt(ui.id().with("transition_type"))
                            .selected_text(animatix_syntax::transition_registry::display_name(
                                &edge.transition.id,
                            ))
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for def in registry {
                                    if ui
                                        .stable_selectable_label(
                                            def.id == edge.transition.id,
                                            def.display_name,
                                        )
                                        .clicked()
                                        && def.id != edge.transition.id
                                    {
                                        commands.push_back(
                                            SceneCommand::SetTransition {
                                                from_scene: active_scene.to_string(),
                                                transition: animatix_syntax::ast::Transition {
                                                    id: def.id.to_string(),
                                                    duration_ms: edge.transition.duration_ms,
                                                    easing: edge.transition.easing,
                                                },
                                            }
                                            .into(),
                                        );
                                    }
                                }
                            });
                    });

                    // Duration
                    let mut duration_ms = edge.transition.duration_ms as f64;
                    layout::labeled_row(ui, "Duration", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                        ui.add(
                            egui::DragValue::new(&mut duration_ms)
                                .speed(10.0)
                                .suffix(" ms")
                                .max_decimals(0),
                        );
                    });
                    let new_duration_ms = duration_ms.round() as u64;
                    if new_duration_ms != edge.transition.duration_ms {
                        commands.push_back(
                            SceneCommand::SetTransition {
                                from_scene: active_scene.to_string(),
                                transition: animatix_syntax::ast::Transition {
                                    id: edge.transition.id.clone(),
                                    duration_ms: new_duration_ms,
                                    easing: edge.transition.easing,
                                },
                            }
                            .into(),
                        );
                    }

                    // Easing dropdown
                    let mut new_custom_easing: Option<animatix_syntax::easing::Easing> = None;
                    layout::labeled_row(ui, "Easing", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                        let current_easing = edge.transition.easing;
                        let current_name = animatix_syntax::easing::EASING_REGISTRY
                            .iter()
                            .find(|(id, _)| {
                                animatix_syntax::easing::parse_easing_name(id)
                                    .unwrap_or(animatix_syntax::easing::Easing::Linear)
                                    == current_easing
                            })
                            .map(|(_, name)| *name)
                            .unwrap_or("Linear");
                        egui::ComboBox::from_id_salt(ui.id().with("transition_easing"))
                            .selected_text(current_name)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for &(id_str, display_name) in
                                    animatix_syntax::easing::EASING_REGISTRY
                                {
                                    let variant =
                                        animatix_syntax::easing::parse_easing_name(id_str)
                                            .unwrap_or(animatix_syntax::easing::Easing::Linear);
                                    if ui
                                        .stable_selectable_label(
                                            variant == current_easing,
                                            display_name,
                                        )
                                        .clicked()
                                        && variant != current_easing
                                    {
                                        new_custom_easing = Some(variant);
                                    }
                                }
                            });
                    });
                    if let Some(variant) = new_custom_easing {
                        commands.push_back(
                            SceneCommand::SetTransition {
                                from_scene: active_scene.to_string(),
                                transition: animatix_syntax::ast::Transition {
                                    id: edge.transition.id.clone(),
                                    duration_ms: edge.transition.duration_ms,
                                    easing: variant,
                                },
                            }
                            .into(),
                        );
                    }

                    // Custom easing curve editor
                    if let animatix_syntax::easing::Easing::CubicBezier(cp) = edge.transition.easing
                    {
                        let state = EasingCurveState::from_array(cp);
                        if let Some(new_state) = easing_curve_editor::easing_curve_editor(ui, state)
                        {
                            commands.push_back(
                                SceneCommand::SetTransition {
                                    from_scene: active_scene.to_string(),
                                    transition: animatix_syntax::ast::Transition {
                                        id: edge.transition.id.clone(),
                                        duration_ms: edge.transition.duration_ms,
                                        easing: animatix_syntax::easing::Easing::CubicBezier(
                                            new_state.to_array(),
                                        ),
                                    },
                                }
                                .into(),
                            );
                        }
                    }

                    // Click to jump to target scene
                    if ui
                        .button(
                            RichText::new(format!(
                                "{} Go to {}",
                                egui_phosphor::regular::ARROW_RIGHT,
                                edge.to_scene
                            ))
                            .size(TextRole::BodyS.size())
                            .color(theme.accent.primary),
                        )
                        .clicked()
                    {
                        commands.push_back(SceneCommand::SelectScene(edge.to_scene.clone()).into());
                    }
                },
            );
        }

        ui.add_space(sp.base.space_3);

        // ── Scene List ──
        layout::card(ui, |ui| {
            layout::section_header(
                ui,
                egui_phosphor::regular::FILM_STRIP,
                "All Scenes",
                Some(composition.declaration_order.len()),
            );
            for scene_name in &composition.declaration_order {
                let is_active = scene_name == active_scene;
                let response = ui.interact(
                    egui::Rect::from_min_size(
                        ui.cursor().min,
                        Vec2::new(ui.available_width(), sp.base.row_m),
                    ),
                    ui.id().with(format!("scene_list_{}", scene_name)),
                    egui::Sense::click(),
                );
                let bg = if is_active {
                    theme.accent.selection
                } else if response.hovered() {
                    theme.surface.hover
                } else {
                    Color32::TRANSPARENT
                };
                if bg != Color32::TRANSPARENT {
                    ui.painter().rect_filled(response.rect, RADIUS_S, bg);
                }
                ui.painter().text(
                    Pos2::new(response.rect.min.x + sp.base.space_2, response.rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    scene_name,
                    TextRole::BodyS.font_id(),
                    if is_active {
                        theme.accent.primary
                    } else {
                        theme.text.secondary
                    },
                );
                if response.clicked() && !is_active {
                    commands.push_back(SceneCommand::SelectScene(scene_name.clone()).into());
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
    active_scene: Option<&str>,
) {
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
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
            ui.add_space(sp.base.space_5 * 3.0);
            ui.add(
                egui::Label::new(
                    RichText::new(egui_phosphor::regular::FILM_STRIP)
                        .size(layout::EMPTY_STATE_ICON_SIZE)
                        .color(theme.text.muted),
                )
                .selectable(false),
            );
            ui.add_space(sp.base.space_3);
            ui.add(
                egui::Label::new(
                    RichText::new("No actors in scene")
                        .size(TextRole::Title.size())
                        .color(theme.text.secondary),
                )
                .selectable(false),
            );
            ui.add_space(sp.base.space_4);
            let add_actor_btn = ui.button(
                RichText::new(format!("{} Add Actor", egui_phosphor::regular::PLUS))
                    .size(TextRole::Title.size())
                    .color(theme.accent.primary),
            );
            text_tooltip(
                ui,
                add_actor_btn.id.with("add_actor_tip"),
                &add_actor_btn,
                "Add a new actor to the scene",
            );
            if add_actor_btn.clicked() {
                let label = "rect1".to_string();
                let pos = [
                    scene_dimensions.width as f32 / 2.0,
                    scene_dimensions.height as f32 / 2.0,
                ];
                commands.push_back(
                    ActorCommand::CreateActor {
                        ty: super::default_actor_type().into(),
                        label,
                        position: pos,
                        props: vec![],
                    }
                    .into(),
                );
            }
        });
        return;
    }

    // ── Spreadsheet mode: show all actors in a table ──
    if *property_view_mode == PropertyViewMode::Spreadsheet {
        render_property_spreadsheet(
            ui,
            Some(timeline),
            current_time_s,
            selected_actors,
            commands,
            scene_dimensions,
            property_view_mode,
        );
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

        if multi_count > 1 {
            // ── Multi-selection: show info card instead of single-actor properties ──
            ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                layout::card(ui, |ui| {
                    layout::section_header(
                        ui,
                        egui_phosphor::regular::USERS,
                        &format!("{} actors selected", multi_count),
                        None,
                    );
                    ui.add_space(sp.base.space_2);
                    ui.label(
                        RichText::new("Multi-selected — drag/nudge in preview applies to all. Select a single actor to edit properties.")
                            .size(TextRole::Micro.size())
                            .color(theme.text.muted),
                    );
                    ui.add_space(sp.base.space_1);
                    let names: Vec<&str> = selected_actors.iter().map(String::as_str).collect();
                    ui.label(
                        RichText::new(names.join(", "))
                            .size(TextRole::Micro.size())
                            .color(theme.text.muted),
                    );
                });
            });
            return;
        }

        // Single-actor selection
        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            render_actor_header(ui, track, current_time_s, commands);
            ui.add_space(sp.base.space_3);

            // ── Parent ──
            render_parent_card(ui, timeline, sel, commands);
            ui.add_space(sp.base.space_3);

            // ── Referenced Assets ──
            let referenced_assets: Vec<String> =
                timeline.asset_cache().assets_for(sel).cloned().collect();
            if !referenced_assets.is_empty() {
                layout::card(ui, |ui| {
                    layout::section_header(ui, egui_phosphor::regular::FOLDER, "Assets", None);
                    for path in referenced_assets {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&path)
                                    .monospace()
                                    .size(TextRole::BodyS.size())
                                    .color(theme.text.secondary),
                            )
                            .selectable(false),
                        );
                    }
                });
                ui.add_space(sp.base.space_3);
            }

            // ── Active Properties ──
            layout::card(ui, |ui| {
                let mut view_mode = *property_view_mode;

                let header_top = ui.cursor().min.y;
                layout::section_header(ui, egui_phosphor::regular::WRENCH, "Properties", None);

                // View-mode segmented control
                {
                    let right = ui.clip_rect().max.x;
                    let sticky_top = header_top.max(ui.clip_rect().min.y);
                    let row_top = sticky_top + sp.base.space_2 + 2.0 + sp.base.space_2;
                    let seg_count = 3;
                    let seg_width = 80.0;
                    let total_w = seg_count as f32 * seg_width;
                    let seg_rect = egui::Rect::from_min_size(
                        egui::pos2(right - sp.base.space_2 - total_w, row_top),
                        egui::Vec2::new(total_w, sp.base.component.pill_tab_height),
                    );
                    let mut seg_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(seg_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    let modes = [
                        PropertyViewMode::Semantic,
                        PropertyViewMode::Spreadsheet,
                        PropertyViewMode::Intensity,
                    ];
                    let labels = ["Grouped", "Table", "Intensity"];
                    let mut selected =
                        modes.iter().position(|mode| *mode == view_mode).unwrap_or(0);
                    TabBar::new(seg_ui.id().with("property_view_tabs"), &mut selected, &labels)
                        .show(&mut seg_ui);
                    if let Some(mode) = modes.get(selected).copied() {
                        *property_view_mode = mode;
                    }
                }

                let current_time_ms = (current_time_s * 1000.0) as u64;
                let groups = build_property_groups(timeline, track, current_time_ms);
                if groups.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(sp.base.space_3);
                        ui.add(
                            egui::Label::new(
                                RichText::new("No editable properties")
                                    .size(TextRole::Body.size())
                                    .color(theme.text.muted),
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
                                    active_scene,
                                );
                            }
                        },
                        PropertyViewMode::Intensity => {
                            render_property_stream(
                                ui,
                                &groups,
                                &track.label,
                                commands,
                                keyframe_mode,
                                current_time_s,
                                &mut view_mode,
                            );
                        },
                        PropertyViewMode::Spreadsheet => {
                            // Drop out of the card context for spreadsheet view
                            // (spreadsheet renders its own full-width layout)
                        },
                    }
                    // Persist any view-mode change made by the stream click handler
                    *property_view_mode = view_mode;
                }
            });

            ui.add_space(sp.base.space_3);

            // ── Pivot ──
            if multi_count == 1 {
                layout::group_box(
                    ui,
                    format!("{} Pivot", egui_phosphor::regular::CROSSHAIR),
                    |ui| {
                        let pivot = pivot_offsets.entry(sel.clone()).or_insert([0.0, 0.0]);
                        layout::labeled_row(ui, "X", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                            ui.add(egui::DragValue::new(&mut pivot[0]).speed(1.0).suffix(" px"));
                        });
                        layout::labeled_row(ui, "Y", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                            ui.add(egui::DragValue::new(&mut pivot[1]).speed(1.0).suffix(" px"));
                        });
                        let reset_pivot = ui.button(
                            RichText::new("Reset")
                                .size(TextRole::BodyS.size())
                                .color(theme.text.muted),
                        );
                        text_tooltip(
                            ui,
                            ui.id().with(("reset_pivot_tooltip", sel)),
                            &reset_pivot,
                            "Reset pivot to center",
                        );
                        if reset_pivot.clicked() {
                            *pivot = [0.0, 0.0];
                        }
                    },
                );
                ui.add_space(sp.base.space_3);
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
                ui.add_space(sp.base.space_3);
            }

            // ── Mini Timeline ──
            layout::group_box(ui, format!("{} Timeline", egui_phosphor::regular::CLOCK), |ui| {
                let duration_s = timeline.duration_seconds().max(0.1);
                let all_kf = collect_all_keyframe_times(track);
                let strip = timeline::TimelineStrip {
                    duration_s,
                    current_time_s,
                    keyframes: &all_kf,
                    height: sp.base.row_xs,
                };
                if let Some(scrub_t) = strip.show(ui) {
                    commands.push_back(PlaybackCommand::ScrubTo(scrub_t).into());
                }
            });

            ui.add_space(sp.base.space_3);

            // ── Keyframes ──
            let kf_count = count_keyframes(track);
            layout::card(ui, |ui| {
                let kf_view = *keyframe_view_mode;

                let header_top = ui.cursor().min.y;
                layout::section_header(
                    ui,
                    egui_phosphor::regular::KEY,
                    "Keyframes",
                    Some(kf_count),
                );

                // View-mode segmented control
                {
                    let right = ui.clip_rect().max.x;
                    let sticky_top = header_top.max(ui.clip_rect().min.y);
                    let row_top = sticky_top + sp.base.space_2 + 2.0 + sp.base.space_2;
                    let seg_count = 2;
                    let seg_width = 80.0;
                    let total_w = seg_count as f32 * seg_width;
                    let seg_rect = egui::Rect::from_min_size(
                        egui::pos2(right - sp.base.space_2 - total_w, row_top),
                        egui::Vec2::new(total_w, sp.base.component.pill_tab_height),
                    );
                    let mut seg_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(seg_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    let modes = [KeyframeViewMode::List, KeyframeViewMode::Curve];
                    let labels = ["List", "Curve"];
                    let mut selected = modes.iter().position(|mode| *mode == kf_view).unwrap_or(0);
                    TabBar::new(seg_ui.id().with("keyframe_view_tabs"), &mut selected, &labels)
                        .show(&mut seg_ui);
                    if let Some(mode) = modes.get(selected).copied() {
                        *keyframe_view_mode = mode;
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
                            active_scene,
                        );
                    },
                    KeyframeViewMode::Curve => {
                        graph_editor::render_multi_fcurve(
                            ui,
                            track,
                            timeline.duration_seconds(),
                            current_time_s,
                            commands,
                        );
                    },
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
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
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

    ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_1);
    for (group, entry) in &all_entries {
        let row_height = sp.inspector.row_height;
        let available = ui.available_width();
        let (row_rect, row_response) =
            ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

        if row_response.hovered() {
            ui.painter().rect_filled(row_rect, 0.0, theme.surface.hover);
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
                egui::pos2(row_rect.min.x + sp.base.space_2, baseline_y - 3.0),
                egui::pos2(row_rect.min.x + sp.base.space_2 + bar_w, baseline_y + 3.0),
            );
            let bar_color = if entry.keyframe_count >= max_kf / 2 {
                theme.status.warning
            } else {
                theme.text.muted
            };
            ui.painter().rect_filled(bar_rect, RADIUS_S, bar_color);
        }

        // Icon + property name
        let name_x = row_rect.min.x + sp.base.space_2 + bar_max_w + sp.base.space_2;
        ui.painter().text(
            egui::pos2(name_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            format!("{} {}", group.icon, entry.name),
            TextRole::BodyS.font_id(),
            theme.text.secondary,
        );

        // Current value (middle area)
        let value_text = format_property_value(&entry.kind);
        let value_x = name_x + 100.0;
        if !value_text.is_empty() {
            ui.painter().text(
                egui::pos2(value_x, baseline_y),
                egui::Align2::LEFT_CENTER,
                &value_text,
                egui::FontId::monospace(TextRole::Micro.size()),
                theme.text.muted,
            );
        }

        // Keyframe count badge (right)
        if entry.keyframe_count > 0 {
            let count_rect = egui::Rect::from_center_size(
                egui::pos2(row_rect.max.x - sp.base.space_2, baseline_y),
                Vec2::new(40.0, row_height),
            );
            ui.put(
                count_rect,
                Badge::new(format!("{} {}", egui_phosphor::regular::DIAMOND, entry.keyframe_count)),
            );
        }

        // Click to jump to the property in semantic view
        if row_response.clicked() {
            *property_view_mode = PropertyViewMode::Semantic;
        }
    }
    ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_2);

    // Divider between animated and non-animated
    let animated_count = all_entries.iter().filter(|(_, e)| e.keyframe_count > 0).count();
    if animated_count > 0 && animated_count < all_entries.len() {
        ui.add_space(sp.base.space_2);
        let divider_rect = ui.available_rect_before_wrap();
        if divider_rect.width() > 0.0 {
            ui.painter().line_segment(
                [
                    egui::pos2(divider_rect.min.x + sp.base.space_2, divider_rect.min.y + 4.0),
                    egui::pos2(divider_rect.max.x - sp.base.space_2, divider_rect.min.y + 4.0),
                ],
                egui::Stroke::new(STROKE_WIDTH, theme.border.default),
            );
        }
        ui.add_space(sp.base.space_2);
    }
}

// ─── Actor Header ─────────────────────────────────────────────────────────

fn render_actor_header(
    ui: &mut egui::Ui,
    track: &AnimationTrack,
    current_time_s: f64,
    commands: &mut ActionQueue,
) {
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
    let current_time_ms = (current_time_s * 1000.0) as u64;
    let available = ui.available_width();
    let row_h = sp.base.row_l;
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(available, row_h), egui::Sense::hover());

    // ── Left side: icon + name ──
    let left_rect =
        egui::Rect::from_min_max(row_rect.min, egui::pos2(row_rect.center().x, row_rect.max.y));
    ui.scope_builder(egui::UiBuilder::new().max_rect(left_rect), |ui| {
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
            |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(actor_icon_str(track.kind))
                            .size(TextRole::Heading.size())
                            .color(theme.status.warning),
                    )
                    .selectable(false),
                );
                ui.add_space(sp.base.space_2);

                // Actor label (click to rename)
                let edit_id = ui.id().with("actor_name_edit");
                let is_editing: bool = ui.data(|d| d.get_temp(edit_id)).unwrap_or(false);
                let mut edit_buffer: String = ui
                    .data(|d| d.get_temp(edit_id.with("buf")))
                    .unwrap_or_else(|| track.label.clone());

                if is_editing {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut edit_buffer)
                            .font(TextRole::Heading.font_id())
                            .text_color(theme.text.primary)
                            .desired_width(120.0),
                    );
                    response.request_focus();
                    if response.lost_focus() {
                        ui.data_mut(|d| d.insert_temp(edit_id, false));
                        if edit_buffer != track.label && !edit_buffer.is_empty() {
                            commands.push_back(
                                ActorCommand::RenameActor {
                                    old_label: track.label.clone(),
                                    new_label: edit_buffer.clone(),
                                }
                                .into(),
                            );
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
                                .size(TextRole::Heading.size())
                                .color(theme.text.primary),
                        )
                        .selectable(false)
                        .sense(egui::Sense::click()),
                    );
                    if label_response.double_clicked() {
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
    let right_rect =
        egui::Rect::from_min_max(egui::pos2(row_rect.center().x, row_rect.min.y), row_rect.max);
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
                            .size(TextRole::Micro.size())
                            .color(theme.text.muted),
                        )
                        .selectable(false),
                    );
                    ui.add_space(sp.base.space_3);
                }

                if let Some(shape_pt) = &track.shape.shape_type {
                    let shape = shape_pt.evaluate(current_time_ms);
                    ui.add(
                        egui::Label::new(
                            RichText::new(shape.to_string())
                                .size(TextRole::BodyS.size())
                                .color(theme.text.muted),
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
    let current_parent = timeline
        .tracks()
        .iter()
        .find(|(_, track)| track.children.iter().any(|c| c == actor))
        .map(|(label, _)| label.clone());

    layout::group_box(ui, format!("{} Hierarchy", egui_phosphor::regular::TREE_STRUCTURE), |ui| {
        layout::labeled_row(ui, "Parent", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
            let all_labels: Vec<String> =
                timeline.tracks().keys().filter(|&label| label != actor).cloned().collect();

            let current_display = current_parent.as_deref().unwrap_or("None (root)");
            egui::ComboBox::from_id_salt(ui.id().with("parent_dropdown"))
                .selected_text(current_display)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    if ui.stable_selectable_label(current_parent.is_none(), "None (root)").clicked()
                        && current_parent.is_some()
                    {
                        commands.push_back(
                            ActorCommand::ReparentActor {
                                actor: actor.to_string(),
                                new_parent: None,
                            }
                            .into(),
                        );
                    }
                    for label in &all_labels {
                        let is_selected = current_parent.as_deref() == Some(label.as_str());
                        if ui.stable_selectable_label(is_selected, label).clicked() && !is_selected
                        {
                            commands.push_back(
                                ActorCommand::ReparentActor {
                                    actor: actor.to_string(),
                                    new_parent: Some(label.clone()),
                                }
                                .into(),
                            );
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
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
    ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, sp.base.space_2);
    for (i, label) in order.iter().enumerate() {
        let row_id = ui.id().with(format!("child_{}", i));
        let available = ui.available_width();
        let (row_rect, _) =
            ui.allocate_exact_size(Vec2::new(available, sp.base.row_m), egui::Sense::hover());

        // Background
        let bg = if ui.rect_contains_pointer(row_rect) {
            theme.surface.hover
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(row_rect, RADIUS_M, bg);
        }

        let baseline_y = row_rect.center().y;
        let mut cursor_x = row_rect.min.x + sp.base.space_3;

        // Index badge
        let badge_rect = egui::Rect::from_min_size(
            egui::pos2(cursor_x, baseline_y - 9.0),
            Vec2::new(sp.base.row_s, sp.base.row_s),
        );
        ui.put(badge_rect, Badge::new((i + 1).to_string()));
        cursor_x += sp.base.row_s + sp.base.space_2;

        // Label
        ui.painter().text(
            egui::pos2(cursor_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            label,
            TextRole::BodyS.font_id(),
            theme.text.secondary,
        );

        // Up / Down buttons (right-aligned)
        let btn_size = Vec2::new(sp.base.row_s, sp.base.row_s);
        let btn_y = row_rect.min.y + (row_rect.height() - btn_size.y) * 0.5;
        let mut btn_x = row_rect.max.x - sp.base.space_2 - btn_size.x;

        // Down button
        let down_rect = egui::Rect::from_min_size(egui::pos2(btn_x, btn_y), btn_size);
        let down_resp = ui.interact(down_rect, row_id.with("down"), egui::Sense::click());
        text_tooltip(ui, down_resp.id.with("tooltip"), &down_resp, "Move down");
        let down_color = if i + 1 >= order.len() {
            theme.text.disabled
        } else if down_resp.hovered() {
            theme.text.primary
        } else {
            theme.text.secondary
        };
        ui.painter().text(
            down_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::CARET_DOWN,
            TextRole::BodyS.font_id(),
            down_color,
        );
        btn_x -= btn_size.x + sp.base.space_1;

        // Up button
        let up_rect = egui::Rect::from_min_size(egui::pos2(btn_x, btn_y), btn_size);
        let up_resp = ui.interact(up_rect, row_id.with("up"), egui::Sense::click());
        text_tooltip(ui, up_resp.id.with("tooltip"), &up_resp, "Move up");
        let up_color = if i == 0 {
            theme.text.disabled
        } else if up_resp.hovered() {
            theme.text.primary
        } else {
            theme.text.secondary
        };
        ui.painter().text(
            up_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::CARET_UP,
            TextRole::BodyS.font_id(),
            up_color,
        );

        // Emit reorder on click
        if up_resp.clicked() && i > 0 {
            let mut new_order = order.to_vec();
            new_order.swap(i, i - 1);
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: container.to_string(),
                    property: "child_order".into(),
                    value: GuiPropertyValue::StringList(new_order),
                    create_keyframe: keyframe_mode,
                })
                .into(),
            );
        }
        if down_resp.clicked() && i + 1 < order.len() {
            let mut new_order = order.to_vec();
            new_order.swap(i, i + 1);
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: container.to_string(),
                    property: "child_order".into(),
                    value: GuiPropertyValue::StringList(new_order),
                    create_keyframe: keyframe_mode,
                })
                .into(),
            );
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
        },
        PropertyKind::Text(s) => {
            if s.chars().count() > 16 {
                format!("{}…", app_text::truncate_chars(s, 15))
            } else {
                s.clone()
            }
        },
        PropertyKind::Enum { value, .. } => value.clone(),
        PropertyKind::Sum { value, .. } => match value {
            animatix::timeline::PropertyValue::Variant { name, value } => {
                let payload = match value.as_ref() {
                    animatix::timeline::PropertyValue::Bool(v) => v.to_string(),
                    animatix::timeline::PropertyValue::String(v) => v.clone(),
                    animatix::timeline::PropertyValue::F32(v) => format!("{v:.2}"),
                    other => format!("{other:?}"),
                };
                format!("{name}: {payload}")
            },
            other => format!("{other:?}"),
        },
        PropertyKind::Union { value, .. } => match value {
            animatix::timeline::PropertyValue::Bool(v) => v.to_string(),
            animatix::timeline::PropertyValue::String(v) => v.clone(),
            animatix::timeline::PropertyValue::F32(v) => format!("{v:.2}"),
            animatix::timeline::PropertyValue::U32(v) => v.to_string(),
            animatix::timeline::PropertyValue::Vec2(v) => format!("({:.1}, {:.1})", v[0], v[1]),
            animatix::timeline::PropertyValue::Color(v)
            | animatix::timeline::PropertyValue::Vec4(v) => {
                let r = (v[0] * 255.0).round() as u8;
                let g = (v[1] * 255.0).round() as u8;
                let b = (v[2] * 255.0).round() as u8;
                if v[3] >= 0.999 {
                    format!("#{r:02x}{g:02x}{b:02x}")
                } else {
                    format!("#{r:02x}{g:02x}{b:02x}{:02x}", (v[3] * 255.0) as u8)
                }
            },
            other => format!("{other:?}"),
        },
    }
}
