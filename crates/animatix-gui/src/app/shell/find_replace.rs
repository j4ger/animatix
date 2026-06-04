//! Find / Replace dialog for the source editor.

use crate::app::design_tokens::*;
use crate::app::GuiShell;

impl GuiShell {
    pub(crate) fn find_replace_ui(&mut self,
        ui: &mut egui::Ui,
    ) {
        let screen_rect = ui.ctx().viewport_rect();
        ui.painter().rect_filled(screen_rect, 0.0, overlay_backdrop());

        // Close on Escape or backdrop click
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.ui_store.view.find_replace_open = false;
        }
        let backdrop = ui.interact(screen_rect, ui.id().with("find_replace_backdrop"), egui::Sense::click());
        if backdrop.clicked() {
            self.ui_store.view.find_replace_open = false;
        }

        egui::Window::new("")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_size([420.0, 160.0])
            .min_size([380.0, 140.0])
            .resizable(false)
            .collapsible(false)
            .title_bar(false)
            .frame(
                egui::Frame::new()
                    .fill(BG_BASE)
                    .stroke(egui::Stroke::new(STROKE_WIDTH, BORDER))
                    .corner_radius(RADIUS_XL)
                    .inner_margin(egui::Margin::same(SPACE_XL as i8)),
            )
            .show(ui.ctx(), |ui| {
                ui.set_min_width(340.0);

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Find & Replace")
                            .size(FONT_SIZE_XL)
                            .color(TEXT_PRIMARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui_phosphor::regular::X).clicked() {
                            self.ui_store.view.find_replace_open = false;
                        }
                    });
                });
                ui.add_space(SPACE_M);
                ui.separator();
                ui.add_space(SPACE_M);

                ui.label(egui::RichText::new("Find").size(FONT_SIZE_S).color(TEXT_SECONDARY));
                ui.add(
                    egui::TextEdit::singleline(&mut self.ui_store.find_query)
                        .desired_width(f32::INFINITY)
                        .hint_text("Search term…"),
                );
                ui.add_space(SPACE_S);

                ui.label(egui::RichText::new("Replace with").size(FONT_SIZE_S).color(TEXT_SECONDARY));
                ui.add(
                    egui::TextEdit::singleline(&mut self.ui_store.replace_query)
                        .desired_width(f32::INFINITY)
                        .hint_text("Replacement…"),
                );
                ui.add_space(SPACE_M);

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let replace_all = ui
                            .add_sized(
                                [100.0, ROW_M],
                                egui::Button::new(
                                    egui::RichText::new("Replace All")
                                        .size(FONT_SIZE_S)
                                        .color(TEXT_PRIMARY),
                                )
                                .fill(ACCENT_BLUE),
                            );
                        if replace_all.clicked() {
                            self.perform_find_replace_all();
                        }

                        let find_next = ui
                            .add_sized(
                                [90.0, ROW_M],
                                egui::Button::new(
                                    egui::RichText::new("Find Next")
                                        .size(FONT_SIZE_S)
                                        .color(TEXT_SECONDARY),
                                )
                                .fill(BG_WIDGET),
                            );
                        if find_next.clicked() {
                            self.find_next_in_editor();
                        }
                    });
                });
            });
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
        self.document_store.source.editor.replace_text(new_text.clone());
        self.document_store.source.document.source_text = new_text;
        self.document_store.source.document.is_dirty = true;
        self.preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms));
        self.preview_store.preview.status = format!("Replaced {} occurrence(s)", count);
    }

    fn find_next_in_editor(&mut self) {
        let find = &self.ui_store.find_query;
        if find.is_empty() {
            self.preview_store.preview.status = "Find query is empty".to_string();
            return;
        }

        let text = self.document_store.source.editor.text();
        // Simple find — scroll to first occurrence for now
        if let Some(pos) = text.find(find) {
            let (line, _col) = self.document_store.source.editor.byte_to_line_col(pos);
            self.document_store.source.editor.scroll_to_line(line);
            self.preview_store.preview.status = format!("Found at line {}", line + 1);
        } else {
            self.preview_store.preview.status = "No matches found".to_string();
        }
    }
}
