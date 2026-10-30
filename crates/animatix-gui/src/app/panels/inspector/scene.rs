use super::*;
use crate::app::design_tokens::spatial::Spatial;
pub(super) fn render_scene_inspector(
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
            format!("{} Scene: {}", egui_phosphor::regular::FILM_STRIP, active_scene),
            TextRole::Heading.font_id(),
            theme.palette.accent.primary,
        );
        ui.label(
            RichText::new("No actor selected — showing scene properties")
                .size(TextRole::Caption.size())
                .color(theme.palette.text.muted),
        );
        ui.add_space(sp.base.space_3);

        // ── Scene Properties ──
        layout::group_box(ui, format!("{} Properties", egui_phosphor::regular::WRENCH), |ui| {
            // Duration (editable — explicit or inferred from timeline keyframes)
            layout::labeled_row(ui, "Duration", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                let is_explicit = scene.explicit_duration_s.is_some();
                let mut duration_val = scene.duration_s;
                let drag = NumberField::new(&mut duration_val)
                    .speed(0.1)
                    .suffix(" s")
                    .range(0.01..=600.0)
                    .desired_width(INSPECTOR_INPUT_WIDTH_FLOAT)
                    .show(ui);
                let tooltip = if is_explicit {
                    "Explicit duration (click to revert to auto)"
                } else {
                    "Auto-detected from keyframes (drag to set explicit)"
                };
                let changed = drag.changed();
                text_tooltip(ui, ui.id().with("scene_duration_tooltip"), &drag, tooltip);
                if is_explicit {
                    // Small button to remove explicit duration
                    let revert = ui.add(Button::ghost("").with_icon(egui_phosphor::regular::X));
                    text_tooltip(
                        ui,
                        ui.id().with("scene_duration_revert_tooltip"),
                        &revert,
                        "Revert to auto-detected duration",
                    );
                    if revert.clicked() {
                        commands.push_back(
                            Command::SetSceneDuration {
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
                        Command::SetSceneDuration {
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
                            .color(theme.palette.text.secondary),
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
                        .color(theme.palette.text.muted),
                    )
                    .selectable(false),
                );
            });
        });

        ui.add_space(sp.base.space_3);

        // ── Play edge (create / clear) ──
        layout::group_box(ui, format!("{} Play Edge", egui_phosphor::regular::ARROW_RIGHT), |ui| {
            let current_target =
                composition.edges.get(active_scene).map(|edge| edge.to_scene.clone());
            let other_scenes: Vec<&String> =
                composition.declaration_order.iter().filter(|s| *s != active_scene).collect();
            layout::labeled_row(ui, "Next scene", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                let mut options = vec!["\u{2014} None".to_string()];
                options.extend(other_scenes.iter().map(|s| (*s).clone()));
                let mut sel_idx = match current_target.as_ref() {
                    Some(t) => other_scenes.iter().position(|s| *s == t).map(|i| i + 1),
                    None => Some(0),
                };
                ui.add(
                    Select::new(ui.id().with("play_edge_target"), &mut sel_idx, &options)
                        .placeholder("Select target…"),
                );
                if let Some(idx) = sel_idx {
                    let chosen = if idx == 0 {
                        None
                    } else {
                        Some(other_scenes[idx - 1].clone())
                    };
                    if chosen != current_target {
                        commands.push_back(
                            Command::SetPlayTarget {
                                from_scene: active_scene.to_string(),
                                target: chosen,
                            }
                            .into(),
                        );
                    }
                }
            });
        });

        ui.add_space(sp.base.space_3);

        // ── Transition (edit an existing play edge) ──
        if let Some(edge) = composition.edges.get(active_scene) {
            layout::group_box(
                ui,
                format!("{} Transition", egui_phosphor::regular::ARROW_RIGHT),
                |ui| {
                    // Transition type dropdown
                    let registry = animatix_syntax::transition_registry::REGISTRY;
                    layout::labeled_row(ui, "Type", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                        let names: Vec<&str> = registry.iter().map(|d| d.display_name).collect();
                        let mut sel_idx = registry.iter().position(|d| d.id == edge.transition.id);
                        ui.add(
                            Select::new(ui.id().with("transition_type"), &mut sel_idx, &names)
                                .placeholder("Select transition…"),
                        );
                        if let Some(idx) = sel_idx {
                            let chosen_id = registry[idx].id;
                            if chosen_id != edge.transition.id {
                                commands.push_back(
                                    Command::SetTransition {
                                        from_scene: active_scene.to_string(),
                                        transition: animatix_syntax::ast::Transition {
                                            id: chosen_id.to_string(),
                                            duration_ms: edge.transition.duration_ms,
                                            easing: edge.transition.easing,
                                        },
                                    }
                                    .into(),
                                );
                            }
                        }
                    });

                    // Duration
                    let mut duration_ms = edge.transition.duration_ms as f64;
                    layout::labeled_row(ui, "Duration", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                        NumberField::new(&mut duration_ms)
                            .speed(10.0)
                            .suffix(" ms")
                            .desired_width(INSPECTOR_INPUT_WIDTH_FLOAT)
                            .show(ui);
                    });
                    let new_duration_ms = duration_ms.round() as u64;
                    if new_duration_ms != edge.transition.duration_ms {
                        commands.push_back(
                            Command::SetTransition {
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
                        let names: Vec<&str> = animatix_syntax::easing::EASING_REGISTRY
                            .iter()
                            .map(|(_, name)| *name)
                            .collect();
                        let mut sel_idx =
                            animatix_syntax::easing::EASING_REGISTRY.iter().position(|(id, _)| {
                                animatix_syntax::easing::parse_easing_name(id)
                                    .unwrap_or(animatix_syntax::easing::Easing::Linear)
                                    == current_easing
                            });
                        ui.add(
                            Select::new(ui.id().with("transition_easing"), &mut sel_idx, &names)
                                .placeholder("Select easing…"),
                        );
                        if let Some(idx) = sel_idx {
                            let (id_str, _) = animatix_syntax::easing::EASING_REGISTRY[idx];
                            let variant = animatix_syntax::easing::parse_easing_name(id_str)
                                .unwrap_or(animatix_syntax::easing::Easing::Linear);
                            if variant != current_easing {
                                new_custom_easing = Some(variant);
                            }
                        }
                    });
                    if let Some(variant) = new_custom_easing {
                        commands.push_back(
                            Command::SetTransition {
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
                                Command::SetTransition {
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
                            .color(theme.palette.accent.primary),
                        )
                        .clicked()
                    {
                        commands.push_back(Command::SelectScene(edge.to_scene.clone()).into());
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
                    theme.palette.accent.selection
                } else if response.hovered() {
                    theme.palette.surface.hover
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
                        theme.palette.accent.primary
                    } else {
                        theme.palette.text.secondary
                    },
                );
                if response.clicked() && !is_active {
                    commands.push_back(Command::SelectScene(scene_name.clone()).into());
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
mod inspector;

pub(super) use inspector::inspector_ui;
