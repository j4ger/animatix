use gpui::RenderImage;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum PreviewArtifact {
    Image(Arc<RenderImage>),
    FutureSurface,
}

impl PreviewArtifact {
    pub fn render_image(&self) -> Option<&Arc<RenderImage>> {
        match self {
            Self::Image(image) => Some(image),
            Self::FutureSurface => None,
        }
    }
}
