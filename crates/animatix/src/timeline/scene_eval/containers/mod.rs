//! Child-pipeline strategies for the container primitives.
//!
//! `Filter`, `Glass`, `Mask`, `Equation` and `Captions` each own a specialized
//! sub-pipeline; the region and bounds helpers every effect scope shares stay
//! here in the parent module.

mod captions;
mod equation;
mod filter;
mod glass;
mod mask;

#[cfg(test)]
pub(crate) use equation::equation_markup_escaped;

use crate::timeline::{EntityId, SceneDimensions, Timeline};

impl Timeline {
    /// Compute the region of interest for an effect scope from an authored
    /// `bounds: (x, y, w, h)` (tagged `filter_bounds` storage). Returns `None`
    /// when no bounds are authored, the region is degenerate, or it already
    /// covers the whole scene (the historical full-scene path).
    ///
    /// The authored rect is in scene coordinates but the sub-scene is rendered
    /// with the camera applied and the result blits back in that same space, so
    /// the bounds have to be carried through the same affine — otherwise a camera
    /// move leaves the scope filtering the rectangle the author pointed at
    /// before the move. The derived-bounds branch at the call site needs no such
    /// mapping: the subtree bounds it reads were recorded by that same
    /// transformed evaluation.
    pub(crate) fn effect_scope_region(
        &self,
        id: EntityId,
        scene_dimensions: SceneDimensions,
        time_ms: u64,
        camera: kurbo::Affine,
    ) -> Option<crate::timeline::effects::EffectRegion> {
        let value = self.world.read_property_value(
            id,
            crate::timeline::property_registry::ActorField::Tagged("filter_bounds"),
            time_ms,
        );
        let Some(crate::timeline::PropertyValue::Vec4([x, y, w, h])) = value else {
            return None;
        };
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let rect = kurbo::Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64);
        // The rule lives here rather than at the caller so it can be tested
        // directly: authored bounds are scene coordinates, and the camera moves
        // the picture under them — unless this scope opted the camera out, in
        // which case its subtree was evaluated untransformed and its authored
        // numbers are already screen coordinates.
        let meta = self.world.meta(id);
        let camera = if meta.camera_follow && (meta.parallax - 1.0).abs() < f32::EPSILON {
            camera
        } else if !meta.camera_follow || meta.parallax == 0.0 {
            kurbo::Affine::IDENTITY
        } else {
            let p = meta.parallax as f64;
            let cam = camera.as_coeffs();
            kurbo::Affine::new([
                1.0 + p * (cam[0] - 1.0),
                p * cam[1],
                p * cam[2],
                1.0 + p * (cam[3] - 1.0),
                p * cam[4],
                p * cam[5],
            ])
        };
        Self::region_from_rect(
            camera.transform_rect_bbox(rect),
            self.world
                .effects(id)
                .map(|effects| effects.worst_case_support())
                .unwrap_or(0.0),
            scene_dimensions,
        )
    }

    /// Union the recorded world bounds of `label`'s content subtree into `out`.
    ///
    /// Bounds are recorded by the sub-scene evaluation that just ran, so they
    /// reflect exactly what this frame drew. Nodes without recorded ink (pure
    /// containers, disabled effects) contribute nothing themselves; their
    /// subtrees are still walked.
    fn subtree_bounds_union(&self, label: &str, out: &mut Option<kurbo::Rect>) {
        if let Some(rect) = self.precise_bounds_for(label) {
            *out = Some(match out.take() {
                Some(accumulated) => accumulated.union(rect),
                None => rect,
            });
        }
        if let Some(id) = self.world.get_id(label) {
            for child in self.world.child_labels(id) {
                self.subtree_bounds_union(child, out);
            }
        }
    }

    /// Turn a content-bounds rectangle into an [`EffectRegion`], padding by
    /// `support` and clamping to the scene. Returns `None` when the padded
    /// region covers the whole scene (fall back to the full-scene path) or is
    /// degenerate.
    fn region_from_rect(
        rect: kurbo::Rect,
        support: f32,
        scene_dimensions: SceneDimensions,
    ) -> Option<crate::timeline::effects::EffectRegion> {
        let x0 = (rect.x0 as f32 - support).max(0.0);
        let y0 = (rect.y0 as f32 - support).max(0.0);
        let x1 = (rect.x1 as f32 + support).min(scene_dimensions.width as f32);
        let y1 = (rect.y1 as f32 + support).min(scene_dimensions.height as f32);
        let width = (x1 - x0).floor().max(1.0) as u32;
        let height = (y1 - y0).floor().max(1.0) as u32;
        if width >= scene_dimensions.width && height >= scene_dimensions.height {
            return None;
        }
        Some(crate::timeline::effects::EffectRegion {
            origin: [x0, y0],
            size: SceneDimensions { width, height },
        })
    }
}
