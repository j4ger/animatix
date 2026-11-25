//! Command palette: Cmd+Shift+P searchable list of all commands.

use eparts::TextField;

use crate::app::GuiShell;
use crate::app::commands::{Command, ShellAction, ViewAction};
use crate::app::components::button::Button;
use crate::app::components::dialog::{self, DialogSpec};
use crate::app::design_tokens::typography::TextRole;
use crate::app::i18n::{Language, current_language, template_title, text, text_for, with_arg};

struct PaletteItem {
    label: String,
    english_label: String,
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
            let close = {
                let title = text(ui, "gui-palette-title");
                dialog::title_row(ui, &title)
            };
            ui.add_space(sp.base.space_3);

            // Search input
            let search_resp = TextField::new(&mut self.ui_store.command_palette_query)
                .placeholder(&text(ui, "gui-palette-placeholder"))
                .cleanable(true)
                .desired_width(ui.available_width())
                .show(ui);
            search_resp.request_focus();
            ui.add_space(sp.base.space_3);
            ui.separator();
            ui.add_space(sp.base.space_2);

            let query = self.ui_store.command_palette_query.trim().to_string();
            let items = self.build_palette_items(current_language(ui.ctx()));
            let mut scored: Vec<(i32, &PaletteItem)> = items
                .iter()
                .filter_map(|item| {
                    if query.is_empty() {
                        return Some((0, item));
                    }
                    let label = fuzzy_score(&item.label, &query)
                        .max(fuzzy_score(&item.english_label, &query))
                        .map(|score| score * 2);
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
                    egui::RichText::new(text(ui, "gui-palette-empty"))
                        .size(TextRole::BodyS.size())
                        .color(theme.palette.text.muted),
                );
            } else {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (idx, item) in filtered.iter().enumerate() {
                        let is_selected = idx == self.ui_store.command_palette_selected;

                        let resp = ui.add_sized(
                            egui::vec2(ui.available_width(), sp.base.row_m),
                            Button::ghost(item.label.clone())
                                .with_icon(item.icon)
                                .left_aligned()
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

    fn build_palette_items(&self, language: Language) -> Vec<PaletteItem> {
        let mut items = Vec::new();
        let has_selection = !self.ui_store.selection.selected_actors.is_empty();

        push_palette_view_items(&mut items, language);
        push_palette_command_items(&mut items, language);

        if has_selection {
            push_palette_selection_items(&mut items, language);
        }

        items.push(PaletteItem {
            label: text_for(language, "gui-command-zoom-fit"),
            english_label: text_for(Language::English, "gui-command-zoom-fit"),
            icon: egui_phosphor::regular::ARROWS_IN,
            action: Command::ZoomToAll.into(),
            keywords: "zoom fit all",
        });

        items
    }
}

/// File, view, and layout palette entries.
fn push_palette_view_items(items: &mut Vec<PaletteItem>, language: Language) {
    items.push(PaletteItem {
        label: text_for(language, "gui-command-open"),
        english_label: text_for(Language::English, "gui-command-open"),
        icon: egui_phosphor::regular::FOLDER_OPEN,
        action: ShellAction::View(ViewAction::OpenFileDialog),
        keywords: "open file load amx document",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-new-scene"),
        english_label: text_for(Language::English, "gui-command-new-scene"),
        icon: egui_phosphor::regular::FILE_PLUS,
        action: ShellAction::View(ViewAction::NewFile),
        keywords: "new create file scene document",
    });
    for template in animatix_render::delivery::ProjectTemplate::ALL {
        let label = |locale| {
            with_arg(
                locale,
                "gui-command-new-project",
                "template",
                template_title(locale, template),
            )
        };
        items.push(PaletteItem {
            label: label(language),
            english_label: label(Language::English),
            icon: egui_phosphor::regular::FILE_PLUS,
            action: ShellAction::View(ViewAction::NewProject(template)),
            keywords: "new create starter template project",
        });
    }
    items.push(PaletteItem {
        label: text_for(language, "gui-command-save-as"),
        english_label: text_for(Language::English, "gui-command-save-as"),
        icon: egui_phosphor::regular::FLOPPY_DISK,
        action: ShellAction::View(ViewAction::SaveAsDialog),
        keywords: "save as file copy document",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-settings"),
        english_label: text_for(Language::English, "gui-command-settings"),
        icon: egui_phosphor::regular::GEAR,
        action: ShellAction::View(ViewAction::OpenSettings),
        keywords: "settings preferences options theme density",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-shortcuts"),
        english_label: text_for(Language::English, "gui-command-shortcuts"),
        icon: egui_phosphor::regular::KEYBOARD,
        action: ShellAction::View(ViewAction::OpenShortcuts),
        keywords: "shortcuts keys help cheatsheet",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-find-replace"),
        english_label: text_for(Language::English, "gui-command-find-replace"),
        icon: egui_phosphor::regular::MAGNIFYING_GLASS,
        action: ShellAction::View(ViewAction::OpenFindReplace),
        keywords: "find replace search text",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-inspector"),
        english_label: text_for(Language::English, "gui-command-inspector"),
        icon: egui_phosphor::regular::SLIDERS,
        action: ShellAction::View(ViewAction::ShowInspector),
        keywords: "inspector properties panel detail",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-code"),
        english_label: text_for(Language::English, "gui-command-code"),
        icon: egui_phosphor::regular::CODE,
        action: ShellAction::View(ViewAction::ShowCode),
        keywords: "mode layout write review toggle code source editor",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-curves"),
        english_label: text_for(Language::English, "gui-command-curves"),
        icon: egui_phosphor::regular::CHART_LINE,
        action: ShellAction::View(ViewAction::ShowCurves),
        keywords: "toggle curves review graph fcurve keyframe easing panel bottom",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-timeline"),
        english_label: text_for(Language::English, "gui-command-timeline"),
        icon: egui_phosphor::regular::FILM_STRIP,
        action: ShellAction::View(ViewAction::ShowTimeline),
        keywords: "timeline keyframes panel bottom",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-preview-fullscreen"),
        english_label: text_for(Language::English, "gui-command-preview-fullscreen"),
        icon: egui_phosphor::regular::CORNERS_OUT,
        action: ShellAction::View(ViewAction::TogglePreviewFullscreen),
        keywords: "focus preview fullscreen full screen",
    });
    for preset in crate::app::LayoutPreset::ALL {
        let label = |locale| {
            with_arg(locale, "gui-command-layout", "layout", text_for(locale, preset.message_id()))
        };
        items.push(PaletteItem {
            label: label(language),
            english_label: label(Language::English),
            icon: egui_phosphor::regular::LAYOUT,
            action: ShellAction::View(ViewAction::ApplyLayout(preset)),
            keywords: "layout preset workspace panels",
        });
    }
    items.push(PaletteItem {
        label: text_for(language, "gui-command-reset-layout"),
        english_label: text_for(Language::English, "gui-command-reset-layout"),
        icon: egui_phosphor::regular::ARROW_CLOCKWISE,
        action: ShellAction::View(ViewAction::ResetLayout),
        keywords: "layout reset workspace default panels",
    });
}

/// Document, playback, and zoom command entries.
fn push_palette_command_items(items: &mut Vec<PaletteItem>, language: Language) {
    items.push(PaletteItem {
        label: text_for(language, "gui-command-save"),
        english_label: text_for(Language::English, "gui-command-save"),
        icon: egui_phosphor::regular::FLOPPY_DISK,
        action: Command::Save.into(),
        keywords: "save file disk",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-reload"),
        english_label: text_for(Language::English, "gui-command-reload"),
        icon: egui_phosphor::regular::ARROW_CLOCKWISE,
        action: Command::Reload.into(),
        keywords: "reload refresh",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-reload-plugins"),
        english_label: text_for(Language::English, "gui-command-reload-plugins"),
        icon: egui_phosphor::regular::PLUG,
        action: Command::ReloadPlugins.into(),
        keywords: "plugins reload extension manifest",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-rebuild"),
        english_label: text_for(Language::English, "gui-command-rebuild"),
        icon: egui_phosphor::regular::ARROWS_CLOCKWISE,
        action: Command::Rebuild.into(),
        keywords: "rebuild compile",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-export"),
        english_label: text_for(Language::English, "gui-command-export"),
        icon: egui_phosphor::regular::DOWNLOAD,
        action: ShellAction::View(ViewAction::OpenExportDialog),
        keywords: "export render video gif image",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-plugins"),
        english_label: text_for(Language::English, "gui-command-plugins"),
        icon: egui_phosphor::regular::PLUG,
        action: ShellAction::View(ViewAction::OpenPluginStatus),
        keywords: "plugin extensions status reload manifest",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-playback"),
        english_label: text_for(Language::English, "gui-command-playback"),
        icon: egui_phosphor::regular::PLAY,
        action: Command::TogglePlayback.into(),
        keywords: "play pause playback",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-presentation"),
        english_label: text_for(Language::English, "gui-command-presentation"),
        icon: egui_phosphor::regular::PROJECTOR_SCREEN,
        action: Command::TogglePresentationMode.into(),
        keywords: "presentation mode slideshow powerpoint f5 full screen",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-step-next"),
        english_label: text_for(Language::English, "gui-command-step-next"),
        icon: egui_phosphor::regular::CARET_RIGHT,
        action: Command::StepNext.into(),
        keywords: "step next slide pause point pagedown",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-step-previous"),
        english_label: text_for(Language::English, "gui-command-step-previous"),
        icon: egui_phosphor::regular::CARET_LEFT,
        action: Command::StepPrevious.into(),
        keywords: "step previous slide pause point pageup",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-undo"),
        english_label: text_for(Language::English, "gui-command-undo"),
        icon: egui_phosphor::regular::ARROW_U_UP_LEFT,
        action: Command::Undo.into(),
        keywords: "undo revert",
    });
    items.push(PaletteItem {
        label: text_for(language, "gui-command-redo"),
        english_label: text_for(Language::English, "gui-command-redo"),
        icon: egui_phosphor::regular::ARROW_U_UP_RIGHT,
        action: Command::Redo.into(),
        keywords: "redo forward",
    });
}

/// Selection-dependent actor commands.
fn push_palette_selection_items(items: &mut Vec<PaletteItem>, language: Language) {
    items.push(PaletteItem {
        label: text_for(language, "gui-command-zoom-selection"),
        english_label: text_for(Language::English, "gui-command-zoom-selection"),
        icon: egui_phosphor::regular::MAGNIFYING_GLASS_PLUS,
        action: Command::ZoomToSelection.into(),
        keywords: "zoom fit selection",
    });
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
