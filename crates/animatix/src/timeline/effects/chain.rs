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

    /// Drain any pending composites produced by `render_scene_to_pending_composite`.
    fn take_pending_composites(&mut self) -> Vec<PendingComposite> {
        Vec::new()
    }
}
