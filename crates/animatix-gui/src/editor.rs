//! Code editor with tree-sitter syntax highlighting and line numbers.

use egui::{TextEdit, TextStyle, text::LayoutJob};
use std::path::{Path, PathBuf};

mod highlight {
    pub use crate::highlighting::highlight_source;
}

pub struct EditorBuffer {
    text: String,
    document_path: PathBuf,
    /// Cached highlight result to avoid re-parsing every frame.
    cached_highlight: Option<(String, LayoutJob)>,
}

impl EditorBuffer {
    pub fn new(path: &Path, text: String) -> Self {
        Self {
            text,
            document_path: path.to_path_buf(),
            cached_highlight: None,
        }
    }

    pub fn set_document(&mut self, path: &Path, text: String) {
        self.text = text;
        self.document_path = path.to_path_buf();
        self.cached_highlight = None;
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn replace_text(&mut self, text: String) {
        self.text = text;
        self.cached_highlight = None;
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
            let job = highlight::highlight_source(&self.text, &style);
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

        // Invalidate cache on change
        if response.changed() {
            self.cached_highlight = None;
        }

        response
    }
}
