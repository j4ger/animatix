use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2};

use crate::app::design_tokens::*;
use crate::app::GuiShell;

/// A categorized action entry: (category name, category color, list of (verb, display label)).
type ActionCategory = (&'static str, Color32, &'static [(&'static str, &'static str)]);

/// Action categories with their color and actions.
const ACTION_CATEGORIES: &[ActionCategory] = &[
    (
        "Entrance",
        GREEN,
        &[
            ("fade-in", "Fade In"),
            ("wipe-in", "Wipe In"),
            ("reveal-in", "Reveal In"),
            ("draw-in", "Draw In"),
        ],
    ),
    (
        "Exit",
        RED,
        &[
            ("fade-out", "Fade Out"),
            ("wipe-out", "Wipe Out"),
        ],
    ),
    (
        "Motion",
        ACCENT_BLUE,
        &[
            ("move", "Move"),
            ("rotate", "Rotate"),
            ("scale", "Scale"),
            ("shift", "Shift"),
        ],
    ),
    (
        "Effects",
        AMBER,
        &[
            ("shake", "Shake"),
            ("pulse", "Pulse"),
            ("bounce", "Bounce"),
        ],
    ),
];

impl GuiShell {
    pub(crate) fn action_palette_ui(
        &mut self,
        ui: &mut egui::Ui,
    ) {
        let screen_rect = ui.ctx().viewport_rect();

        // Dark semi-transparent backdrop
        ui.painter().rect_filled(screen_rect, 0.0, overlay_backdrop());

        // Capture clicks on backdrop to close
        let backdrop_response = ui.interact(
            screen_rect,
            ui.id().with("action_palette_backdrop"),
            egui::Sense::click(),
        );
        if backdrop_response.clicked() {
            self.ui_store.view.action_palette_open = false;
        }

        // Close on Escape
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.ui_store.view.action_palette_open = false;
        }

        // Centered palette
        let palette_w = 360.0;
        let palette_h = 400.0;
        let palette_pos = Pos2::new(
            (screen_rect.width() - palette_w) / 2.0 + screen_rect.min.x,
            (screen_rect.height() - palette_h) / 2.0 + screen_rect.min.y,
        );
        let palette_rect = Rect::from_min_size(palette_pos, Vec2::new(palette_w, palette_h));

        // Background
        ui.painter().rect_filled(palette_rect, RADIUS_XL as u8, BG_BASE);
        ui.painter().rect_stroke(
            palette_rect,
            RADIUS_XL as u8,
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Outside,
        );

        // Content
        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(palette_rect));
        content.set_clip_rect(palette_rect);
        content.add_space(SPACE_L);

        // Title
        content.horizontal(|ui| {
            ui.label(
                RichText::new("Actions")
                    .size(FONT_SIZE_XL)
                    .color(TEXT_PRIMARY)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(egui_phosphor::regular::X).clicked() {
                    self.ui_store.view.action_palette_open = false;
                }
            });
        });
        content.add_space(SPACE_M);

        // Selected actor info
        let selected = self.ui_store.selection.selected_actors.iter().next().cloned();
        if let Some(ref actor) = selected {
            content.label(
                RichText::new(format!("Target: {}", actor))
                    .size(FONT_SIZE_S)
                    .color(TEXT_SECONDARY),
            );
        } else {
            content.label(
                RichText::new("No actor selected")
                    .size(FONT_SIZE_S)
                    .color(TEXT_MUTED),
            );
        }
        content.add_space(SPACE_M);

        // Action categories
        for (category, color, actions) in ACTION_CATEGORIES {
            content.label(
                RichText::new(*category)
                    .size(FONT_SIZE_S)
                    .color(*color)
                    .strong(),
            );
            content.add_space(SPACE_XS);

            content.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, SPACE_S);
                for (verb, label) in *actions {
                    let btn_size = Vec2::new(80.0, 32.0);
                    let btn_rect =
                        Rect::from_min_size(ui.cursor().min, btn_size);
                    let btn_id = ui.id().with(format!("action_{}", verb));
                    let btn_resp = ui.interact(btn_rect, btn_id, egui::Sense::click());

                    let btn_bg = if btn_resp.hovered() {
                        color.linear_multiply(0.2)
                    } else {
                        BG_WIDGET
                    };
                    ui.painter().rect_filled(btn_rect, RADIUS_M as u8, btn_bg);
                    ui.painter().rect_stroke(
                        btn_rect,
                        RADIUS_M as u8,
                        Stroke::new(1.0, *color),
                        egui::StrokeKind::Outside,
                    );
                    ui.painter().text(
                        btn_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *label,
                        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                        if btn_resp.hovered() {
                            TEXT_PRIMARY
                        } else {
                            TEXT_SECONDARY
                        },
                    );

                    if btn_resp.clicked() {
                        if let Some(ref actor) = selected {
                            self.apply_action(verb, actor);
                            self.ui_store.view.action_palette_open = false;
                        }
                    }

                    ui.allocate_rect(btn_rect, egui::Sense::hover());
                }
            });
            content.add_space(SPACE_M);
        }
    }

    /// Apply an action to the selected actor by inserting source text.
    fn apply_action(&mut self, verb: &str, actor: &str) {
        let Some(ref mut stmts) = self.document_store.document.raw_statements else {
            self.preview_store.preview.status = "No AST available".to_string();
            return;
        };

        let time_s = self.preview_store.preview.playback.current_time_s;
        let time_ms = (time_s * 1000.0) as u64;

        // Find the keyframe block that contains this time
        let target_line = self
            .document_store
            .document
            .timeline_index
            .line_for_time(time_ms);

        // Build action statement
        let action_text = format!("{} {} [1s]\n", verb, actor);

        if let Some(line) = target_line {
            // Insert after the keyframe line
            if line < stmts.len() {
                // This is a simplified insertion — in practice we'd use SourceEdit
                // For now, append to source text as a pragmatic workaround
                let current = self.document_store.document.source_text.clone();
                let lines: Vec<&str> = current.lines().collect();
                let insert_line = line.min(lines.len().saturating_sub(1));
                let mut new_lines = lines.clone();
                new_lines.insert(insert_line + 1, action_text.trim());
                let new_source = new_lines.join("\n");
                self.document_store.document.source_text = new_source.clone();
                self.document_store.editor.replace_text(new_source);
                self.document_store.document.is_dirty = true;
                self.preview_store.pending_rebuild_at = Some(
                    std::time::Instant::now()
                        + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
                );
                self.preview_store.preview.status = format!(
                    "Added action: {} {} @ {:.2}s",
                    verb, actor, time_s
                );
            }
        } else {
            // No keyframe found — append at end
            let mut new_source = self.document_store.document.source_text.clone();
            if !new_source.ends_with('\n') {
                new_source.push('\n');
            }
            new_source.push_str(&action_text);
            self.document_store.document.source_text = new_source.clone();
            self.document_store.editor.replace_text(new_source);
            self.document_store.document.is_dirty = true;
            self.preview_store.pending_rebuild_at = Some(
                std::time::Instant::now()
                    + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
            );
            self.preview_store.preview.status = format!(
                "Added action: {} {} @ {:.2}s",
                verb, actor, time_s
            );
        }
    }
}
