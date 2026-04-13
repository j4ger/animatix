use egui::TextBuffer;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use std::path::Path;

pub struct EditorBuffer {
    text: String,
    syntax: Syntax,
    widget: CodeEditor,
}

impl EditorBuffer {
    pub fn new(path: &Path, text: String) -> Self {
        Self {
            text,
            syntax: syntax_for_path(path),
            widget: CodeEditor::default()
                .id_source("animatix-editor")
                .with_rows(24)
                .with_fontsize(14.0)
                .with_theme(ColorTheme::SONOKAI)
                .with_numlines(true),
        }
    }

    pub fn set_document(&mut self, path: &Path, text: String) {
        self.text = text;
        self.syntax = syntax_for_path(path);
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn replace_text(&mut self, text: String) {
        self.text = text;
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let output = self
            .widget
            .clone()
            .with_syntax(self.syntax.clone())
            .show(ui, &mut self.text as &mut dyn TextBuffer);
        output.response
    }
}

pub fn syntax_for_path(path: &Path) -> Syntax {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("rs") => Syntax::rust(),
        Some("py") => Syntax::python(),
        Some("sh") | Some("bash") => Syntax::shell(),
        Some("sql") => Syntax::sql(),
        Some("lua") => Syntax::lua(),
        Some("asm") => Syntax::asm(),
        Some("amx") => Syntax::new("animatix")
            .with_comment("//")
            .with_keywords([
                "animate",
                "at",
                "cartesian",
                "from",
                "group",
                "image",
                "in",
                "let",
                "line",
                "math",
                "over",
                "path",
                "polar",
                "pub",
                "rect",
                "svg",
                "text",
                "to",
            ])
            .with_types([
                "Actor", "Arc", "Circle", "Ellipse", "Image", "Line", "Path", "Rect", "Text",
            ]),
        _ => Syntax::new("plain").with_comment("//"),
    }
}

#[cfg(test)]
mod tests {
    use super::syntax_for_path;
    use std::path::Path;

    #[test]
    fn animatix_files_use_animatix_syntax() {
        let syntax = syntax_for_path(Path::new("scene.amx"));
        assert_eq!(syntax.language, "animatix");
    }

    #[test]
    fn rust_files_use_rust_syntax() {
        let syntax = syntax_for_path(Path::new("main.rs"));
        assert_eq!(syntax.language, "Rust");
    }
}
