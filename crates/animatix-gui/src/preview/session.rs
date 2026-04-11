use crate::preview::artifact::PreviewArtifact;
use crate::preview::backend::PreviewBackend;
use animatix::ast::Stmt;
use animatix::timeline::SceneDimensions;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PreviewState {
    pub artifact: Option<PreviewArtifact>,
    pub status: String,
    pub error: Option<String>,
}

pub struct PreviewSession {
    backend: Box<dyn PreviewBackend>,
    pub state: PreviewState,
    pub current_time_s: f64,
    pub duration_s: f64,
    pub is_playing: bool,
    pub dimensions: SceneDimensions,
}

impl PreviewSession {
    pub fn new(backend: Box<dyn PreviewBackend>, dimensions: SceneDimensions) -> Self {
        Self {
            backend,
            state: PreviewState {
                artifact: None,
                status: "Loaded file".to_string(),
                error: None,
            },
            current_time_s: 0.0,
            duration_s: 5.0,
            is_playing: false,
            dimensions,
        }
    }

    pub fn from_error(
        backend: Box<dyn PreviewBackend>,
        dimensions: SceneDimensions,
        error: String,
    ) -> Self {
        Self {
            backend,
            state: PreviewState {
                artifact: None,
                status: "Failed to initialize session".to_string(),
                error: Some(error),
            },
            current_time_s: 0.0,
            duration_s: 5.0,
            is_playing: false,
            dimensions,
        }
    }

    pub fn set_duration(&mut self, duration_s: f64) {
        self.duration_s = duration_s.max(0.1);
        if self.current_time_s > self.duration_s {
            self.current_time_s = self.duration_s;
        }
    }

    pub fn set_current_time(&mut self, next_time_s: f64) {
        self.current_time_s = next_time_s.clamp(0.0, self.duration_s.max(0.0));
    }

    pub fn tick_playback(&mut self, delta: Duration) {
        if !self.is_playing {
            return;
        }

        let next_time = self.current_time_s + delta.as_secs_f64();
        if next_time >= self.duration_s {
            self.current_time_s = self.duration_s;
            self.is_playing = false;
        } else {
            self.current_time_s = next_time;
        }
    }

    pub fn toggle_playback(&mut self) {
        if self.current_time_s >= self.duration_s {
            self.current_time_s = 0.0;
        }
        self.is_playing = !self.is_playing;
    }

    pub fn render(&mut self, ast: &[Stmt]) -> Result<(), String> {
        let artifact = self
            .backend
            .render(ast, self.current_time_s, self.dimensions)
            .map_err(|err| err.to_string())?;

        self.state.status = format!(
            "{} preview • t = {:.2}s / {:.2}s",
            self.backend.backend_name(),
            self.current_time_s,
            self.duration_s
        );
        self.state.artifact = Some(artifact);
        self.state.error = None;
        Ok(())
    }
}
