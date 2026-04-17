use egui::{Color32, FontId, TextBuffer, TextEdit, TextFormat, TextStyle, text::LayoutJob};
use egui_extras::syntax_highlighting::{CodeTheme, SyntectSettings, highlight_with};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxDefinition, SyntaxSet};

const ANIMATIX_SUBLIME_SYNTAX: &str = include_str!("animatix.sublime-syntax");

static EDITOR_SYNTECT_SETTINGS: LazyLock<SyntectSettings> = LazyLock::new(build_syntect_settings);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorLanguage {
    Rust,
    Python,
    Shell,
    Sql,
    Lua,
    Asm,
    Animatix,
    Plain,
}

impl EditorLanguage {
    fn syntect_language(self) -> Option<&'static str> {
        match self {
            Self::Rust => Some("rs"),
            Self::Python => Some("py"),
            Self::Shell => Some("sh"),
            Self::Sql => Some("sql"),
            Self::Lua => Some("lua"),
            Self::Asm => Some("asm"),
            Self::Animatix => Some("amx"),
            Self::Plain => None,
        }
    }
}

pub struct EditorBuffer {
    text: String,
    language: EditorLanguage,
    document_path: PathBuf,
    theme: CodeTheme,
}

impl EditorBuffer {
    pub fn new(path: &Path, text: String) -> Self {
        Self {
            text,
            language: language_for_path(path),
            document_path: path.to_path_buf(),
            theme: CodeTheme::from_style(&egui::Style::default()),
        }
    }

    pub fn set_document(&mut self, path: &Path, text: String) {
        self.text = text;
        self.language = language_for_path(path);
        self.document_path = path.to_path_buf();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn replace_text(&mut self, text: String) {
        self.text = text;
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        self.theme = CodeTheme::from_memory(ui.ctx(), ui.style());

        let theme = self.theme.clone();
        let language = self.language;
        let mut layouter = move |ui: &egui::Ui, buf: &dyn TextBuffer, wrap_width: f32| {
            let mut layout_job = match language.syntect_language() {
                Some(syntect_language) => highlight_with(
                    ui.ctx(),
                    ui.style(),
                    &theme,
                    buf.as_str(),
                    syntect_language,
                    &EDITOR_SYNTECT_SETTINGS,
                ),
                None => plain_text_layout_job(ui, buf.as_str()),
            };
            layout_job.wrap.max_width = wrap_width;
            ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
        };

        ui.add(
            TextEdit::multiline(&mut self.text)
                .id_salt((&self.document_path, "animatix-editor"))
                .font(TextStyle::Monospace)
                .code_editor()
                .desired_rows(24)
                .desired_width(f32::INFINITY)
                .layouter(&mut layouter),
        )
    }
}

fn plain_text_layout_job(ui: &egui::Ui, text: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: FontId::monospace(14.0),
            color: ui.visuals().text_color(),
            background: Color32::TRANSPARENT,
            ..Default::default()
        },
    );
    job
}

fn language_for_path(path: &Path) -> EditorLanguage {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("rs") => EditorLanguage::Rust,
        Some("py") => EditorLanguage::Python,
        Some("sh") | Some("bash") => EditorLanguage::Shell,
        Some("sql") => EditorLanguage::Sql,
        Some("lua") => EditorLanguage::Lua,
        Some("asm") => EditorLanguage::Asm,
        Some("amx") => EditorLanguage::Animatix,
        _ => EditorLanguage::Plain,
    }
}

fn build_syntect_settings() -> SyntectSettings {
    let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
    let animatix = SyntaxDefinition::load_from_str(ANIMATIX_SUBLIME_SYNTAX, true, Some("Animatix"))
        .expect("animatix sublime syntax should parse");
    builder.add(animatix);

    SyntectSettings {
        ps: builder.build(),
        ts: ThemeSet::load_defaults(),
    }
}

#[cfg(test)]
mod tests {
    use super::{EDITOR_SYNTECT_SETTINGS, EditorLanguage, language_for_path};
    use std::path::Path;

    #[test]
    fn animatix_files_use_animatix_language() {
        assert_eq!(
            language_for_path(Path::new("scene.amx")),
            EditorLanguage::Animatix
        );
    }

    #[test]
    fn rust_files_use_rust_language() {
        assert_eq!(
            language_for_path(Path::new("main.rs")),
            EditorLanguage::Rust
        );
    }

    #[test]
    fn syntect_settings_include_animatix_extension() {
        let syntax = EDITOR_SYNTECT_SETTINGS
            .ps
            .find_syntax_by_extension("amx")
            .expect("animatix syntax should be registered");

        assert_eq!(syntax.name, "Animatix");
    }
}
