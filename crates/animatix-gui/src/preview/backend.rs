use crate::preview::artifact::PreviewArtifact;
use animatix::ast::Stmt;
use animatix::renderer::render_image;
use animatix::timeline::SceneDimensions;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub trait PreviewBackend {
    fn render(
        &mut self,
        ast: &[Stmt],
        time_s: f64,
        dimensions: SceneDimensions,
    ) -> Result<PreviewArtifact, String>;

    fn backend_name(&self) -> &'static str;
}

pub struct SnapshotBackend {
    revision: u64,
}

impl SnapshotBackend {
    pub fn new() -> Self {
        Self { revision: 0 }
    }
}

impl PreviewBackend for SnapshotBackend {
    fn render(
        &mut self,
        ast: &[Stmt],
        time_s: f64,
        dimensions: SceneDimensions,
    ) -> Result<PreviewArtifact, String> {
        self.revision += 1;
        let preview_path = preview_output_path(self.revision);
        render_image(
            ast,
            dimensions.width,
            dimensions.height,
            time_s as f32,
            &preview_path,
        );
        Ok(PreviewArtifact::Snapshot(preview_path))
    }

    fn backend_name(&self) -> &'static str {
        "Snapshot"
    }
}

fn preview_output_path(revision: u64) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    std::env::temp_dir().join(format!("animatix_gui_preview_{stamp}_{revision}.png"))
}
