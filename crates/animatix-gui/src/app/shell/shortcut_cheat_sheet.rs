use egui::{RichText, Vec2};

use crate::app::GuiShell;
use crate::app::components::{self};
use crate::app::design_tokens::semantic::{accent, text};
use crate::app::design_tokens::spatial::dialog as dialog_token;
use crate::app::design_tokens::typography::TextRole;
use crate::app::interaction::keyboard::shortcut_hints_for_name;

/// A cheat-sheet row. Registry entries render the actual key binding from
/// `SHORTCUT_REGISTRY`; gesture entries are pointer interactions or stateful
/// keys that are not registered as one-shot shortcuts.
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
        "Playback",
        &[
            CheatSheetEntry::Bindings {
                names: &["Play/Pause"],
                desc: "Play / Pause",
            },
            CheatSheetEntry::Bindings {
                names: &["Prev Keyframe", "Next Keyframe"],
                desc: "Prev / Next keyframe",
            },
            CheatSheetEntry::Gesture {
                key: "T (hold)",
                desc: "Time lens scrub",
            },
            CheatSheetEntry::Gesture {
                key: "\u{2190} / \u{2192}",
                desc: "Scrub timeline",
            },
        ],
    ),
    (
        "Tools",
        &[
            CheatSheetEntry::Gesture {
                key: "Esc",
                desc: "Select (default)",
            },
            CheatSheetEntry::Bindings {
                names: &["Move Tool"],
                desc: "Move",
            },
            CheatSheetEntry::Bindings {
                names: &["Scale Tool"],
                desc: "Scale",
            },
            CheatSheetEntry::Bindings {
                names: &["Rotate Tool"],
                desc: "Rotate",
            },
            CheatSheetEntry::Bindings {
                names: &["Vertex Tool"],
                desc: "Vertex edit",
            },
            CheatSheetEntry::Bindings {
                names: &["Pivot Tool"],
                desc: "Pivot",
            },
        ],
    ),
    (
        "Canvas",
        &[
            CheatSheetEntry::Gesture {
                key: "Drag body",
                desc: "Move actor",
            },
            CheatSheetEntry::Gesture {
                key: "Drag handle",
                desc: "Scale / Rotate",
            },
            CheatSheetEntry::Gesture {
                key: "Shift + drag",
                desc: "Constrain / Snap",
            },
            CheatSheetEntry::Gesture {
                key: "Alt + drag",
                desc: "Duplicate",
            },
            CheatSheetEntry::Gesture {
                key: "Middle-drag",
                desc: "Pan canvas",
            },
            CheatSheetEntry::Gesture {
                key: "Scroll",
                desc: "Zoom",
            },
            CheatSheetEntry::Bindings {
                names: &["Zoom to Selection", "Zoom to All"],
                desc: "Zoom to sel. / all",
            },
        ],
    ),
    (
        "Timeline",
        &[
            CheatSheetEntry::Gesture {
                key: "Click ruler",
                desc: "Scrub to time",
            },
            CheatSheetEntry::Gesture {
                key: "Drag ruler",
                desc: "Scrub",
            },
            CheatSheetEntry::Bindings {
                names: &["Prev Keyframe", "Next Keyframe"],
                desc: "Prev / Next keyframe",
            },
            CheatSheetEntry::Bindings {
                names: &["Scene 1", "Scene 2", "Scene 3"],
                desc: "Jump to scene",
            },
            CheatSheetEntry::Gesture {
                key: "\u{2190} / \u{2192}",
                desc: "Scrub (no selection)",
            },
        ],
    ),
    (
        "General",
        &[
            CheatSheetEntry::Bindings {
                names: &["Save"],
                desc: "Save",
            },
            CheatSheetEntry::Bindings {
                names: &["Undo", "Redo"],
                desc: "Undo / Redo",
            },
            CheatSheetEntry::Bindings {
                names: &["Copy", "Paste"],
                desc: "Copy / Paste actors",
            },
            CheatSheetEntry::Bindings {
                names: &["Duplicate"],
                desc: "Duplicate selected",
            },
            CheatSheetEntry::Bindings {
                names: &["Reload", "Rebuild"],
                desc: "Reload / Rebuild",
            },
            CheatSheetEntry::Bindings {
                names: &["Find/Replace"],
                desc: "Find / Replace",
            },
            CheatSheetEntry::Bindings {
                names: &["Group", "Ungroup"],
                desc: "Group / Ungroup",
            },
            CheatSheetEntry::Bindings {
                names: &["Command Palette"],
                desc: "Command palette",
            },
            CheatSheetEntry::Bindings {
                names: &[
                    "Insertion Palette (Actions)",
                    "Insertion Palette (Universal)",
                ],
                desc: "Insertion palette",
            },
            CheatSheetEntry::Bindings {
                names: &["Editor Sync"],
                desc: "Toggle editor sync",
            },
            CheatSheetEntry::Bindings {
                names: &["Delete"],
                desc: "Delete selected",
            },
            CheatSheetEntry::Bindings {
                names: &["Escape"],
                desc: "Deselect / Select tool",
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
            let close = components::dialog::title_row(ui, "Keyboard Shortcuts");
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
                        shortcut_column(ui, std::slice::from_ref(group), col_w);
                    }
                } else {
                    // Two columns — split groups across columns
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(sp.dialog.col_gap, 0.0);

                        let mid = SHORTCUT_GROUPS.len().div_ceil(2);
                        shortcut_column(ui, &SHORTCUT_GROUPS[..mid], col_w);
                        shortcut_column(ui, &SHORTCUT_GROUPS[mid..], col_w);
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

fn shortcut_column(ui: &mut egui::Ui, groups: &[(&str, &[CheatSheetEntry])], width: f32) {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    ui.vertical(|ui| {
        ui.set_min_width(width);
        for (title, shortcuts) in groups {
            ui.label(
                RichText::new(*title)
                    .size(TextRole::BodyS.size())
                    .color(accent::PRIMARY)
                    .strong(),
            );
            ui.add_space(sp.base.space_1);

            for entry in *shortcuts {
                shortcut_row(ui, *entry, width);
            }
            ui.add_space(sp.base.space_3);
        }
    });
}

fn shortcut_row(ui: &mut egui::Ui, entry: CheatSheetEntry, col_w: f32) {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    match entry {
        CheatSheetEntry::Bindings { names, desc } => {
            let hints: Vec<String> =
                names.iter().flat_map(|name| shortcut_hints_for_name(name, ui.ctx())).collect();
            let key = if hints.is_empty() {
                names.join(" / ")
            } else {
                hints.join(" / ")
            };
            shortcut_row_inner(ui, &key, desc, col_w, sp.base.row_s);
        },
        CheatSheetEntry::Gesture { key, desc } => {
            shortcut_row_inner(ui, key, desc, col_w, sp.base.row_s);
        },
    }
}

fn shortcut_row_inner(ui: &mut egui::Ui, key: &str, desc: &str, col_w: f32, row_height: f32) {
    // Fixed-width key column — prevents long keys from overlapping the description.
    let key_w = (col_w * dialog_token::KEY_COL_FRAC).min(dialog_token::KEY_COL_MAX);

    ui.horizontal(|ui| {
        ui.add_sized(
            [key_w, row_height],
            egui::Label::new(
                RichText::new(key)
                    .monospace()
                    .size(TextRole::BodyS.size())
                    .color(text::SECONDARY),
            )
            .truncate(),
        );
        ui.label(RichText::new(desc).size(TextRole::BodyS.size()).color(text::PRIMARY));
    });
}
