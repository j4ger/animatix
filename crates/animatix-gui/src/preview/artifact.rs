use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum PreviewArtifact {
    Snapshot(PathBuf),
    FutureSurface,
}

impl PreviewArtifact {
    pub fn snapshot_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Snapshot(path) => Some(path),
            Self::FutureSurface => None,
        }
    }
}
