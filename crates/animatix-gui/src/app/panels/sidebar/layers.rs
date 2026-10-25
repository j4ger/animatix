//! Layers tab and actor tree.

use super::*;

pub(super) fn layers_content_ui(ctx: &mut LayersContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let Some(timeline) = ctx.timeline else {
        layout::empty_state(ui, egui_phosphor::regular::FILM_STRIP, "No timeline loaded", "");
        return;
    };

    // ── Filter input ──────────────────────────────────────────────────────
    let filter_id = egui::Id::new(LAYERS_FILTER_ID);
    let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();

    ui.horizontal(|ui| {
        ui.add_space(sp.base.space_2);
        let field_w = (ui.available_width() - sp.base.space_2).max(40.0);
        let response = TextField::new(&mut filter)
            .placeholder("Filter layers…")
            .cleanable(true)
            .desired_width(field_w)
            .show(ui);
        if response.changed {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
        if filter.is_empty() && response.lost_focus() {
            ui.data_mut(|d| d.insert_temp(filter_id, String::new()));
        }
    });
    ui.add_space(sp.base.space_2);

    // ── Inline rename bar (context-menu "Rename…" or double-click a layer) ──
    let rename_id = layer_rename_id();
    if let Some(target) = ui.data(|d| d.get_temp::<String>(rename_id)) {
        let buf_id = rename_id.with("buf");
        let mut buf = ui.data(|d| d.get_temp::<String>(buf_id)).unwrap_or_else(|| target.clone());
        ui.horizontal(|ui| {
            ui.add_space(sp.base.space_2);
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{} Rename", egui_phosphor::regular::PENCIL_SIMPLE))
                        .size(TextRole::BodyS.size())
                        .color(t.palette.text.muted),
                )
                .selectable(false),
            );
            let response = TextField::new(&mut buf)
                .placeholder("New name")
                .desired_width(ui.available_width())
                .show(ui);
            response.request_focus();
            if response.lost_focus() {
                ui.data_mut(|d| d.remove::<String>(rename_id));
                ui.data_mut(|d| d.remove::<String>(buf_id));
                let new_label = buf.trim().to_string();
                if !new_label.is_empty() && new_label != target {
                    ctx.commands.push_back(
                        ActorCommand::RenameActor {
                            old_label: target.clone(),
                            new_label,
                        }
                        .into(),
                    );
                }
            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui.data_mut(|d| d.remove::<String>(rename_id));
                ui.data_mut(|d| d.remove::<String>(buf_id));
            } else {
                ui.data_mut(|d| d.insert_temp(buf_id, buf.clone()));
            }
        });
        ui.add_space(sp.base.space_2);
    }

    let filter_lower = filter.to_lowercase();
    let has_filter = !filter_lower.is_empty();

    // Show which scene's actors are being displayed
    if ctx.is_composition {
        if let Some(scene_name) = ctx.active_scene.as_ref() {
            ui.horizontal(|ui| {
                ui.add_space(sp.base.space_4);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{} {}",
                            egui_phosphor::regular::FILM_STRIP,
                            scene_name
                        ))
                        .size(TextRole::BodyS.size())
                        .color(t.palette.text.muted),
                    )
                    .selectable(false),
                );
            });
            ui.add_space(sp.base.space_2);
        }
    }

    let root_ids = timeline.world().root_entities();
    if root_ids.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(sp.base.space_5 * 3.0);
            ui.add(
                egui::Label::new(
                    RichText::new(egui_phosphor::regular::FILM_STRIP)
                        .size(sp.base.row_l)
                        .color(t.palette.text.muted),
                )
                .selectable(false),
            );
            ui.add_space(sp.base.space_3);
            ui.add(
                egui::Label::new(
                    RichText::new("No actors in scene")
                        .size(TextRole::Title.size())
                        .color(t.palette.text.secondary),
                )
                .selectable(false),
            );
            ui.add_space(sp.base.space_5);
            if ui
                .button(
                    RichText::new(format!("{} Add Actor", egui_phosphor::regular::PLUS))
                        .size(TextRole::Title.size())
                        .color(t.palette.accent.primary),
                )
                .clicked()
            {
                let ty = crate::app::panels::default_actor_type();
                let template = crate::app::primitives::ActorTemplate::for_primitive(
                    ty,
                    None,
                    ctx.scene_dimensions,
                    ctx.timeline,
                );
                let pos = [
                    ctx.scene_dimensions.width as f32 / 2.0,
                    ctx.scene_dimensions.height as f32 / 2.0,
                ];
                ctx.commands.push_back(
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

    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        for &root_id in root_ids {
            let Some(root_label) = timeline.get_entity_label(root_id) else {
                continue;
            };
            render_actor_tree(
                ui,
                timeline,
                root_label,
                ctx.selected_actors,
                ctx.collapsed_actors,
                ctx.commands,
                time_ms,
                0,
                &filter_lower,
                has_filter,
            );
        }
    });
}

// ─── Layer Tree ─────────────────────────────────────────────────────────────

fn render_actor_tree(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    label: &str,
    selected_actors: &mut HashSet<String>,
    collapsed_actors: &mut HashSet<String>,
    commands: &mut ActionQueue,
    _time_ms: u64,
    depth: usize,
    filter_lower: &str,
    has_filter: bool,
) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let Some(id) = timeline.get_entity_id(label) else {
        return;
    };

    let is_selected = selected_actors.contains(label);
    let is_anonymous = label.starts_with("__anon");
    let has_children = timeline.child_labels(label).next().is_some();

    // ── Filter logic ─────────────────────────────────────────────────────
    // When a filter is active, decide whether this actor (and its subtree)
    // should be visible.  An actor is visible if:
    //   1. Its own label matches the filter, OR
    //   2. Any of its descendants match the filter.
    // Matching directories/actors are force-expanded so their descendants
    // remain visible (mirrors explorer_content_ui's show-mask logic).
    // NOTE: selected_actors is intentionally NOT cleared when filtering;
    // hidden-but-selected actors stay selected so they survive filter changes.
    // Drag-reparent drops are disabled while filtered to avoid reparenting
    // through partially-visible tree branches (ambiguous drop targets).
    let (is_expanded, should_show) = if has_filter {
        let base_expanded = has_children && !collapsed_actors.contains(label);
        let self_matches = label.to_lowercase().contains(filter_lower);
        let descendant_matches = timeline
            .child_labels(label)
            .any(|child| actor_matches_filter(timeline, child, filter_lower));
        let show = self_matches || descendant_matches;
        let expanded = self_matches || descendant_matches || base_expanded;
        (expanded, show)
    } else {
        let is_expanded = has_children && !collapsed_actors.contains(label);
        (is_expanded, true)
    };

    if has_filter && !should_show {
        return;
    }

    let is_visible = timeline.is_actor_visible(label);

    let (icon, display_label, label_color): (
        Option<std::borrow::Cow<'static, str>>,
        &str,
        Option<egui::Color32>,
    ) = if is_anonymous {
        (
            Some(std::borrow::Cow::Borrowed(egui_phosphor::regular::GHOST)),
            "anon",
            Some(t.palette.text.muted),
        )
    } else {
        let icon = Some(crate::app::icons::actor_icon_for_track(id, timeline));
        (icon, label, None)
    };

    let row_id = ui.id().with(label);
    let label_owned = label.to_string();

    // Visibility toggle (eye icon) on the right
    let eye_icon = if is_visible {
        egui_phosphor::regular::EYE
    } else {
        egui_phosphor::regular::EYE_CLOSED
    };
    let eye_color = if is_visible {
        t.palette.text.secondary
    } else {
        t.palette.text.disabled
    };

    let is_locked = timeline.is_actor_locked(label);
    let lock_icon = if is_locked {
        egui_phosphor::regular::LOCK_KEY
    } else {
        egui_phosphor::regular::LOCK_KEY_OPEN
    };
    let lock_color = if is_locked {
        t.palette.status.warning
    } else {
        t.palette.text.disabled
    };

    let response = row::Row::new(display_label)
        .indent(depth as f32 * sp.base.component.icon_slot_width)
        .selected(is_selected)
        .icon(icon.as_deref())
        .label_color(label_color.unwrap_or(if is_visible {
            t.palette.text.secondary
        } else {
            t.palette.text.disabled
        }))
        .has_children(has_children)
        .expanded(is_expanded)
        .sense(egui::Sense::click_and_drag())
        .right(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);
            let eye_btn = ui.add(
                Button::icon(eye_icon)
                    .with_tooltip(if is_visible {
                        "Hide layer"
                    } else {
                        "Show layer"
                    })
                    .icon_color(eye_color)
                    .hover_icon_color(t.palette.text.primary),
            );
            if eye_btn.clicked() {
                commands.push_back(ActorCommand::ToggleActorVisibility(label.to_string()).into());
            }
            let lock_btn = ui.add(
                Button::icon(lock_icon)
                    .with_tooltip(if is_locked {
                        "Unlock layer"
                    } else {
                        "Lock layer"
                    })
                    .icon_color(lock_color)
                    .hover_icon_color(t.palette.text.primary),
            );
            if lock_btn.clicked() {
                commands.push_back(ActorCommand::ToggleActorLock(label.to_string()).into());
            }
        })
        .show(ui, row_id);

    // ── Drag-and-drop reparenting ──
    // Disabled while a filter is active: the partial tree makes drop targets
    // ambiguous (hidden ancestors/descendants can't be valid drop destinations).
    let drag_id = ui.id().with("layer_drag");
    let drag_data_id = drag_id.with("data");

    if !has_filter && response.drag_started && !is_anonymous {
        ui.data_mut(|d| d.insert_temp(drag_data_id, label.to_string()));
    }

    let is_dragging = ui.data(|d| d.get_temp::<String>(drag_data_id)).is_some();
    let is_drop_target = !has_filter && is_dragging && response.hovered && !is_anonymous;
    if is_drop_target {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if dragged != label {
            ui.painter().rect_stroke(
                response.row_rect.expand(1.0),
                2,
                egui::Stroke::new(1.5, t.palette.accent.primary),
                egui::StrokeKind::Outside,
            );
        }
    }

    // Drop-to-root indicator: show a line at the top of depth-0 rows when
    // the pointer is within the expanded drop zone but not over the row.
    if is_dragging && depth == 0 {
        let pointer_pos = ui.input(|i| i.pointer.latest_pos());
        let in_drop_zone = pointer_pos.is_some_and(|p| {
            !response.row_rect.contains(p) && response.row_rect.expand(8.0).contains(p)
        });
        if in_drop_zone {
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(response.row_rect.left(), response.row_rect.top()),
                    egui::pos2(response.row_rect.right(), response.row_rect.top() + 2.0),
                ),
                0.0,
                t.palette.accent.primary,
            );
        }
    }

    if !has_filter && is_dragging && ui.input(|i| i.pointer.any_released()) {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if !dragged.is_empty() && dragged != label {
            let pointer_pos = ui.input(|i| i.pointer.latest_pos());
            let over_this_row = pointer_pos.is_some_and(|p| response.row_rect.contains(p));
            if over_this_row && is_drop_target {
                commands.push_back(ShellAction::Command(Command::ReparentActor {
                    actor: dragged.clone(),
                    new_parent: Some(label.to_string()),
                }));
            } else if !over_this_row && depth == 0 {
                let over_any_root =
                    pointer_pos.is_some_and(|p| response.row_rect.expand(8.0).contains(p));
                if over_any_root {
                    commands.push_back(ShellAction::Command(Command::ReparentActor {
                        actor: dragged.clone(),
                        new_parent: None,
                    }));
                }
            }
        }
        ui.data_mut(|d| d.remove::<String>(drag_data_id));
    }

    // Right-click context menu for layer rows (use row response so we don't steal left-clicks)
    response.response.context_menu(|ui| {
        let has_multi = selected_actors.len() >= 2;

        // Each row carries an explicit action identity, so a click can never
        // fall through to a destructive action when the menu layout changes.
        #[derive(Clone, Copy)]
        enum LayerMenuAction {
            Duplicate,
            Rename,
            Align(crate::app::commands::Align),
            Distribute(crate::app::commands::Axis),
            Group,
            Ungroup,
            Delete,
        }

        use egui_phosphor::regular as icons;

        let mut menu: Vec<(MenuEntry, Option<LayerMenuAction>)> = Vec::new();
        menu.push((
            MenuEntry::item_with_icon(icons::COPY, "Duplicate"),
            Some(LayerMenuAction::Duplicate),
        ));
        menu.push((
            MenuEntry::item_with_icon(icons::PENCIL_SIMPLE, "Rename…"),
            Some(LayerMenuAction::Rename),
        ));
        if has_multi {
            use crate::app::commands::{Align, Axis};
            menu.push((MenuEntry::separator(), None));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_LEFT, "Align Left"),
                Some(LayerMenuAction::Align(Align::Left)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_CENTER_HORIZONTAL_SIMPLE, "Align Center"),
                Some(LayerMenuAction::Align(Align::Center)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_RIGHT, "Align Right"),
                Some(LayerMenuAction::Align(Align::Right)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_TOP, "Align Top"),
                Some(LayerMenuAction::Align(Align::Top)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_CENTER_VERTICAL_SIMPLE, "Align Middle"),
                Some(LayerMenuAction::Align(Align::Middle)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_BOTTOM, "Align Bottom"),
                Some(LayerMenuAction::Align(Align::Bottom)),
            ));
            menu.push((MenuEntry::separator(), None));
            menu.push((
                MenuEntry::item_with_icon(
                    icons::ARROWS_OUT_LINE_HORIZONTAL,
                    "Distribute Horizontally",
                ),
                Some(LayerMenuAction::Distribute(Axis::Horizontal)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ARROWS_OUT_LINE_VERTICAL, "Distribute Vertically"),
                Some(LayerMenuAction::Distribute(Axis::Vertical)),
            ));
        }
        menu.push((MenuEntry::separator(), None));
        menu.push((
            MenuEntry::item_with_icon(icons::SELECTION_PLUS, "Group"),
            Some(LayerMenuAction::Group),
        ));
        menu.push((
            MenuEntry::item_with_icon(icons::SQUARES_FOUR, "Ungroup"),
            Some(LayerMenuAction::Ungroup),
        ));
        menu.push((MenuEntry::separator(), None));
        menu.push((
            MenuEntry::item_with_icon(icons::TRASH, "Delete"),
            Some(LayerMenuAction::Delete),
        ));

        let entries: Vec<MenuEntry> = menu.iter().map(|(entry, _)| entry.clone()).collect();
        if let Some(idx) = render_menu(ui, &entries) {
            match menu.get(idx).and_then(|(_, action)| *action) {
                Some(LayerMenuAction::Duplicate) => commands
                    .push_back(ShellAction::Command(Command::DuplicateActor(label.to_string()))),
                Some(LayerMenuAction::Rename) => start_layer_rename(ui, label),
                Some(LayerMenuAction::Align(align)) => {
                    commands.push_back(ShellAction::Command(Command::AlignActors(align)))
                },
                Some(LayerMenuAction::Distribute(axis)) => {
                    commands.push_back(ShellAction::Command(Command::DistributeActors(axis)))
                },
                Some(LayerMenuAction::Group) => {
                    commands.push_back(ActorCommand::GroupSelectedActors.into())
                },
                Some(LayerMenuAction::Ungroup) => {
                    commands.push_back(ActorCommand::UngroupSelectedActors.into())
                },
                Some(LayerMenuAction::Delete) => {
                    selected_actors.clear();
                    selected_actors.insert(label.to_string());
                    commands.push_back(ActorCommand::DeleteSelectedActors.into());
                },
                None => {}, // Separator or out-of-range index: no action.
            }
            ui.close();
        }
    });

    // Double-click a layer name to rename it inline in the panel header bar.
    if response.response.double_clicked() && !is_anonymous {
        start_layer_rename(ui, label);
    }

    if response.chevron_clicked {
        if collapsed_actors.contains(&label_owned) {
            collapsed_actors.remove(&label_owned);
        } else {
            collapsed_actors.insert(label_owned.clone());
        }
    }

    if response.row_clicked || response.drag_started {
        let modifiers = ui.ctx().input(|i| i.modifiers);
        let multi = modifiers.shift || modifiers.ctrl || modifiers.command;
        if multi {
            if selected_actors.contains(label) {
                selected_actors.remove(label);
            } else {
                selected_actors.insert(label.to_string());
            }
        } else {
            selected_actors.clear();
            selected_actors.insert(label.to_string());
        }
    }

    // Children
    if is_expanded {
        for child_label in timeline.child_labels(label) {
            render_actor_tree(
                ui,
                timeline,
                child_label,
                selected_actors,
                collapsed_actors,
                commands,
                _time_ms,
                depth + 1,
                filter_lower,
                has_filter,
            );
        }
    }
}

/// Recursively check whether any actor in the subtree rooted at `label`
/// matches `filter_lower` (case-insensitive substring on the actor label).
fn actor_matches_filter(timeline: &Timeline, label: &str, filter_lower: &str) -> bool {
    if !timeline.has_actor(label) {
        return false;
    }
    if label.to_lowercase().contains(filter_lower) {
        return true;
    }
    timeline
        .child_labels(label)
        .any(|child| actor_matches_filter(timeline, child, filter_lower))
}

// ─── Components Tab ───────────────────────────────────────────────────────

/// Temp-data id for the inline layer rename bar.
fn layer_rename_id() -> egui::Id {
    egui::Id::new("layer_rename_target")
}

/// Begin renaming a layer: seed the edit buffer for the rename bar.
fn start_layer_rename(ui: &mut egui::Ui, label: &str) {
    let id = layer_rename_id();
    ui.data_mut(|d| {
        d.insert_temp(id, label.to_string());
        d.insert_temp(id.with("buf"), label.to_string());
    });
}
