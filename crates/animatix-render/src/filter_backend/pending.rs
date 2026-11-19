use animatix::timeline::effects::PendingBackdrop;

/// A GPU texture that should be composited after the main Vello scene render.
#[derive(Clone)]
pub struct PendingComposite {
    /// Owns the copied filtered texture so `view` remains valid.
    pub texture: wgpu::Texture,
    /// Texture view sampled by the fullscreen compositor.
    pub view: wgpu::TextureView,
    /// Opacity to apply during compositing.
    pub alpha: f32,
    /// Destination top-left corner in **scene** pixels — see [`Self::texel_scale`]
    /// for what the texture itself is measured in.
    /// The composite covers `origin` plus the texture's size scaled back into
    /// scene space (the full render target when the scope had no region).
    pub origin: [f32; 2],
    /// Texels of `texture` per scene pixel.
    /// `1.0` = scene resolution, so the frame's raster scale still applies; a
    /// backend told that scale via [`FilterBackend::set_raster_scale`] filtered
    /// at it and this carries the factor, so a pre-scaled backdrop rect is not
    /// shrunk twice by the drain.
    pub texel_scale: f32,
    /// Corner radius, in render-target pixels, to clip the composite to.
    /// `0.0` composites the whole rectangle; a `Glass` panel passes its own so
    /// the blurred backdrop keeps the panel's shape.
    pub corner_radius: f32,
    /// Sub-rectangle of the texture to sample, in texture pixels
    /// `[x0, y0, x1, y1]`. A backdrop textures a *padded* region while only the
    /// panel rect is composited; `None` samples the whole texture.
    pub src_rect: Option<[f32; 4]>,
    /// Destination rect to clip to, in render-target pixels `[x0, y0, x1, y1]`.
    /// `None` composites at `origin` with the texture's own size.
    pub clip_rect: Option<[f32; 4]>,
}

/// One layer the renderer must composite after the main scene render.
/// Queue order is z-order: split `Glass` pieces must drain in walk order.
#[derive(Clone)]
pub enum PendingLayer {
    /// A texture holding an effect chain's output over a sub-scene.
    Composite(PendingComposite),
    /// Blur what the render target already holds inside `region`, then composite
    /// the result back there. The main target cannot be bound as a chain
    /// texture, so this backdrop half is served at drain time.
    Backdrop(PendingBackdrop),
}
