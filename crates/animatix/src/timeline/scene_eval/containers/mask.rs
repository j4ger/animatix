use kurbo::Shape;

use crate::timeline::{DEFAULT_LAYOUT_HALF_SIZE, EntityId, Timeline, TrackAccessor};

impl Timeline {
    /// Mask strategy: children render inside the Mask's clip geometry. Reached
    /// only through `MaskPrimitive::render_children`.
    fn render_mask_children(
        &self,
        node_id: EntityId,
        node_label: &str,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let overrides = frame.overrides;
        let Some(track) = self
            .world
            .tracks
            .get_by_id(node_id)
            .or_else(|| self.world.tracks.get(node_label))
        else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
        };
        let half_size = self.world.geometry(track.id).size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);

        // Resolve the clip geometry. A child labelled `clip_shape` defines
        // the clip and is NOT rendered itself; its primitive supplies the
        // geometry via `Primitive::clip_path`, so any shape (built-in or
        // extension) works. Without one — or when the primitive has no clip
        // geometry — the clip falls back to a rect covering the Mask's own
        // size; the latter case warns instead of silently clipping wrong.
        let clip_shape_track = self
            .world
            .hierarchy(node_id)
            .children
            .iter()
            .filter_map(|id| self.world.tracks.get_by_id(*id))
            .find(|c| self.world.label(c.id).unwrap_or("") == "clip_shape");
        let fallback_clip = || {
            let w = half_size[0] as f64;
            let h = half_size[1] as f64;
            kurbo::Rect::new(-w, -h, w, h).into_path(1e-3)
        };
        let clip_path: kurbo::BezPath = match clip_shape_track {
            Some(child) => {
                match self.clip_path_for_child(child, time_ms, scene_dimensions, overrides) {
                    Some(path) => path,
                    None => {
                        self.eval_caches.runtime_diagnostics.borrow_mut().push(
                            crate::diagnostics::Diagnostic::warning(
                                crate::diagnostics::DiagnosticCode::RenderFailure,
                                crate::diagnostics::DiagnosticPhase::Render,
                                format!(
                                    "Mask clip_shape '{}' provides no clip geometry; \
                                         using a rectangular clip",
                                    self.world.label(child.id).unwrap_or("")
                                ),
                            ),
                        );
                        fallback_clip()
                    },
                }
            },
            None => fallback_clip(),
        };
        let clip_child_id = clip_shape_track.map(|c| c.id);

        // Push clip layer. The clip path is in the mask's LOCAL space, so
        // it must be transformed into out.scene space — pushing it with the
        // identity transform pinned the clip at the out.scene origin, clipping
        // away every child of any mask not positioned at the top-left
        // corner (Mask + Image children were the visible symptom).
        out.scene.push_layer(
            vello::peniko::Fill::NonZero,
            vello::peniko::BlendMode::default(),
            1.0,
            global_transform,
            &clip_path,
        );

        // Render all children normally inside the clip
        for &child_id in &self.world.hierarchy(node_id).children {
            if let Some(child_track) = self.world.tracks.get_by_id(child_id) {
                if clip_child_id == Some(child_track.id) {
                    continue;
                }
                self.evaluate_node_by_id(
                    child_id,
                    global_transform,
                    global_opacity,
                    &child_layout_positions,
                    allow_pending_composites,
                    frame,
                    out,
                );
            }
        }

        // Pop clip layer
        out.scene.pop_layer();
    }

    /// Apply the Mask strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_mask_children_ctx(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
    ) {
        // The container primitives drive their own recursion through
        // `RenderChildrenCtx` (the documented extension surface); the render
        // recursion itself takes the bundled frame/outputs.
        let frame = crate::primitives::RenderFrame {
            time_ms: ctx.time_ms,
            scene_dimensions: ctx.scene_dimensions,
            debug_options: ctx.debug_options,
            overrides: ctx.overrides,
            frame_env: ctx.frame_env,
        };
        let mut out = crate::primitives::RenderOutputs {
            scene: &mut *ctx.scene,
            hit_regions: &mut *ctx.hit_regions,
            program_items: &mut *ctx.program_items,
            filter_backend: &mut *ctx.filter_backend,
        };
        self.render_mask_children(
            ctx.host_id,
            ctx.node_label,
            ctx.global_transform,
            ctx.global_opacity,
            ctx.allow_pending_composites,
            &frame,
            &mut out,
        );
    }
}
