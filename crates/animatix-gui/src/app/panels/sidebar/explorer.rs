//! Explorer tab.

use std::sync::Arc;

use super::*;

pub(super) fn explorer_filter_ui(ui: &mut egui::Ui) -> String {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    // ── Filter input ────────────────────────────────────────────────────────
    let filter_id = egui::Id::unique(EXPLORER_FILTER_ID);
    let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();

    ui.horizontal(|ui| {
        ui.add_space(sp.base.space_2);
        let field_w = (ui.available_width() - sp.base.space_2).max(40.0);
        let response = TextField::new(&mut filter)
            .placeholder(&crate::app::i18n::text(ui, "gui-explorer-filter"))
            .cleanable(true)
            .desired_width(field_w)
            .show(ui);
        if response.changed {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
        // If the field was cleared interactively, persist the empty string so
        // the stored value stays in sync (clearing is distinct from never-set).
        if filter.is_empty() && response.lost_focus() {
            ui.data_mut(|d| d.insert_temp(filter_id, String::new()));
        }
    });
    ui.add_space(sp.base.space_2);

    filter
}

pub(super) fn explorer_content_ui(
    ctx: &mut ExplorerContext<'_>,
    ui: &mut egui::Ui,
    filter: &str,
    viewport: egui::Rect,
) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let filtered = filtered_rows(ui, ctx, filter);
    let length = filtered.as_ref().map_or(ctx.file_tree.len(), |indices| indices.len());
    // Keep offscreen rows in the scroll layout without building their widgets.
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
        let row_height = sp.base.row_m;
        let first = ((viewport.top() / row_height).floor() as usize).min(length);
        let end = ((viewport.bottom() / row_height).ceil() as usize).clamp(first, length);
        ui.add_space(first as f32 * row_height);
        ui.skip_ahead_auto_ids(first);
        for row in first..end {
            let index = filtered.as_ref().map_or(row, |indices| indices[row]);
            let entry = &ctx.file_tree[index];
            let is_selected = !entry.is_dir && entry.path == ctx.current_file;
            let is_expanded = entry.is_dir && ctx.expanded_dirs.contains(&entry.path);
            let has_children = entry.is_dir;

            let (icon, label_color) = if entry.is_dir {
                let folder_icon = if is_expanded {
                    egui_phosphor::regular::FOLDER_OPEN
                } else {
                    egui_phosphor::regular::FOLDER
                };
                (Some(folder_icon), None)
            } else {
                let is_amx = entry.path.extension().and_then(|e| e.to_str()) == Some("amx");
                let file_icon = if is_amx {
                    egui_phosphor::regular::FILM_STRIP
                } else {
                    egui_phosphor::regular::FILE
                };
                let color = if is_amx {
                    Some(t.palette.accent.primary)
                } else {
                    None
                };
                (Some(file_icon), color)
            };

            let row_id = ui.scope_id().with(&entry.path);
            let path = entry.path.clone();
            let is_dir = entry.is_dir;
            let indent = entry.depth as f32 * sp.base.component.icon_slot_width;
            let response = row::Row::new(&entry.name)
                .indent(indent.min(ui.available_width() / 2.0))
                .selected(is_selected)
                .icon(icon)
                .label_color(label_color.unwrap_or(t.palette.text.secondary))
                .has_children(has_children)
                .expanded(is_expanded)
                .show(ui, row_id);

            // Right-click context menu (use row response so we don't steal left-clicks)
            response.response.context_menu(|ui| {
                let entries = if !is_dir {
                    vec![MenuEntry::item_with_icon(
                        egui_phosphor::regular::FOLDER_OPEN,
                        "Open",
                    )]
                } else if is_expanded {
                    vec![MenuEntry::item_with_icon(
                        egui_phosphor::regular::CARET_UP,
                        "Collapse",
                    )]
                } else {
                    vec![MenuEntry::item_with_icon(
                        egui_phosphor::regular::CARET_DOWN,
                        "Expand",
                    )]
                };
                if render_menu(ui, &entries).is_some() {
                    if is_dir {
                        ctx.commands.push_back(ShellAction::Command(Command::ToggleExpandDir(
                            path.clone(),
                        )));
                    } else {
                        ctx.commands
                            .push_back(ShellAction::Command(Command::OpenFile(path.clone())));
                    }
                    ui.close();
                }
            });

            if response.row_clicked || response.chevron_clicked {
                if is_dir {
                    ctx.commands.push_back(ShellAction::Command(Command::ToggleExpandDir(path)));
                } else {
                    ctx.commands.push_back(ShellAction::Command(Command::OpenFile(path)));
                }
            }
        }
        ui.add_space((length - end) as f32 * row_height);
    });
}

#[derive(Clone)]
struct CachedFilter {
    query: String,
    revision: u64,
    indices: Arc<[usize]>,
}

fn filtered_rows(ui: &egui::Ui, ctx: &ExplorerContext<'_>, filter: &str) -> Option<Arc<[usize]>> {
    if filter.is_empty() {
        return None;
    }
    let key = ui.scope_id().with("explorer_filter_result");
    if let Some(cached) = ui.data(|data| data.get_temp::<CachedFilter>(key))
        && cached.query == filter
        && cached.revision == ctx.file_tree_revision
    {
        return Some(cached.indices);
    }
    let indices = Arc::from(filtered_entries(ctx.file_tree, &filter.to_lowercase()));
    ui.data_mut(|data| {
        data.insert_temp(
            key,
            CachedFilter {
                query: filter.to_owned(),
                revision: ctx.file_tree_revision,
                indices: Arc::clone(&indices),
            },
        )
    });
    Some(indices)
}

fn filtered_entries(entries: &[FileTreeEntry], filter: &str) -> Vec<usize> {
    let mut matching_dir_depth = None;
    let mut visible = Vec::with_capacity(entries.len());
    for entry in entries {
        if matching_dir_depth.is_some_and(|depth| entry.depth <= depth) {
            matching_dir_depth = None;
        }
        let matches = matching_dir_depth.is_some() || entry.name.to_lowercase().contains(filter);
        if matches && entry.is_dir && matching_dir_depth.is_none() {
            matching_dir_depth = Some(entry.depth);
        }
        visible.push(matches);
    }

    // When a matching descendant makes a directory visible, its depth also
    // marks the boundary between this subtree and its preceding siblings.
    let mut next_visible_depth = None;
    for (entry, show) in entries.iter().zip(&mut visible).rev() {
        if entry.is_dir && next_visible_depth.is_some_and(|depth| depth > entry.depth) {
            *show = true;
        }
        if *show {
            next_visible_depth = Some(entry.depth);
        }
    }
    visible
        .into_iter()
        .enumerate()
        .filter_map(|(index, show)| show.then_some(index))
        .collect()
}
