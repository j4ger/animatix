//! Code editor with tree-sitter syntax highlighting, line numbers, and auto-complete.

use egui::{TextEdit, TextStyle, text::LayoutJob, Key, PointerButton};
use std::path::{Path, PathBuf};
use animatix_analyzer::Analyzer;
use crate::completion_popup::CompletionPopup;

mod highlight {
    pub use crate::highlighting::highlight_source;
}

pub struct EditorBuffer {
    text: String,
    document_path: PathBuf,
    /// Cached highlight result to avoid re-parsing every frame.
    cached_highlight: Option<(String, LayoutJob)>,
    /// Language analyzer for completions and diagnostics.
    analyzer: Analyzer,
    /// Completion popup state.
    completion: CompletionPopup,
    /// Whether completion was just confirmed (to avoid re-triggering).
    completion_confirmed: bool,
}

impl EditorBuffer {
    pub fn new(path: &Path, text: String) -> Self {
        let analyzer = Analyzer::new(&text);
        Self {
            text,
            document_path: path.to_path_buf(),
            cached_highlight: None,
            analyzer,
            completion: CompletionPopup::new(),
            completion_confirmed: false,
        }
    }

    pub fn set_document(&mut self, path: &Path, text: String) {
        self.text = text;
        self.document_path = path.to_path_buf();
        self.cached_highlight = None;
        self.analyzer.update(&self.text);
        self.completion.hide();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn replace_text(&mut self, text: String) {
        self.text = text;
        self.cached_highlight = None;
        self.analyzer.update(&self.text);
    }

    /// Get the analyzer for diagnostics, hover, etc.
    pub fn analyzer(&self) -> &Analyzer {
        &self.analyzer
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let style = ui.style().clone();

        // Invalidate cache if text changed
        if let Some((ref cached_text, _)) = self.cached_highlight {
            if *cached_text != self.text {
                self.cached_highlight = None;
            }
        }

        // Build or reuse cached highlight
        if self.cached_highlight.is_none() {
            let diagnostics = self.analyzer.diagnostics();
            let job = highlight::highlight_source(&self.text, &style, &diagnostics);
            self.cached_highlight = Some((self.text.clone(), job));
        }

        let cached_job = self.cached_highlight.as_ref().unwrap().1.clone();

        let mut layouter = move |ui: &egui::Ui, _buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = cached_job.clone();
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };

        let response = ui.add(
            TextEdit::multiline(&mut self.text)
                .id_salt((&self.document_path, "animatix-editor"))
                .font(TextStyle::Monospace)
                .code_editor()
                .desired_rows(30)
                .desired_width(f32::INFINITY)
                .layouter(&mut layouter),
        );

        // Hover tooltip handling
        if response.hovered() {
            if let Some(pos) = ui.ctx().pointer_interact_pos() {
                let line_height = 14.0 * 1.2; // 16.8
                let char_width = 8.0; // approximate monospace char width

                // Convert pixel position to line/col (relative to editor rect)
                let rel_y = pos.y - response.rect.min.y;
                let rel_x = pos.x - response.rect.min.x;

                let line = (rel_y / line_height).floor() as usize;
                let col = (rel_x / char_width).floor() as usize;

                if let Some(hover_info) = self.analyzer.hover_at(line, col) {
                    response.show_tooltip_text(hover_info.contents);
                }
            }
        }

        // Ctrl+Click go-to-definition
        if response.hovered() {
            ui.input(|i| {
                if i.pointer.button_clicked(PointerButton::Primary) && i.modifiers.ctrl {
                    if let Some(pos) = i.pointer.interact_pos() {
                        let line_height = 14.0 * 1.2; // 16.8
                        let char_width = 8.0; // approximate monospace char width

                        // Convert pixel position to line/col (relative to editor rect)
                        let rel_y = pos.y - response.rect.min.y;
                        let rel_x = pos.x - response.rect.min.x;

                        let line = (rel_y / line_height).floor() as usize;
                        let col = (rel_x / char_width).floor() as usize;

                        if let Some(location) = self.analyzer.definition_at(line, col) {
                            // For now, we'll scroll to the line by updating the text view offset
                            // The actual scrolling would be handled by the caller or via a state mechanism
                            let _target_line = location.line;
                            // Trigger a scroll to target_line (the UI will need to handle this)
                            ui.scroll_to_cursor(Some(egui::Align::Center));
                        }
                    }
                }
            });
        }

        // Handle completion keyboard input
        let completion_consumed = self.completion.handle_input(ui.ctx());

        // If completion consumed the input, don't process further
        if completion_consumed {
            // Check if Tab/Enter was pressed to confirm completion
            let insert_text = self.completion.selected_item().map(|item| {
                item.insert_text.as_deref().unwrap_or(&item.label).to_string()
            });
            if let Some(text) = insert_text {
                self.insert_completion(&text);
                self.completion.hide();
                self.completion_confirmed = true;
            }
        }

        // Trigger completion on Ctrl+Space
        if response.has_focus() {
            ui.input(|i| {
                if i.key_pressed(Key::Space) && i.modifiers.ctrl {
                    self.trigger_completion();
                }
            });
        }

        // Trigger completion on typing (after a short delay)
        if response.changed() && !self.completion_confirmed {
            self.analyzer.update(&self.text);
            self.cached_highlight = None;

            // Auto-trigger completion on certain characters
            if let Some(last_char) = self.text.chars().last() {
                if last_char == ':' || last_char == '.' || last_char == ' ' {
                    self.trigger_completion();
                }
            }
        }

        // Reset completion_confirmed flag
        if self.completion_confirmed {
            self.completion_confirmed = false;
        }

        // Show completion popup if visible
        if self.completion.is_visible() {
            // Get cursor position (approximate - we'll use the response rect)
            let cursor_rect = response.rect; // TODO: get actual cursor position
            if let Some(insert_text) = self.completion.ui(ui, cursor_rect) {
                self.insert_completion(&insert_text);
                self.completion.hide();
            }
        }

        response
    }

    /// Trigger completion at current cursor position.
    fn trigger_completion(&mut self) {
        // Get cursor position from the text
        // For now, we'll use a simple heuristic: find the last word being typed
        let cursor_pos = self.text.len();
        let (line, col) = self.byte_to_line_col(cursor_pos);

        let items = self.analyzer.completions_at(line, col);
        let trigger_text = self.get_current_word();
        self.completion.show(items, trigger_text);
    }

    /// Insert completion text at cursor position.
    fn insert_completion(&mut self, insert_text: &str) {
        // For now, append to the end of the text
        // TODO: insert at actual cursor position
        self.text.push_str(insert_text);
        self.cached_highlight = None;
        self.analyzer.update(&self.text);
    }

    /// Get the current word being typed (for completion filtering).
    fn get_current_word(&self) -> String {
        // Find the last word before cursor
        let text = &self.text;
        let mut word = String::new();

        for ch in text.chars().rev() {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                word.insert(0, ch);
            } else {
                break;
            }
        }

        word
    }

    /// Convert byte offset to (line, col).
    fn byte_to_line_col(&self, byte_offset: usize) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;

        for (i, ch) in self.text.char_indices() {
            if i >= byte_offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }

        (line, col)
    }
}
