//! Renderer-facing effect-chain types and the compositing backend boundary.

use super::{EffectId, EffectParams};
use crate::timeline::SceneDimensions;
use crate::timeline::image::SceneImage;

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
    /// Scene-relative overrides, indexed by active instance and uniform parameter.
    pub point_centers: Vec<(usize, usize, super::PointSample)>,
}

impl EffectChain {
    pub(crate) fn transform_points(&mut self, transform: kurbo::Affine) {
        for (_, _, point) in &mut self.point_centers {
            point.transform_scene(transform);
        }
    }

    pub(crate) fn normalized_region(
        &self,
        region: Option<EffectRegion>,
        canvas: SceneDimensions,
    ) -> std::borrow::Cow<'_, Self> {
        let region = region.unwrap_or(EffectRegion {
            origin: [0.0; 2],
            size: canvas,
        });
        self.normalized_points(
            kurbo::Affine::translate((-f64::from(region.origin[0]), -f64::from(region.origin[1]))),
            [region.size.width.max(1), region.size.height.max(1)],
        )
    }
    /// Convert named actor points to texture coordinates once per effect instance.
    pub fn normalized_points(
        &self,
        shifted: kurbo::Affine,
        size: [u32; 2],
    ) -> std::borrow::Cow<'_, Self> {
        if self.point_centers.is_empty() {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut chain = self.clone();
        for (stage, parameter, center) in chain.point_centers.drain(..) {
            chain.instances[stage].params.values[parameter] =
                super::EffectParamValue::Vec2(center.texture_uv(shifted, size));
        }
        std::borrow::Cow::Owned(chain)
    }
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

/// Backend that can render a [`crate::scene::Scene`] and apply an [`EffectChain`].
/// There is no CPU fallback: an unsupported chain errors and the timeline renders
/// the children unfiltered with a diagnostic (`docs/effects.md` §5).
pub trait FilterBackend: Send {
    /// Whether effect scopes should be recorded into the device-neutral Scene
    /// instead of executed while evaluating the timeline.
    ///
    /// New renderers use this to preserve Filter and Glass operations in painter
    /// order. The legacy Vello adapter leaves this disabled.
    fn records_scene_effects(&self) -> bool {
        false
    }

    /// Declare that the frame is about to be rasterized at `scale` of the
    /// scene's own resolution, so the chain can follow it.
    /// Defaults to a no-op (`1.0`); honouring it means resizing targets *and*
    /// shrinking every [`EffectParamUnit::Pixel`] by the same factor, since a
    /// blur radius is a scene-pixel distance (halving the raster doubles it).
    ///
    /// [`EffectParamUnit::Pixel`]: animatix_core::effect::EffectParamUnit::Pixel
    fn set_raster_scale(&mut self, _scale: f32) {}

    /// Render `scene` (covering `dimensions`), apply `chain`, and read the result
    /// back as a [`SceneImage`]; with `region` both input and output cover only
    /// that region (the caller composites it at `region.origin`).
    fn render_scene_to_image_gpu_filtered(
        &mut self,
        scene: &crate::scene::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<SceneImage, String>;

    /// Render a scene with `chain` and store the result as a pending composite
    /// that blits without CPU readback; `region` restricts its extent.
    fn render_scene_to_pending_composite(
        &mut self,
        scene: &crate::scene::Scene,
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
        scene: &crate::scene::Scene,
        dimensions: SceneDimensions,
        origin: [f32; 2],
        alpha: f32,
    ) -> Result<(), String> {
        let _ = (scene, dimensions, origin, alpha);
        Err("scene compositing is not supported by this backend".to_string())
    }
}
