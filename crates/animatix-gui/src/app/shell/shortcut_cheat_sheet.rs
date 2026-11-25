use egui::{RichText, Vec2};
use eparts::Kbd;

use crate::app::GuiShell;
use crate::app::components::{self};
use crate::app::design_tokens::spatial::dialog as dialog_token;
use crate::app::design_tokens::typography::TextRole;
use crate::app::interaction::keyboard::{ShortcutRegistry, shortcut_hints_for_name};

/// A cheat-sheet row. Registry entries render the actual key binding from
/// the shell's `ShortcutRegistry`; gesture entries are pointer interactions or
/// stateful keys that are not registered as one-shot shortcuts.
#[derive(Clone, Copy)]
enum CheatSheetEntry {
    Bindings {
        names: &'static [&'static str],
        desc: &'static str,
    },
    Gesture {
        key: &'static str,
        desc: &'static str,
    },
}

/// Keyboard shortcut groups.
const SHORTCUT_GROUPS: &[(&str, &[CheatSheetEntry])] = &[
    (
        "gui-shortcut-sheet-playback",
        &[
            CheatSheetEntry::Bindings {
                names: &["Play/Pause"],
                desc: "gui-shortcut-sheet-play-pause",
            },
            CheatSheetEntry::Bindings {
                names: &["Prev Keyframe", "Next Keyframe"],
                desc: "gui-shortcut-sheet-prev-next-keyframe",
            },
            CheatSheetEntry::Bindings {
                names: &["Step Back 1 Frame", "Step Forward 1 Frame"],
                desc: "gui-shortcut-sheet-prev-next-frame",
            },
            CheatSheetEntry::Gesture {
                key: "gui-shortcut-sheet-t-hold",
                desc: "gui-shortcut-sheet-time-lens-scrub",
            },
            CheatSheetEntry::Gesture {
                key: "\u{2190} / \u{2192}",
                desc: "gui-shortcut-sheet-scrub-timeline",
            },
            CheatSheetEntry::Bindings {
                names: &["Toggle Presentation Mode"],
                desc: "gui-shortcut-sheet-presentation-mode",
            },
            CheatSheetEntry::Bindings {
                names: &["Step Previous", "Step Next"],
                desc: "gui-shortcut-sheet-step-prev-next",
            },
        ],
    ),
    (
        "gui-shortcut-sheet-selection",
        &[
            CheatSheetEntry::Gesture {
                key: "gui-shortcut-sheet-drag",
                desc: "gui-shortcut-sheet-marquee-selection",
            },
            CheatSheetEntry::Gesture {
                key: "gui-shortcut-sheet-click",
                desc: "gui-shortcut-sheet-inspect-actor",
            },
            CheatSheetEntry::Gesture {
                key: "gui-shortcut-sheet-shift-click",
                desc: "gui-shortcut-sheet-extend-selection",
            },
            CheatSheetEntry::Gesture {
                key: "gui-shortcut-sheet-alt-click",
                desc: "gui-shortcut-sheet-locate-source",
            },
        ],
    ),
    (
        "gui-shortcut-sheet-canvas",
        &[
            CheatSheetEntry::Gesture {
                key: "gui-shortcut-sheet-middle-drag",
                desc: "gui-shortcut-sheet-pan-canvas",
            },
            CheatSheetEntry::Gesture {
                key: "gui-shortcut-sheet-scroll",
                desc: "gui-shortcut-sheet-zoom",
            },
            CheatSheetEntry::Bindings {
                names: &["Zoom to Selection", "Zoom to All"],
                desc: "gui-shortcut-sheet-zoom-to-sel-all",
            },
        ],
    ),
    (
        "gui-shortcut-sheet-timeline",
        &[
            CheatSheetEntry::Gesture {
                key: "gui-shortcut-sheet-click-ruler",
                desc: "gui-shortcut-sheet-scrub-to-time",
            },
            CheatSheetEntry::Gesture {
                key: "gui-shortcut-sheet-drag-ruler",
                desc: "gui-shortcut-sheet-scrub",
            },
            CheatSheetEntry::Bindings {
                names: &["Prev Keyframe", "Next Keyframe"],
                desc: "gui-shortcut-sheet-prev-next-keyframe",
            },
            CheatSheetEntry::Bindings {
                names: &["Scene 1", "Scene 2", "Scene 3"],
                desc: "gui-shortcut-sheet-jump-to-scene",
            },
            CheatSheetEntry::Bindings {
                names: &["Scrub Backward", "Scrub Forward"],
                desc: "gui-shortcut-sheet-scrub-in-small-steps",
            },
        ],
    ),
    (
        "gui-shortcut-sheet-general",
        &[
            CheatSheetEntry::Bindings {
                names: &["Save"],
                desc: "gui-shortcut-sheet-save",
            },
            CheatSheetEntry::Bindings {
                names: &["Undo", "Redo"],
                desc: "gui-shortcut-sheet-undo-redo",
            },
            CheatSheetEntry::Bindings {
                names: &["Reload", "Rebuild"],
                desc: "gui-shortcut-sheet-reload-rebuild",
            },
            CheatSheetEntry::Bindings {
                names: &["Find/Replace"],
                desc: "gui-shortcut-sheet-find-replace",
            },
            CheatSheetEntry::Bindings {
                names: &["Command Palette"],
                desc: "gui-shortcut-sheet-command-palette",
            },
            CheatSheetEntry::Bindings {
                names: &["Editor Sync"],
                desc: "gui-shortcut-sheet-toggle-editor-sync",
            },
            CheatSheetEntry::Bindings {
                names: &["Escape"],
                desc: "gui-shortcut-sheet-cancel-selection-presentation",
            },
        ],
    ),
];

impl GuiShell {
    pub(crate) fn shortcut_cheat_sheet_ui(&mut self, ui: &mut egui::Ui) {
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let spec = components::dialog::DialogSpec::new("shortcut_cheat_sheet", [480.0, 540.0])
            .with_min_size([380.0, 320.0]);

        let open = components::dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let close = {
                let title = crate::app::i18n::text(ui, "gui-settings-section-shortcuts");
                components::dialog::title_row(ui, &title)
            };
            ui.add_space(sp.base.space_3);
            ui.separator();
            ui.add_space(sp.base.space_3);

            egui::ScrollArea::vertical().max_height(ui.available_height()).show(ui, |ui| {
                let avail_w = ui.available_width();
                let n_cols = if avail_w < dialog_token::SINGLE_COL_THRESHOLD {
                    1
                } else {
                    2
                };
                let col_w = (avail_w - sp.dialog.col_gap * (n_cols - 1) as f32) / n_cols as f32;

                if n_cols == 1 {
                    // Single column — render all groups
                    for group in SHORTCUT_GROUPS {
                        shortcut_column(
                            ui,
                            std::slice::from_ref(group),
                            col_w,
                            &self.shortcut_registry,
                        );
                    }
                } else {
                    // Two columns — split groups across columns
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(sp.dialog.col_gap, 0.0);

                        let mid = SHORTCUT_GROUPS.len().div_ceil(2);
                        shortcut_column(
                            ui,
                            &SHORTCUT_GROUPS[..mid],
                            col_w,
                            &self.shortcut_registry,
                        );
                        shortcut_column(
                            ui,
                            &SHORTCUT_GROUPS[mid..],
                            col_w,
                            &self.shortcut_registry,
                        );
                    });
                }
            });
            close
        });

        if !open {
            self.ui_store.view.shortcuts_open = false;
        }
    }
}

fn shortcut_column(
    ui: &mut egui::Ui,
    groups: &[(&str, &[CheatSheetEntry])],
    width: f32,
    registry: &ShortcutRegistry,
) {
    let theme = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    ui.vertical(|ui| {
        ui.set_min_width(width);
        for (title, shortcuts) in groups {
            ui.label(
                RichText::new(crate::app::i18n::text(ui, title))
                    .size(TextRole::BodyS.size())
                    .color(theme.palette.accent.primary)
                    .strong(),
            );
            ui.add_space(sp.base.space_1);

            for entry in *shortcuts {
                shortcut_row(ui, *entry, width, registry);
            }
            ui.add_space(sp.base.space_3);
        }
    });
}

fn shortcut_row(
    ui: &mut egui::Ui,
    entry: CheatSheetEntry,
    col_w: f32,
    registry: &ShortcutRegistry,
) {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    match entry {
        CheatSheetEntry::Bindings { names, desc } => {
            let hints: Vec<String> = names
                .iter()
                .flat_map(|name| shortcut_hints_for_name(registry, name, ui.ctx()))
                .collect();
            if hints.is_empty() {
                return;
            }
            let key = hints.join(" / ");
            let desc = crate::app::i18n::text(ui, desc);
            shortcut_row_inner(ui, &key, &desc, col_w, sp.base.row_s);
        },
        CheatSheetEntry::Gesture { key, desc } => {
            let key = if key.starts_with("gui-") {
                crate::app::i18n::text(ui, key)
            } else {
                key.to_owned()
            };
            let desc = crate::app::i18n::text(ui, desc);
            shortcut_row_inner(ui, &key, &desc, col_w, sp.base.row_s);
        },
    }
}

fn shortcut_row_inner(ui: &mut egui::Ui, key: &str, desc: &str, col_w: f32, row_height: f32) {
    let theme = eparts::theme(ui);
    // Fixed-width key column — prevents long keys from overlapping the description.
    let key_w = (col_w * dialog_token::KEY_COL_FRAC).min(dialog_token::KEY_COL_MAX);

    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(key_w, row_height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add(Kbd::new(key));
            },
        );
        ui.label(
            RichText::new(desc)
                .size(TextRole::BodyS.size())
                .color(theme.palette.text.primary),
        );
    });
}
