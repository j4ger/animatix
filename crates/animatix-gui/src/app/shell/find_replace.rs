//! Find / Replace dialog for the source editor.

use crate::app::components::dialog;
use crate::app::GuiShell;
use crate::app::commands::UndoLabel;
use crate::app::design_tokens::semantic::accent;
use crate::app::design_tokens::semantic::surface;

use crate::app::design_tokens::semantic::text;

use crate::app::design_tokens::spatial::{ROW_M, SPACE_M, SPACE_S};
use crate::app::design_tokens::typography::TextRole;

impl GuiShell {
    pub(crate) fn find_replace_ui(&mut self, ui: &mut egui::Ui) {
        let spec = dialog::DialogSpec::new("find_replace", [420.0, 160.0])
            .with_min_size([380.0, 140.0]);

        let open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let close = dialog::title_row(ui, "Find & Replace");
            ui.add_space(SPACE_M);
            ui.separator();
            ui.add_space(SPACE_M);

                ui.label(
                    egui::RichText::new("Find").size(TextRole::BodyS.size()).color(text::SECONDARY),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.ui_store.find_query)
                        .desired_width(f32::INFINITY)
                        .hint_text("Search term…"),
                );
                ui.add_space(SPACE_S);

                ui.label(
                    egui::RichText::new("Replace with")
                        .size(TextRole::BodyS.size())
                        .color(text::SECONDARY),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.ui_store.replace_query)
                        .desired_width(f32::INFINITY)
                        .hint_text("Replacement…"),
                );
                ui.add_space(SPACE_M);

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let replace_all = ui.add_sized(
                            [100.0, ROW_M],
                            egui::Button::new(
                                egui::RichText::new("Replace All")
                                    .size(TextRole::BodyS.size())
                                    .color(text::PRIMARY),
                            )
                            .fill(accent::PRIMARY),
                        );
                        if replace_all.clicked() {
                            self.perform_find_replace_all();
                        }

                        let find_next = ui.add_sized(
                            [90.0, ROW_M],
                            egui::Button::new(
                                egui::RichText::new("Find Next")
                                    .size(TextRole::BodyS.size())
                                    .color(text::SECONDARY),
                            )
                            .fill(surface::WIDGET),
                        );
                        if find_next.clicked() {
                            self.find_next_in_editor();
                        }
                    });
                });

            close
        });

        if !open {
            self.ui_store.view.find_replace_open = false;
        }
    }

    fn perform_find_replace_all(&mut self) {
        let find = &self.ui_store.find_query;
        let replace = &self.ui_store.replace_query;
        if find.is_empty() {
            self.preview_store.preview.status = "Find query is empty".to_string();
            return;
        }

        let text = self.document_store.source.editor.text().to_string();
        let count = text.matches(find).count();
        if count == 0 {
            self.preview_store.preview.status = "No matches found".to_string();
            return;
        }

        let new_text = text.replace(find, replace);
        self.document_store.snapshot(UndoLabel::FindReplaceAll);
        self.document_store.replace_text(new_text);
        self.document_store.source.document.raw_statements = None;
        self.document_store.source.document.expanded_statements = None;
        self.preview_store.pending_rebuild_at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
        );
        self.ui_store.find_last_match = None;
        self.preview_store.preview.status = format!("Replaced {} occurrence(s)", count);
    }

    fn find_next_in_editor(&mut self) {
        let find = &self.ui_store.find_query;
        if find.is_empty() {
            self.preview_store.preview.status = "Find query is empty".to_string();
            return;
        }

        let text = self.document_store.source.editor.text();
        let text_len = text.len();

        // Start searching from after the last match, or from the beginning
        let start = self.ui_store.find_last_match.map(|p| (p + 1).min(text_len)).unwrap_or(0);

        // Search forward from cursor position
        if start < text_len {
            if let Some(pos) = text[start..].find(find) {
                let abs_pos = start + pos;
                self.ui_store.find_last_match = Some(abs_pos);
                let (line, _col) = self.document_store.source.editor.byte_to_line_col(abs_pos);
                self.document_store.source.editor.scroll_to_line(line);
                self.preview_store.preview.status = format!("Found at line {}", line + 1);
                return;
            }
        }

        // Not found after cursor — try wrapping from the start
        if let Some(pos) = text[..start.min(text_len)].find(find) {
            self.ui_store.find_last_match = Some(pos);
            let (line, _col) = self.document_store.source.editor.byte_to_line_col(pos);
            self.document_store.source.editor.scroll_to_line(line);
            self.preview_store.preview.status =
                format!("Wrapped to top — found at line {}", line + 1);
            return;
        }

        // No matches at all
        self.preview_store.preview.status = "No matches found".to_string();
    }
}
