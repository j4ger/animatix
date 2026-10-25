use super::*;
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
                            SceneCommand::SetPlayTarget {
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
                                    SceneCommand::SetTransition {
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
                            .color(theme.palette.accent.primary),
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

    if timeline.world().root_entities().is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(sp.base.space_5 * 3.0);
            ui.add(
                egui::Label::new(
                    RichText::new(egui_phosphor::regular::FILM_STRIP)
                        .size(layout::EMPTY_STATE_ICON_SIZE)
                        .color(theme.palette.text.muted),
                )
                .selectable(false),
            );
            ui.add_space(sp.base.space_3);
            ui.add(
                egui::Label::new(
                    RichText::new("No actors in scene")
                        .size(TextRole::Title.size())
                        .color(theme.palette.text.secondary),
                )
                .selectable(false),
            );
            ui.add_space(sp.base.space_4);
            let add_actor_btn =
                ui.add(Button::primary("Add Actor").with_icon(egui_phosphor::regular::PLUS));
            text_tooltip(
                ui,
                add_actor_btn.id.with("add_actor_tip"),
                &add_actor_btn,
                "Add a new actor to the scene",
            );
            if add_actor_btn.clicked() {
                let ty = crate::app::panels::default_actor_type();
                let template = crate::app::primitives::ActorTemplate::for_primitive(
                    ty,
                    None,
                    scene_dimensions,
                    Some(timeline),
                );
                let pos = [
                    scene_dimensions.width as f32 / 2.0,
                    scene_dimensions.height as f32 / 2.0,
                ];
                commands.push_back(
                    ActorCommand::CreateActor {
                        ty: template.ty,
                        label: template.label,
                        position: pos,
                        props: template.props,
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
        let Some(id) = timeline.get_entity_id(sel) else {
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
                            .color(theme.palette.text.muted),
                    );
                    ui.add_space(sp.base.space_1);
                    let names: Vec<&str> = selected_actors.iter().map(String::as_str).collect();
                    ui.label(
                        RichText::new(names.join(", "))
                            .size(TextRole::Micro.size())
                            .color(theme.palette.text.muted),
                    );
                });
            });
            return;
        }

        // Single-actor selection
        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            render_actor_header(ui, timeline, id, current_time_s, commands);
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
                                    .color(theme.palette.text.secondary),
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
                let groups = build_property_groups(timeline, id, current_time_ms);
                if groups.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(sp.base.space_3);
                        ui.add(
                            egui::Label::new(
                                RichText::new("No editable properties")
                                    .size(TextRole::Body.size())
                                    .color(theme.palette.text.muted),
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
                                    sel,
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
                                sel,
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
                            let mut px = pivot[0] as f64;
                            if NumberField::new(&mut px)
                                .speed(1.0)
                                .suffix(" px")
                                .desired_width(INSPECTOR_INPUT_WIDTH_FLOAT)
                                .show(ui)
                                .changed()
                            {
                                pivot[0] = px as f32;
                            }
                        });
                        layout::labeled_row(ui, "Y", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                            let mut py = pivot[1] as f64;
                            if NumberField::new(&mut py)
                                .speed(1.0)
                                .suffix(" px")
                                .desired_width(INSPECTOR_INPUT_WIDTH_FLOAT)
                                .show(ui)
                                .changed()
                            {
                                pivot[1] = py as f32;
                            }
                        });
                        let reset_pivot = ui.add(Button::ghost("Reset"));
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
            if timeline.container_layout(sel).is_some() {
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
                let all_kf =
                    collect_all_keyframe_times(id, timeline.world().effects(id), timeline.world());
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
            let kf_count = count_keyframes(timeline, id);
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
                            id,
                            (current_time_s * 1000.0) as u64,
                            sel,
                            commands,
                            active_scene,
                        );
                    },
                    KeyframeViewMode::Curve => {
                        graph_editor::render_multi_fcurve(
                            ui,
                            timeline,
                            id,
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
