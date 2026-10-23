//! Explorer tab.

use super::*;

pub(super) fn explorer_content_ui(ctx: &mut ExplorerContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    // ── Filter input ────────────────────────────────────────────────────────
    let filter_id = egui::Id::new(EXPLORER_FILTER_ID);
    let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();

    ui.horizontal(|ui| {
        ui.add_space(sp.base.space_2);
        let field_w = (ui.available_width() - sp.base.space_2).max(40.0);
        let response = TextField::new(&mut filter)
            .placeholder("Filter files…")
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

    let filter_lower = filter.to_lowercase();
    let has_filter = !filter_lower.is_empty();

    // ── Pre-compute visibility ──────────────────────────────────────────────
    // When a filter is active we build a `show` mask.  The rules are:
    //   1. An entry is visible if its name contains the filter (case-insensitive).
    //   2. If a directory is visible (by name), all its descendants are visible.
    //   3. If any descendant is visible, the ancestor directory is also visible.
    // When no filter is active every entry passes.
    let show = if has_filter {
        let len = ctx.file_tree.len();
        let mut show = vec![false; len];

        // --- Pass 1: direct name match ---
        for (i, entry) in ctx.file_tree.iter().enumerate() {
            show[i] = entry.name.to_lowercase().contains(&filter_lower);
        }

        // --- Pass 2 (forward): expand matching directories to all children ---
        for i in 0..len {
            if show[i] && ctx.file_tree[i].is_dir {
                let parent_depth = ctx.file_tree[i].depth;
                for s in show[(i + 1)..].iter_mut().zip(ctx.file_tree[(i + 1)..].iter()) {
                    if s.1.depth <= parent_depth {
                        break;
                    }
                    *s.0 = true;
                }
            }
        }

        // --- Pass 3 (backward): show ancestors of any visible entry ---
        for i in (0..len).rev() {
            if ctx.file_tree[i].is_dir {
                let parent_depth = ctx.file_tree[i].depth;
                for s in show[(i + 1)..].iter().zip(ctx.file_tree[(i + 1)..].iter()) {
                    if s.1.depth <= parent_depth {
                        break;
                    }
                    if *s.0 {
                        show[i] = true;
                        break;
                    }
                }
            }
        }

        show
    } else {
        vec![true; ctx.file_tree.len()]
    };

    // ── Render visible tree entries ─────────────────────────────────────────
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
        for (i, entry) in ctx.file_tree.iter().enumerate() {
            if !show[i] {
                continue;
            }

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

            let row_id = ui.id().with(entry.path.display().to_string());
            let path = entry.path.clone();
            let is_dir = entry.is_dir;
            let response = row::Row::new(&entry.name)
                .indent(entry.depth as f32 * sp.base.component.icon_slot_width)
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

            if response.chevron_clicked {
                ctx.commands
                    .push_back(ShellAction::Command(Command::ToggleExpandDir(path.clone())));
            }
            if response.row_clicked {
                if is_dir {
                    ctx.commands.push_back(ShellAction::Command(Command::ToggleExpandDir(path)));
                } else {
                    ctx.commands.push_back(ShellAction::Command(Command::OpenFile(path)));
                }
            }
        }
    });
}
