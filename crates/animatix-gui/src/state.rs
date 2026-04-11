use crate::document::DocumentSession;
use crate::preview::backend::{PreviewBackend, SnapshotBackend};
use crate::preview::session::PreviewSession;
use animatix::timeline::SceneDimensions;
use std::path::PathBuf;
use std::time::Duration;

pub struct SessionState {
    pub document: DocumentSession,
    pub preview: PreviewSession,
}

impl SessionState {
    pub fn load(file_path: PathBuf) -> Result<Self, String> {
        let document = DocumentSession::load(file_path)?;
        let mut preview = Self::new_preview_session(Box::new(SnapshotBackend::new()));
        preview.set_duration(document.duration_s);
        if let Some(ast) = document.ast.as_ref() {
            preview.render(ast)?;
        }
        Ok(Self { document, preview })
    }

    pub fn from_error(file_path: PathBuf, error: String) -> Self {
        Self {
            document: DocumentSession::from_error(file_path),
            preview: PreviewSession::from_error(
                Box::new(SnapshotBackend::new()),
                Self::default_dimensions(),
                error,
            ),
        }
    }

    pub fn file_path(&self) -> &PathBuf {
        &self.document.file_path
    }

    pub fn source_text(&self) -> &str {
        &self.document.source_text
    }

    pub fn is_dirty(&self) -> bool {
        self.document.is_dirty
    }

    pub fn set_source_text(&mut self, source_text: String) {
        self.document.set_source_text(source_text);
    }

    pub fn reload_from_disk(&mut self) -> Result<(), String> {
        self.document.reload_from_disk()?;
        self.preview.set_duration(self.document.duration_s);
        self.render_preview()
    }

    pub fn save_to_disk(&mut self) -> Result<(), String> {
        self.document.save_to_disk()?;
        self.preview.state.status = format!("Saved {}", self.document.file_path.display());
        Ok(())
    }

    pub fn rebuild(&mut self) -> Result<(), String> {
        self.document.rebuild()?;
        self.preview.set_duration(self.document.duration_s);
        self.preview.state.status = format!(
            "Built timeline • {:.2}s total duration",
            self.document.duration_s
        );
        self.preview.state.error = None;
        self.render_preview()
    }

    pub fn set_current_time(&mut self, next_time_s: f64) -> Result<(), String> {
        self.preview.set_current_time(next_time_s);
        self.render_preview()
    }

    pub fn tick_playback(&mut self, delta: Duration) -> Result<(), String> {
        self.preview.tick_playback(delta);
        self.render_preview()
    }

    pub fn toggle_playback(&mut self) {
        self.preview.toggle_playback();
    }

    fn render_preview(&mut self) -> Result<(), String> {
        let ast = self
            .document
            .ast
            .as_ref()
            .ok_or_else(|| "No compiled scene available for preview".to_string())?;
        self.preview.render(ast)
    }

    fn new_preview_session(backend: Box<dyn PreviewBackend>) -> PreviewSession {
        PreviewSession::new(backend, Self::default_dimensions())
    }

    fn default_dimensions() -> SceneDimensions {
        SceneDimensions {
            width: 1280,
            height: 720,
        }
    }
}

pub use crate::document::default_file_path;
