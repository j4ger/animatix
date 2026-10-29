//! Renderer-facing effect-chain types and the compositing backend boundary.

use super::{EffectId, EffectParams};
use crate::timeline::SceneDimensions;
use crate::timeline::image::SceneImage;

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

/// A deferred backdrop-blur pass.
#[derive(Clone)]
pub struct PendingBackdrop {
    /// The area to read from and blur, in render-target pixels. Padded by the
    /// chain's support so the blur samples real pixels at the panel's edge.
    pub region: EffectRegion,
    /// The panel's own rect, in render-target pixels — what is composited back,
    /// clipped to `corner_radius`.
    pub clip: EffectRegion,
    /// The chain to run over the copied pixels — a `Blur` in practice.
    pub chain: EffectChain,
    /// Corner radius to clip the composited result to.
    pub corner_radius: f32,
    /// Opacity to apply during compositing.
    pub alpha: f32,
}

/// One resolved effect in a chain.
#[derive(Clone, Debug)]
pub struct EffectInstance {
    /// Which effect to run.
    pub id: EffectId,
    /// Whether the instance is enabled this frame.
    pub enabled: bool,
    /// Resolved parameters.
    pub params: EffectParams,
}

/// An ordered list of effects to apply to a compositing scope.
#[derive(Clone, Debug, Default)]
pub struct EffectChain {
    /// Active instances, in application order.
    pub instances: Vec<EffectInstance>,
    /// Timeline time in milliseconds, supplied to shaders as `time_ms`.
    pub time_ms: f32,
}

impl EffectChain {
    /// `true` when no effect will run (empty or every instance disabled).
    pub fn is_empty(&self) -> bool {
        self.instances.iter().all(|instance| !instance.enabled)
    }
}

/// A region of interest for an effect scope, in scene pixels; `None` on the
/// consuming APIs means the full `dimensions`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectRegion {
    /// Top-left corner of the region in scene pixels.
    pub origin: [f32; 2],
    /// Width and height of the region in scene pixels.
    pub size: SceneDimensions,
}

/// Backend that can render a [`vello::Scene`] and apply an [`EffectChain`].
/// There is no CPU fallback: an unsupported chain errors and the timeline renders
/// the children unfiltered with a diagnostic (`docs/effects.md` §5).
pub trait FilterBackend: Send {
    /// Declare that the frame is about to be rasterized at `scale` of the
    /// scene's own resolution, so the chain can follow it.
    /// Defaults to a no-op (`1.0`); honouring it means resizing targets *and*
    /// shrinking every [`EffectParamUnit::Pixel`] by the same factor, since a
    /// blur radius is a scene-pixel distance (halving the raster doubles it).
    /// [`EffectParamUnit::Pixel`]: animatix_core::effect::EffectParamUnit::Pixel
    fn set_raster_scale(&mut self, _scale: f32) {}

    /// Render `scene` (covering `dimensions`), apply `chain`, and read the result
    /// back as a [`SceneImage`]; with `region` both input and output cover only
    /// that region (the caller composites it at `region.origin`).
    fn render_scene_to_image_gpu_filtered(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<SceneImage, String>;

    /// Render a scene with `chain` and store the result as a pending composite
    /// that blits without CPU readback; `region` restricts its extent.
    fn render_scene_to_pending_composite(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
        alpha: f32,
    ) -> Result<(), String> {
        let _ = (scene, dimensions, region, chain, alpha);
        Err("zero-readback effect compositing is not supported by this backend".to_string())
    }

    /// Queue a backdrop pass over whatever the render target holds inside
    /// `region` when the frame is drained.
    fn enqueue_backdrop(&mut self, backdrop: &PendingBackdrop) -> Result<(), String> {
        let _ = backdrop;
        Err("backdrop compositing is not supported by this backend".to_string())
    }

    /// Render `scene` into its own texture with no effect chain and queue it as
    /// a plain blit at `origin`. The "above" half of a split frame: pinned vello
    /// always clears its target, so over-drawn content cannot render in place.
    fn enqueue_scene_composite(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        origin: [f32; 2],
        alpha: f32,
    ) -> Result<(), String> {
        let _ = (scene, dimensions, origin, alpha);
        Err("scene compositing is not supported by this backend".to_string())
    }

    /// Serve one queued backdrop against `target`, the texture the main scene
    /// has just been rendered into, and return the layer to composite.
    fn run_backdrop(
        &mut self,
        target: &wgpu::Texture,
        backdrop: &PendingBackdrop,
    ) -> Result<PendingComposite, String> {
        let _ = (target, backdrop);
        Err("backdrop compositing is not supported by this backend".to_string())
    }

    /// Drain any pending layers produced by the scope paths above.
    fn take_pending_composites(&mut self) -> Vec<PendingLayer> {
        Vec::new()
    }

    /// Recycle a pending composite texture that has finished blitting.
    /// Texture-pool backends reuse the allocation; the default drops it.
    fn recycle_pending_composite(&mut self, _composite: PendingComposite) {}
}
