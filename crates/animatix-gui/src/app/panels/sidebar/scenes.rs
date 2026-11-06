//! Scenes tab.

use super::*;

pub(super) fn scenes_content_ui(ctx: &mut ScenesContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let Some(composition) = ctx.composition else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No composition loaded",
            "Define multiple scenes with # SceneName to see them here.",
        );
        return;
    };

    let scene_names = &composition.declaration_order;

    // ── Add scene ─────────────────────────────────────────────────────────
    scenes_add_scene_ui(ctx, ui, scene_names, sp);

    if scene_names.is_empty() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No scenes",
            "This composition has no scenes.",
        );
        return;
    }

    let drag_id = ui.scope_id().with("scene_drag");
    let drag_data_id = drag_id.with("data");
    let drop_index_id = drag_id.with("drop_idx");

    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        for (idx, scene_name) in scene_names.iter().enumerate() {
            scene_row_ui(ui, ctx, composition, scene_name, idx, t, sp, drag_data_id, drop_index_id);
        }

        // Handle drop (outside the loop so is_dragging is in scope)
        scenes_drop_ui(ui, ctx, scene_names, drag_data_id, drop_index_id);
    });
}

/// "New scene" action row.
fn scenes_add_scene_ui(
    ctx: &mut ScenesContext<'_>,
    ui: &mut egui::Ui,
    scene_names: &[String],
    sp: crate::app::design_tokens::spatial::Spatial,
) {
    ui.horizontal(|ui| {
        ui.add_space(sp.base.space_2);
        let new_btn = ui.add(Button::ghost("New scene").with_icon(egui_phosphor::regular::PLUS));
        text_tooltip(
            ui,
            new_btn.id.with("new_scene_tip"),
            &new_btn,
            "Add a scene at the end of the composition",
        );
        if new_btn.clicked() {
            let mut n = scene_names.len() + 1;
            let mut name = format!("scene{n}");
            while scene_names.iter().any(|existing| existing == &name) {
                n += 1;
                name = format!("scene{n}");
            }
            ctx.commands.push_back(Command::AddScene(name).into());
        }
    });
    ui.add_space(sp.base.space_2);
}

/// One scene row: drag/drop, click-to-activate, context menu, and hints.
fn scene_row_ui(
    ui: &mut egui::Ui,
    ctx: &mut ScenesContext<'_>,
    composition: &animatix::composition::Composition,
    scene_name: &str,
    idx: usize,
    t: eparts::Theme,
    sp: crate::app::design_tokens::spatial::Spatial,
    drag_data_id: egui::Id,
    drop_index_id: egui::Id,
) {
    let is_active = ctx.active_scene == Some(scene_name);
    let row_id = ui.scope_id().with(scene_name);

    // Duration hint
    let duration_hint = composition
        .scene_start_times
        .get(scene_name)
        .map(|start| {
            let end = composition
                .scene_start_times
                .iter()
                .filter(|(k, _)| *k != scene_name)
                .map(|(_, v)| *v)
                .filter(|v| *v > *start)
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(composition.global_duration_s);
            format!("{:.1}s – {:.1}s", start, end)
        })
        .unwrap_or_else(|| "–".to_string());

    // Transition hint (outgoing edge)
    let transition_hint = composition.edges.get(scene_name).map(|edge| {
        format!(
            "{} {} ({:.0}ms)",
            egui_phosphor::regular::ARROW_RIGHT,
            edge.to_scene,
            edge.transition.duration_ms
        )
    });

    let response = row::Row::new(scene_name)
        .selected(is_active)
        .icon(Some(egui_phosphor::regular::FILM_STRIP))
        .label_color(if is_active {
            t.palette.accent.primary
        } else {
            t.palette.text.secondary
        })
        .sense(egui::Sense::click_and_drag())
        .show(ui, row_id);

    // Drag start
    if response.drag_started {
        ui.data_mut(|d| d.insert_temp(drag_data_id, (idx, scene_name.to_string())));
    }

    // Drop target detection
    let is_dragging = ui.data(|d| d.get_temp::<(usize, String)>(drag_data_id)).is_some();
    if is_dragging && response.hovered {
        let pointer = ui.ctx().input(|i| i.pointer.latest_pos());
        if let Some(p) = pointer {
            let center = response.row_rect.center().y;
            let new_idx = if p.y < center { idx } else { idx + 1 };
            ui.data_mut(|d| d.insert_temp(drop_index_id, new_idx));
            // Draw drop indicator line
            let line_y = if p.y < center {
                response.row_rect.top()
            } else {
                response.row_rect.bottom()
            };
            ui.painter().line_segment(
                [
                    egui::pos2(response.row_rect.left(), line_y),
                    egui::pos2(response.row_rect.right(), line_y),
                ],
                egui::Stroke::new(2.0, t.palette.accent.primary),
            );
        }
    }

    // Click to activate scene
    if response.row_clicked {
        ctx.commands.push_back(Command::SelectScene(scene_name.to_string()).into());
    }

    // Context menu
    response.response.context_menu(|ui| {
        let entries = vec![
            MenuEntry::item_with_icon(egui_phosphor::regular::CHECK, "Set as active"),
            MenuEntry::separator(),
            MenuEntry::item_with_icon(egui_phosphor::regular::COPY, "Duplicate scene"),
            MenuEntry::item_with_icon(egui_phosphor::regular::TRASH, "Delete scene"),
        ];
        if let Some(menu_idx) = render_menu(ui, &entries) {
            match menu_idx {
                0 => ctx.commands.push_back(Command::SelectScene(scene_name.to_string()).into()),
                2 => ctx.commands.push_back(Command::DuplicateScene(scene_name.to_string()).into()),
                3 => ctx.commands.push_back(Command::DeleteScene(scene_name.to_string()).into()),
                _ => {},
            }
            ui.close();
        }
    });

    // Sub-label: duration + transition
    if is_active {
        ui.horizontal(|ui| {
            ui.add_space(sp.base.component.icon_slot_width + sp.base.space_2);
            ui.label(
                RichText::new(&duration_hint)
                    .size(TextRole::Micro.size())
                    .color(t.palette.text.muted),
            );
        });
        if let Some(hint) = transition_hint {
            ui.horizontal(|ui| {
                ui.add_space(sp.base.component.icon_slot_width + sp.base.space_2);
                ui.label(
                    RichText::new(hint).size(TextRole::Micro.size()).color(t.palette.text.muted),
                );
            });
        }
        ui.add_space(sp.base.space_2);
    }
}

/// Finish a scene drag-and-drop and emit the reorder command.
fn scenes_drop_ui(
    ui: &mut egui::Ui,
    ctx: &mut ScenesContext<'_>,
    scene_names: &[String],
    drag_data_id: egui::Id,
    drop_index_id: egui::Id,
) {
    let drag_active = ui.data(|d| d.get_temp::<(usize, String)>(drag_data_id)).is_some();
    if drag_active
        && ui.input(|i| i.pointer.any_released())
        && let Some((from_idx, _dragged_name)) =
            ui.data(|d| d.get_temp::<(usize, String)>(drag_data_id))
    {
        let to_idx = ui.data(|d| d.get_temp::<usize>(drop_index_id)).unwrap_or(from_idx);
        if from_idx != to_idx && to_idx <= scene_names.len() {
            let mut new_order = scene_names.to_vec();
            let removed = new_order.remove(from_idx);
            let insert_at = if to_idx > from_idx {
                to_idx - 1
            } else {
                to_idx
            };
            new_order.insert(insert_at.min(new_order.len()), removed);
            ctx.commands.push_back(Command::ReorderScenes(new_order).into());
        }
        ui.data_mut(|d| {
            d.remove::<(usize, String)>(drag_data_id);
            d.remove::<usize>(drop_index_id);
        });
    }
}
