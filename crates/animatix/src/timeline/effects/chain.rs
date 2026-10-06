//! Renderer-facing effect-chain types and the compositing backend boundary.
//!
//! [`EffectChainTrack`](super::EffectChainTrack) is the *stored*, animatable
//! form; [`EffectChain`] is the fully-sampled per-frame form the renderer
//! consumes. The [`FilterBackend`] trait is the timeline↔renderer seam: the
//! timeline renders a scope's content children into a sub-scene and the backend
//! applies the chain to it.

use super::{EffectId, EffectParams};
use crate::timeline::SceneDimensions;
use crate::timeline::image::SceneImage;

/// A GPU texture that should be composited after the main Vello scene render.
/// Used by the zero-readback filter compositing path.
#[derive(Clone)]
pub struct PendingComposite {
    /// Owns the copied filtered texture so `view` remains valid.
    pub texture: wgpu::Texture,
    /// Texture view sampled by the fullscreen compositor.
    pub view: wgpu::TextureView,
    /// Opacity to apply during compositing.
    pub alpha: f32,
    /// Destination top-left corner in render-target pixels. The composite
    /// covers `origin` + the texture's own size (full render target when the
    /// scope did not use a region of interest).
    pub origin: [f32; 2],
    /// Corner radius, in render-target pixels, to clip the composite to.
    /// `0.0` composites the whole rectangle, which is what an effect scope
    /// with a derived region wants; a `Glass` panel passes its own
    /// `corner_radius` so the blurred backdrop keeps the panel's shape.
    pub corner_radius: f32,
    /// Sub-rectangle of the texture to sample, in texture pixels
    /// `[x0, y0, x1, y1]`. A backdrop's texture covers a *padded* region (the
    /// blur needs pixels from outside the panel), while only the panel's own
    /// rect is composited — so the sample window and the destination differ.
    /// `None` samples the whole texture, which is what an effect scope wants.
    pub src_rect: Option<[f32; 4]>,
    /// Destination rect to clip to, in render-target pixels
    /// `[x0, y0, x1, y1]`. `None` composites at `origin` with the texture's
    /// own size.
    pub clip_rect: Option<[f32; 4]>,
}

/// One layer the renderer must composite after the main scene render.
///
/// The queue is ordered, and the order *is* the z-order: splitting a frame at a
/// `Glass` scope into "below", "backdrop", "above" only reproduces the scene's
/// depth if the pieces are drained in the order the walk produced them.
#[derive(Clone)]
pub enum PendingLayer {
    /// A texture holding an effect chain's output over a sub-scene.
    Composite(PendingComposite),
    /// Blur what the render target already holds inside `region`, then composite
    /// the result back there. The backdrop half of a `Glass` scope: its input
    /// is the main target, which no effect chain can bind as a texture (the
    /// chain's two inputs are its own sub-scene), so it can only be served at
    /// drain time, once that target has been rendered.
    Backdrop(PendingBackdrop),
}

/// A deferred backdrop-blur pass.
#[derive(Clone)]
pub struct PendingBackdrop {
    /// The area to read from and blur, in render-target pixels. Padded by the
    /// chain's support so the blur samples real pixels at the panel's edge.
    pub region: EffectRegion,
    /// The panel's own rect, in render-target pixels — what actually gets
    /// composited back, clipped to `corner_radius`.
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

/// A region of interest for an effect scope, in scene pixels.
///
/// When present, the backend crops the rendered sub-scene to `origin`/`size`,
/// runs the chain at `size`, and the caller composites the result back at
/// `origin`. `None` means the full `dimensions` (the historical behaviour).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectRegion {
    /// Top-left corner of the region in scene pixels.
    pub origin: [f32; 2],
    /// Width and height of the region in scene pixels.
    pub size: SceneDimensions,
}

/// Backend that can render a [`vello::Scene`] and apply an [`EffectChain`].
///
/// The timeline captures a `Filter` scope's content children into an offscreen
/// scene, hands the chain to the backend, and composites the result back into
/// the parent scene. There is no CPU fallback: a backend that cannot run the
/// chain reports an error and the timeline renders the children unfiltered with
/// a diagnostic (`docs/effects.md` §5).
pub trait FilterBackend: Send {
    /// Declare that the frame is about to be rasterized at `scale` of the
    /// scene's own resolution, so the chain can follow it.
    ///
    /// A runtime that always renders at scene resolution never calls this, and a
    /// backend that cannot resize leaves it unimplemented: the default is a
    /// no-op, which is the same as `1.0`. A backend that honours it has to
    /// resize its targets *and* shrink every [`EffectParamUnit::Pixel`]
    /// parameter by the same factor — a blur radius is a distance in scene
    /// pixels, so halving the raster without halving the radius doubles the
    /// blur.
    ///
    /// [`EffectParamUnit::Pixel`]: animatix_core::effect::EffectParamUnit::Pixel
    fn set_raster_scale(&mut self, _scale: f32) {}

    /// Render `scene` (covering `dimensions`), apply `chain`, and read the
    /// result back as a [`SceneImage`].
    ///
    /// When `region` is `Some`, the chain runs only inside that region and the
    /// returned image covers the region (the caller composites it at
    /// `region.origin`); otherwise the image covers `dimensions`.
    fn render_scene_to_image_gpu_filtered(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<SceneImage, String>;

    /// Render a scene with `chain` and store the result as a pending composite
    /// that can be blitted onto the render target without CPU readback. When
    /// `region` is `Some`, the pending composite covers only that region.
    /// Returns `Err` if this backend doesn't support zero-readback compositing.
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

    /// Queue a backdrop pass: blur whatever the render target holds inside
    /// `region` when the frame is drained. `chain` runs over those pixels, so
    /// its inputs are the target's contents rather than a sub-scene.
    /// Returns `Err` if this backend cannot serve a backdrop.
    fn enqueue_backdrop(&mut self, backdrop: &PendingBackdrop) -> Result<(), String> {
        let _ = backdrop;
        Err("backdrop compositing is not supported by this backend".to_string())
    }

    /// Render `scene` into its own texture with no effect chain and queue it as
    /// a plain blit at `origin`. This is the "above" half of a split frame: the
    /// pinned vello build always clears its target (`RenderParams` has no
    /// preserve option), so content that must land *over* a drained layer cannot
    /// be rendered into the target in place.
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
    /// has just been rendered into, and return the layer to composite. Called
    /// from the drain loop in queue order, never during evaluation.
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
}
