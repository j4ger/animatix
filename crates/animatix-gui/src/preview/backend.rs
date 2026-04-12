use crate::preview::artifact::PreviewArtifact;
use animatix::renderer::{OffscreenRenderer, RenderedFrame};
use animatix::timeline::{SceneDimensions, Timeline};
use gpui::RenderImage;
use image::{Frame, RgbaImage};
use std::sync::Arc;

pub trait PreviewBackend {
    fn render(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
    ) -> Result<PreviewArtifact, String>;

    fn backend_name(&self) -> &'static str;
}

pub struct OffscreenPreviewBackend {
    renderer: Option<OffscreenRenderer>,
}

impl OffscreenPreviewBackend {
    pub fn new() -> Self {
        Self { renderer: None }
    }
}

impl PreviewBackend for OffscreenPreviewBackend {
    fn render(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
    ) -> Result<PreviewArtifact, String> {
        let renderer = self.renderer.get_or_insert(OffscreenRenderer::new()?);
        let frame = renderer.render_timeline(timeline, time_s, dimensions)?;
        Ok(PreviewArtifact::Image(render_image_from_frame(frame)?))
    }

    fn backend_name(&self) -> &'static str {
        "Live"
    }
}

fn render_image_from_frame(frame: RenderedFrame) -> Result<Arc<RenderImage>, String> {
    let mut buffer = RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
        .ok_or_else(|| "Failed to create preview image buffer".to_string())?;
    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new(vec![Frame::new(buffer)])))
}
