use egui::{RichText, Vec2};

use crate::app::components::{self};
use crate::app::design_tokens::semantic::accent;
use crate::app::design_tokens::semantic::text;

use crate::app::design_tokens::spatial::dialog as dialog_token;
use crate::app::design_tokens::typography::TextRole;

use crate::app::GuiShell;

/// Keyboard shortcut groups.
const SHORTCUT_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Playback",
        &[
            ("Space", "Play / Pause"),
            (", / .", "Prev / Next keyframe"),
            ("T (hold)", "Time lens scrub"),
            ("\u{2190} / \u{2192}", "Scrub timeline"),
        ],
    ),
    (
        "Tools",
        &[
            ("Esc", "Select (default)"),
            ("M", "Move"),
            ("Shift + S", "Scale"),
            ("R", "Rotate"),
            ("V", "Vertex edit"),
            ("P", "Pivot"),
        ],
    ),
    (
        "Canvas",
        &[
            ("Drag body", "Move actor"),
            ("Drag handle", "Scale / Rotate"),
            ("Shift + drag", "Constrain / Snap"),
            ("Alt + drag", "Duplicate"),
            ("Middle-drag", "Pan canvas"),
            ("Scroll", "Zoom"),
            ("F / Shift+F", "Zoom to sel. / all"),
        ],
    ),
    (
        "Timeline",
        &[
            ("Click ruler", "Scrub to time"),
            ("Drag ruler", "Scrub"),
            (", / .", "Prev / Next keyframe"),
            ("1 / 2 / 3", "Jump to scene"),
            ("\u{2190} / \u{2192}", "Scrub (no selection)"),
        ],
    ),
    (
        "General",
        &[
            ("Ctrl+S", "Save"),
            ("Ctrl+Z / Ctrl+Shift+Z", "Undo / Redo"),
            ("Ctrl+C / Ctrl+V", "Copy / Paste actors"),
            ("Ctrl+D", "Duplicate selected"),
            ("Ctrl+R / Ctrl+Shift+R", "Reload / Rebuild"),
            ("Ctrl+F", "Find / Replace"),
            ("Ctrl+G / Ctrl+Shift+G", "Group / Ungroup"),
            ("Ctrl+Shift+P", "Command palette"),
            ("A", "Action palette"),
            ("/", "Insertion palette"),
            ("Y", "Toggle editor sync"),
            ("Delete", "Delete selected"),
            ("Esc", "Deselect / Select tool"),
        ],
    ),
];

impl GuiShell {
    pub(crate) fn shortcut_cheat_sheet_ui(&mut self, ui: &mut egui::Ui) {
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let spec = components::dialog::DialogSpec::new(
            "shortcut_cheat_sheet",
            [480.0, 540.0],
        )
        .with_min_size([380.0, 320.0]);

        let open = components::dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let close = components::dialog::title_row(ui, "Keyboard Shortcuts");
            ui.add_space(sp.base.space_3);
            ui.separator();
            ui.add_space(sp.base.space_3);

            egui::ScrollArea::vertical()
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    let avail_w = ui.available_width();
                    let n_cols = if avail_w < dialog_token::SINGLE_COL_THRESHOLD { 1 } else { 2 };
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

fn shortcut_column(ui: &mut egui::Ui, groups: &[(&str, &[(&str, &str)])], width: f32) {
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

            for (key, desc) in *shortcuts {
                shortcut_row(ui, key, desc, width);
            }
            ui.add_space(sp.base.space_3);
        }
    });
}

fn shortcut_row(ui: &mut egui::Ui, key: &str, desc: &str, col_w: f32) {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    // Fixed-width key column — prevents long keys from overlapping the description.
    let key_w = (col_w * dialog_token::KEY_COL_FRAC).min(dialog_token::KEY_COL_MAX);

    ui.horizontal(|ui| {
        ui.add_sized(
            [key_w, sp.base.row_s],
            egui::Label::new(
                RichText::new(key)
                    .monospace()
                    .size(TextRole::BodyS.size())
                    .color(text::SECONDARY),
            )
            .truncate(),
        );
        ui.label(
            RichText::new(desc).size(TextRole::BodyS.size()).color(text::PRIMARY),
        );
    });
}
