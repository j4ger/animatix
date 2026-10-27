use crate::timeline::{EntityId, Timeline, TrackAccessor};

impl Timeline {
    /// Glass strategy: the panel's children render into their own sub-scene, a
    /// backdrop pass is queued first so the pixels *behind* the panel are
    /// blurred, and the children are composited above that blur. Reached only
    /// through `GlassPrimitive::render_children`.
    ///
    /// The order of the two queue entries is the whole trick: the drain composites
    /// in queue order, so `backdrop` then `children` reproduces the depth the walk
    /// had, which no single `vello::Scene` can express (the pinned vello always
    /// clears its target, so the "above" half cannot simply be rendered into the
    /// main target afterwards).
    fn render_glass_children(
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
        // Resolve the scope's entity id: trust `node_id`, fall back to the label.
        let node_id = if self.world.exists(node_id) {
            node_id
        } else if let Some(id) = self.world.get_id(node_label) {
            id
        } else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_id, time_ms)
        } else {
            crate::timeline::layout::empty_layout_positions()
        };

        // A glass scope with no effect stages has nothing to blur: render it as
        // an ordinary container rather than paying a backdrop pass for a copy.
        //
        // `allow_pending_composites` is deliberately NOT part of this gate. That
        // flag answers a question `Filter` asks itself — "may I blit after the
        // frame, given something renders after me?" — and `Glass` has the
        // opposite answer: the frost *is* what the finished frame holds under
        // the panel, so painting it over later siblings is the documented
        // behaviour, not an ordering bug. Honouring the flag here silently
        // disabled every glass panel that was not the scene's last root actor.
        let chain = self
            .world
            .effects(node_id)
            .map(|effects| effects.build_chain(time_ms))
            .unwrap_or_default();
        let has_backend = out.filter_backend.is_some();
        if chain.is_empty() || !has_backend {
            if !has_backend && !chain.is_empty() {
                self.eval_caches.runtime_diagnostics.borrow_mut().push(
                    crate::diagnostics::Diagnostic::warning(
                        crate::diagnostics::DiagnosticCode::RenderFailure,
                        crate::diagnostics::DiagnosticPhase::Render,
                        format!(
                            "Glass '{node_label}' has no filter backend available; \
                             rendering children without the backdrop blur"
                        ),
                    ),
                );
            }
            for &child_id in &self.world.hierarchy(node_id).children {
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
            return;
        }

        // The panel rect in render-target pixels. `global_transform` already
        // carries the camera for anything under a root subtree, so mapping the
        // local half-size is enough — and it is what keeps the blur region on the
        // same pixels the panel was drawn with.
        let half = self
            .world
            .geometry(node_id)
            .size
            .get(time_ms, crate::timeline::DEFAULT_LAYOUT_HALF_SIZE);
        let panel = kurbo::Rect::new(
            -f64::from(half[0]),
            -f64::from(half[1]),
            f64::from(half[0]),
            f64::from(half[1]),
        );
        let panel = {
            let corners = [
                kurbo::Point::new(panel.x0, panel.y0),
                kurbo::Point::new(panel.x1, panel.y0),
                kurbo::Point::new(panel.x1, panel.y1),
                kurbo::Point::new(panel.x0, panel.y1),
            ];
            let mapped: Vec<kurbo::Point> = corners.iter().map(|c| global_transform * *c).collect();
            let xs: Vec<f64> = mapped.iter().map(|p| p.x).collect();
            let ys: Vec<f64> = mapped.iter().map(|p| p.y).collect();
            kurbo::Rect::new(
                xs.iter().copied().fold(f64::INFINITY, f64::min),
                ys.iter().copied().fold(f64::INFINITY, f64::min),
                xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            )
        };
        if panel.width() <= 0.0 || panel.height() <= 0.0 {
            return;
        }

        let full = crate::timeline::effects::EffectRegion {
            origin: [0.0, 0.0],
            size: scene_dimensions,
        };
        let clip = Self::region_from_rect(panel, 0.0, scene_dimensions).unwrap_or(full);
        // The blur needs source pixels from outside the panel, or its edges smear
        // against the region border; the composite is clipped back to the panel,
        // so the extra margin is never painted.
        let region = Self::region_from_rect(
            panel,
            self.world
                .effects(node_id)
                .map(|effects| effects.worst_case_support())
                .unwrap_or(0.0),
            scene_dimensions,
        )
        .unwrap_or(full);

        let mut sub_scene = vello::Scene::new();
        out.with_scene(&mut sub_scene, |out| {
            for &child_id in &self.world.hierarchy(node_id).children {
                self.evaluate_node_by_id(
                    child_id,
                    global_transform,
                    global_opacity,
                    &child_layout_positions,
                    false,
                    frame,
                    out,
                );
            }
        });

        // The mask is in render-target pixels, so the authored radius scales
        // with whatever the transform did to the panel.
        let [a, b, ..] = global_transform.as_coeffs();
        let scale = (a * a + b * b).sqrt().max(0.0);
        let corner_radius =
            self.world.shape_or_default(node_id).corner_radius.get(time_ms, 0.0) as f64 * scale;
        let Some(backend) = out.filter_backend.as_mut() else {
            return;
        };
        let backdrop = crate::timeline::effects::PendingBackdrop {
            region,
            clip,
            chain,
            corner_radius: corner_radius as f32,
            alpha: global_opacity,
        };
        if let Err(e) = backend.enqueue_backdrop(&backdrop) {
            tracing::warn!("Glass '{node_label}' backdrop could not be queued: {e}");
            return;
        }
        if let Err(e) =
            backend.enqueue_scene_composite(&sub_scene, scene_dimensions, [0.0, 0.0], 1.0)
        {
            tracing::warn!("Glass '{node_label}' children could not be queued: {e}");
        }
    }

    /// Apply the Glass strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_glass_children_ctx(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
    ) {
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
        self.render_glass_children(
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
