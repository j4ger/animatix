use super::*;

///   4. Keyframes
pub(crate) fn inspector_ui(
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
        render_empty_scene(ui, timeline, scene_dimensions, commands, &sp, &theme);
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

    render_single_actor_selection(
        ui,
        timeline,
        selected_actors,
        current_time_s,
        commands,
        keyframe_mode,
        pivot_offsets,
        property_view_mode,
        keyframe_view_mode,
        active_scene,
        &sp,
        &theme,
    );
}
fn render_multi_selection(
    ui: &mut egui::Ui,
    selected_actors: &HashSet<String>,
    multi_count: usize,
    sp: &Spatial,
    theme: &eparts::Theme,
) {
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

fn render_actor_identity(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    id: EntityId,
    sel: &str,
    current_time_s: f64,
    commands: &mut ActionQueue,
    sp: &Spatial,
    theme: &eparts::Theme,
) {
    render_actor_header(ui, timeline, id, current_time_s, commands);
    ui.add_space(sp.base.space_3);

    // ── Parent ──
    render_parent_card(ui, timeline, sel, commands);
    ui.add_space(sp.base.space_3);

    // ── Referenced Assets ──
    let referenced_assets: Vec<String> = timeline.asset_cache().assets_for(sel).cloned().collect();
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
}

fn render_properties_card(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    id: EntityId,
    sel: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    current_time_s: f64,
    property_view_mode: &mut PropertyViewMode,
    active_scene: Option<&str>,
    sp: &Spatial,
    theme: &eparts::Theme,
) {
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
            let mut selected = modes.iter().position(|mode| *mode == view_mode).unwrap_or(0);
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
}

fn render_actor_footer(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    id: EntityId,
    sel: &String,
    current_time_s: f64,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    pivot_offsets: &mut std::collections::HashMap<String, [f32; 2]>,
    multi_count: usize,
    sp: &Spatial,
) {
    if multi_count == 1 {
        layout::group_box(ui, format!("{} Pivot", egui_phosphor::regular::CROSSHAIR), |ui| {
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
        });
        ui.add_space(sp.base.space_3);
    }

    // ── Container Children ──
    if timeline.container_layout(sel).is_some() {
        layout::card(ui, |ui| {
            layout::section_header(
                ui,
                egui_phosphor::regular::ROWS,
                "Children",
                Some(timeline.layout_children_for(id).len()),
            );
            let time_ms = (current_time_s * 1000.0) as u64;
            let order = timeline.get_child_order(sel, time_ms);
            render_container_children(ui, sel, &order, commands, keyframe_mode);
        });
        ui.add_space(sp.base.space_3);
    }
    layout::group_box(ui, format!("{} Timeline", egui_phosphor::regular::CLOCK), |ui| {
        let duration_s = timeline.duration_seconds().max(0.1);
        let all_kf = collect_all_keyframe_times(id, timeline.world().effects(id), timeline.world());
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
}

fn render_keyframes_card(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    id: EntityId,
    sel: &String,
    commands: &mut ActionQueue,
    current_time_s: f64,
    keyframe_view_mode: &mut KeyframeViewMode,
    active_scene: Option<&str>,
    sp: &Spatial,
) {
    let kf_count = count_keyframes(timeline, id);
    layout::card(ui, |ui| {
        let kf_view = *keyframe_view_mode;

        let header_top = ui.cursor().min.y;
        layout::section_header(ui, egui_phosphor::regular::KEY, "Keyframes", Some(kf_count));

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
}

fn render_empty_scene(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    scene_dimensions: animatix::timeline::SceneDimensions,
    commands: &mut ActionQueue,
    sp: &Spatial,
    theme: &eparts::Theme,
) {
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

#[allow(clippy::too_many_arguments)]
fn render_single_actor_selection(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    selected_actors: &mut HashSet<String>,
    current_time_s: f64,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    pivot_offsets: &mut std::collections::HashMap<String, [f32; 2]>,
    property_view_mode: &mut PropertyViewMode,
    keyframe_view_mode: &mut KeyframeViewMode,
    active_scene: Option<&str>,
    sp: &Spatial,
    theme: &eparts::Theme,
) {
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
            render_multi_selection(ui, selected_actors, multi_count, &sp, &theme);
            return;
        }

        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            render_actor_identity(ui, timeline, id, sel, current_time_s, commands, &sp, &theme);
            ui.add_space(sp.base.space_3);
            render_properties_card(
                ui,
                timeline,
                id,
                sel,
                commands,
                keyframe_mode,
                current_time_s,
                property_view_mode,
                active_scene,
                &sp,
                &theme,
            );
            ui.add_space(sp.base.space_3);
            render_actor_footer(
                ui,
                timeline,
                id,
                sel,
                current_time_s,
                commands,
                keyframe_mode,
                pivot_offsets,
                multi_count,
                &sp,
            );
            ui.add_space(sp.base.space_3);
            render_keyframes_card(
                ui,
                timeline,
                id,
                sel,
                commands,
                current_time_s,
                keyframe_view_mode,
                active_scene,
                &sp,
            );
        });
    } else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::MAGNIFYING_GLASS,
            "Select an actor to inspect",
            "Click an actor in the Preview or Layers panel",
        );
    }
}
