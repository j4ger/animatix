//! Command palette: Cmd+Shift+P searchable list of all commands.

use crate::app::GuiShell;
use crate::app::commands::{
    ActorCommand, DocumentCommand, PlaybackCommand, ShellAction, ViewAction, ViewCommand,
};
use crate::app::components::button::Button;
use crate::app::components::dialog::{self, DialogSpec};
use crate::app::design_tokens::typography::TextRole;

struct PaletteItem {
    label: String,
    icon: &'static str,
    action: ShellAction,
    keywords: &'static str,
}

impl GuiShell {
    pub(crate) fn command_palette_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let spec = DialogSpec::new("command_palette", [480.0, 400.0])
            .with_min_size([400.0, 300.0])
            .with_max_size([600.0, 500.0])
            .with_anchor_offset([0.0, -80.0]);

        let mut commands = Vec::new();
        let mut body_close = false;

        let open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let close = dialog::title_row(ui, "Command Palette");
            ui.add_space(sp.base.space_3);

            // Search input
            let search_resp = ui.add(
                egui::TextEdit::singleline(&mut self.ui_store.command_palette_query)
                    .hint_text("Type a command…")
                    .font(TextRole::Body.font_id())
                    .desired_width(f32::INFINITY)
                    .id_source("cmd_palette_search"),
            );
            search_resp.request_focus();
            ui.add_space(sp.base.space_3);
            ui.separator();
            ui.add_space(sp.base.space_2);

            let query = self.ui_store.command_palette_query.trim().to_string();
            let items = self.build_palette_items();
            let mut scored: Vec<(i32, &PaletteItem)> = items
                .iter()
                .filter_map(|item| {
                    if query.is_empty() {
                        return Some((0, item));
                    }
                    let label = fuzzy_score(&item.label, &query).map(|s| s * 2);
                    let keywords = fuzzy_score(item.keywords, &query);
                    match (label, keywords) {
                        (Some(l), Some(k)) => Some((l.max(k), item)),
                        (Some(l), None) => Some((l, item)),
                        (None, Some(k)) => Some((k, item)),
                        (None, None) => None,
                    }
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(&b.1.label)));
            let filtered: Vec<&PaletteItem> = scored.into_iter().map(|(_, item)| item).collect();

            // Clamp selected index after filtering
            if self.ui_store.command_palette_selected >= filtered.len() {
                self.ui_store.command_palette_selected = filtered.len().saturating_sub(1);
            }

            // Keyboard navigation
            let mut enter_pressed = false;
            ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowDown) {
                    let len = filtered.len();
                    if len > 0 {
                        self.ui_store.command_palette_selected =
                            (self.ui_store.command_palette_selected + 1) % len;
                    }
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    let len = filtered.len();
                    if len > 0 {
                        self.ui_store.command_palette_selected =
                            (self.ui_store.command_palette_selected + len - 1) % len;
                    }
                }
                if i.key_pressed(egui::Key::Enter) {
                    enter_pressed = true;
                }
            });
            if enter_pressed && !filtered.is_empty() {
                let item = filtered[self.ui_store.command_palette_selected];
                commands.push(item.action.clone());
                self.ui_store.command_palette_query.clear();
                body_close = true;
            }

            if filtered.is_empty() {
                ui.label(
                    egui::RichText::new("No commands match your search")
                        .size(TextRole::BodyS.size())
                        .color(theme.text.muted),
                );
            } else {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (idx, item) in filtered.iter().enumerate() {
                        let is_selected = idx == self.ui_store.command_palette_selected;

                        let resp = ui.add_sized(
                            egui::vec2(ui.available_width(), sp.base.row_m),
                            Button::ghost(item.label.clone())
                                .with_icon(item.icon)
                                .active(is_selected),
                        );
                        if resp.clicked() {
                            commands.push(item.action.clone());
                            self.ui_store.command_palette_query.clear();
                            body_close = true;
                        }
                    }
                });
            }

            close || body_close
        });

        if !open {
            self.ui_store.view.command_palette_open = false;
        }

        for action in commands {
            let effects = self.handle_action(action);
            self.apply_effects(effects);
        }
    }

    fn build_palette_items(&self) -> Vec<PaletteItem> {
        let mut items = Vec::new();
        let has_selection = !self.ui_store.selection.selected_actors.is_empty();

        items.push(PaletteItem {
            label: "Open File…".into(),
            icon: egui_phosphor::regular::FOLDER_OPEN,
            action: ShellAction::View(ViewAction::OpenFileDialog),
            keywords: "open file load amx document",
        });
        items.push(PaletteItem {
            label: "New Scene".into(),
            icon: egui_phosphor::regular::FILE_PLUS,
            action: ShellAction::View(ViewAction::NewFile),
            keywords: "new create file scene document",
        });
        items.push(PaletteItem {
            label: "Save As…".into(),
            icon: egui_phosphor::regular::FLOPPY_DISK,
            action: ShellAction::View(ViewAction::SaveAsDialog),
            keywords: "save as file copy document",
        });
        items.push(PaletteItem {
            label: "Insert…".into(),
            icon: egui_phosphor::regular::PLUS,
            action: ShellAction::View(ViewAction::OpenInsertionPalette),
            keywords: "insert add actor primitive component action snippet",
        });
        items.push(PaletteItem {
            label: "Settings…".into(),
            icon: egui_phosphor::regular::GEAR,
            action: ShellAction::View(ViewAction::OpenSettings),
            keywords: "settings preferences options theme density",
        });
        items.push(PaletteItem {
            label: "Keyboard Shortcuts…".into(),
            icon: egui_phosphor::regular::KEYBOARD,
            action: ShellAction::View(ViewAction::OpenShortcuts),
            keywords: "shortcuts keys help cheatsheet",
        });
        items.push(PaletteItem {
            label: "Find / Replace…".into(),
            icon: egui_phosphor::regular::MAGNIFYING_GLASS,
            action: ShellAction::View(ViewAction::OpenFindReplace),
            keywords: "find replace search text",
        });
        items.push(PaletteItem {
            label: "Toggle Inspector".into(),
            icon: egui_phosphor::regular::SLIDERS,
            action: ShellAction::View(ViewAction::ShowInspector),
            keywords: "inspector properties panel detail",
        });
        items.push(PaletteItem {
            label: "Toggle Code".into(),
            icon: egui_phosphor::regular::CODE,
            action: ShellAction::View(ViewAction::ShowCode),
            keywords: "code source editor panel detail",
        });
        for preset in crate::app::LayoutPreset::ALL {
            items.push(PaletteItem {
                label: format!("Layout: {}", preset.label()),
                icon: egui_phosphor::regular::LAYOUT,
                action: ShellAction::View(ViewAction::ApplyLayout(preset)),
                keywords: "layout preset workspace panels",
            });
        }
        items.push(PaletteItem {
            label: "Reset Layout".into(),
            icon: egui_phosphor::regular::ARROW_CLOCKWISE,
            action: ShellAction::View(ViewAction::ResetLayout),
            keywords: "layout reset workspace default panels",
        });

        items.push(PaletteItem {
            label: "Save".into(),
            icon: egui_phosphor::regular::FLOPPY_DISK,
            action: DocumentCommand::Save.into(),
            keywords: "save file disk",
        });
        items.push(PaletteItem {
            label: "Reload".into(),
            icon: egui_phosphor::regular::ARROW_CLOCKWISE,
            action: DocumentCommand::Reload.into(),
            keywords: "reload refresh",
        });
        items.push(PaletteItem {
            label: "Reload Plugins".into(),
            icon: egui_phosphor::regular::PLUG,
            action: DocumentCommand::ReloadPlugins.into(),
            keywords: "plugins reload extension manifest",
        });
        items.push(PaletteItem {
            label: "Rebuild".into(),
            icon: egui_phosphor::regular::ARROWS_CLOCKWISE,
            action: DocumentCommand::Rebuild.into(),
            keywords: "rebuild compile",
        });
        items.push(PaletteItem {
            label: "Export…".into(),
            icon: egui_phosphor::regular::DOWNLOAD,
            action: ShellAction::View(ViewAction::OpenExportDialog),
            keywords: "export render video gif image",
        });
        items.push(PaletteItem {
            label: "Plugins…".into(),
            icon: egui_phosphor::regular::PLUG,
            action: ShellAction::View(ViewAction::OpenPluginStatus),
            keywords: "plugin extensions status reload manifest",
        });
        items.push(PaletteItem {
            label: "Toggle Playback".into(),
            icon: egui_phosphor::regular::PLAY,
            action: PlaybackCommand::TogglePlayback.into(),
            keywords: "play pause playback",
        });
        items.push(PaletteItem {
            label: "Undo".into(),
            icon: egui_phosphor::regular::ARROW_U_UP_LEFT,
            action: DocumentCommand::Undo.into(),
            keywords: "undo revert",
        });
        items.push(PaletteItem {
            label: "Redo".into(),
            icon: egui_phosphor::regular::ARROW_U_UP_RIGHT,
            action: DocumentCommand::Redo.into(),
            keywords: "redo forward",
        });

        if has_selection {
            items.push(PaletteItem {
                label: "Delete Selected Actors".into(),
                icon: egui_phosphor::regular::TRASH,
                action: ActorCommand::DeleteSelectedActors.into(),
                keywords: "delete remove actors",
            });
            items.push(PaletteItem {
                label: "Duplicate Selected Actors".into(),
                icon: egui_phosphor::regular::COPY,
                action: ActorCommand::DuplicateSelectedActors.into(),
                keywords: "duplicate copy actors",
            });
            items.push(PaletteItem {
                label: "Group Selected Actors".into(),
                icon: egui_phosphor::regular::SQUARES_FOUR,
                action: ActorCommand::GroupSelectedActors.into(),
                keywords: "group container",
            });
            items.push(PaletteItem {
                label: "Zoom to Selection".into(),
                icon: egui_phosphor::regular::MAGNIFYING_GLASS_PLUS,
                action: ViewCommand::ZoomToSelection.into(),
                keywords: "zoom fit selection",
            });
        }

        // Align / Distribute commands (requires selection)
        if has_selection {
            items.push(PaletteItem {
                label: "Align Left".into(),
                icon: egui_phosphor::regular::ALIGN_LEFT,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Left).into(),
                keywords: "align left actors selection",
            });
            items.push(PaletteItem {
                label: "Align Center".into(),
                icon: egui_phosphor::regular::ALIGN_CENTER_HORIZONTAL_SIMPLE,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Center).into(),
                keywords: "align center horizontal actors",
            });
            items.push(PaletteItem {
                label: "Align Right".into(),
                icon: egui_phosphor::regular::ALIGN_RIGHT,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Right).into(),
                keywords: "align right actors",
            });
            items.push(PaletteItem {
                label: "Align Top".into(),
                icon: egui_phosphor::regular::ALIGN_TOP,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Top).into(),
                keywords: "align top actors",
            });
            items.push(PaletteItem {
                label: "Align Middle".into(),
                icon: egui_phosphor::regular::ALIGN_CENTER_VERTICAL_SIMPLE,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Middle).into(),
                keywords: "align middle vertical actors",
            });
            items.push(PaletteItem {
                label: "Align Bottom".into(),
                icon: egui_phosphor::regular::ALIGN_BOTTOM,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Bottom).into(),
                keywords: "align bottom actors",
            });
            items.push(PaletteItem {
                label: "Distribute Horizontally".into(),
                icon: egui_phosphor::regular::ARROWS_OUT_LINE_HORIZONTAL,
                action: ActorCommand::DistributeActors(crate::app::commands::Axis::Horizontal)
                    .into(),
                keywords: "distribute horizontal evenly space actors",
            });
            items.push(PaletteItem {
                label: "Distribute Vertically".into(),
                icon: egui_phosphor::regular::ARROWS_OUT_LINE_VERTICAL,
                action: ActorCommand::DistributeActors(crate::app::commands::Axis::Vertical).into(),
                keywords: "distribute vertical evenly space actors",
            });
        }

        items.push(PaletteItem {
            label: "Zoom to Fit All".into(),
            icon: egui_phosphor::regular::ARROWS_IN,
            action: ViewCommand::ZoomToAll.into(),
            keywords: "zoom fit all",
        });

        items
    }
}

/// Subsequence fuzzy match with a relevance score (higher is better).
///
/// Returns `None` when `query` is not a subsequence of `haystack`. Adjacent
/// matches and matches at word boundaries score higher, so "tl" ranks
/// "Toggle Layout" above an incidental scatter of the same letters.
fn fuzzy_score(haystack: &str, query: &str) -> Option<i32> {
    let hay: Vec<char> = haystack.chars().flat_map(|c| c.to_lowercase()).collect();
    let needle: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    if needle.is_empty() {
        return Some(0);
    }

    let mut score = 0i32;
    let mut cursor = 0usize;
    let mut prev: Option<usize> = None;
    for &qc in &needle {
        let mut found = None;
        while cursor < hay.len() {
            if hay[cursor] == qc {
                found = Some(cursor);
                break;
            }
            cursor += 1;
        }
        let idx = found?;
        if prev == Some(idx.wrapping_sub(1)) {
            score += 8;
        }
        if idx == 0 || matches!(hay.get(idx - 1).copied(), Some(' ' | '_' | '-' | '/')) {
            score += 6;
        }
        score += 1;
        prev = Some(idx);
        cursor = idx + 1;
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn fuzzy_requires_subsequence() {
        assert!(fuzzy_score("Toggle Layout", "tl").is_some());
        assert!(fuzzy_score("Save", "xyz").is_none());
    }

    #[test]
    fn fuzzy_prefers_adjacent_and_prefix_matches() {
        // Adjacent run beats the same letters spread out.
        let adjacent = fuzzy_score("abc", "ab").expect("adjacent");
        let spread = fuzzy_score("acb", "ab").expect("spread");
        assert!(adjacent > spread, "adjacent match should outrank a scatter");

        // Prefix match beats a later, non-adjacent match.
        let prefix = fuzzy_score("Export", "ex").expect("prefix");
        let later = fuzzy_score("Export", "et").expect("later");
        assert!(prefix > later, "prefix match should outrank a later match");
    }

    #[test]
    fn fuzzy_is_case_insensitive_and_empty_query_matches() {
        assert!(fuzzy_score("Export", "EXP").is_some());
        assert_eq!(fuzzy_score("anything", ""), Some(0));
    }
}
