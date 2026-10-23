use crate::timeline::{EntityId, Timeline};

impl Timeline {
    /// Adapter used by the [`crate::primitives::Primitive::render_children`]
    /// Filter strategy: children render into an offscreen scene, the Filter's
    /// post-processing is applied, and the result is composited back. Reached
    /// only through `FilterPrimitive::render_children`.
    fn render_filter_children(
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
        if self.world.hierarchy(node_id).children.is_empty() || global_opacity <= 0.0001 {
            return;
        }

        // Check if a filter backend is available
        let has_backend = out.filter_backend.is_some();
        if !has_backend {
            // Surface the fallback: authored filter effects are silently
            // dropped without it.
            self.eval_caches.runtime_diagnostics.borrow_mut().push(
                crate::diagnostics::Diagnostic::warning(
                    crate::diagnostics::DiagnosticCode::RenderFailure,
                    crate::diagnostics::DiagnosticPhase::Render,
                    format!(
                        "Filter '{node_label}' has no filter backend available; \
                             rendering children unfiltered"
                    ),
                ),
            );
            // Fallback: render children directly (no filtering)
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

        // Build sub-scene with children rendered at their world positions
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

        // Effects are lowered onto the scope's chain at build time; identity
        // and disabled stages are dropped during sampling.
        let chain = track.effects.build_chain(time_ms);

        if chain.is_empty() {
            out.scene.encoding_mut().append(sub_scene.encoding(), &None);
            return;
        }

        // Region of interest, in priority order:
        // 1. an authored `bounds: (x, y, w, h)`;
        // 2. derived from the content bounds the sub-scene evaluation just recorded
        //    (`docs/effects.md` §3), expanded by worst-case support;
        // 3. `None` — the historical full-scene path.
        // The GPU textures stay at full scene capacity in every case, so
        // varying regions never reallocate (PF-7).
        // Authored bounds are scene coordinates, and the camera moves the
        // picture under them. Recompute the same affine the root subtrees were
        // wrapped in (see the camera application in `evaluate_scene`) rather
        // than the scope's own world transform: the authored numbers already
        // account for the scope's placement.
        let camera = if self.camera_used.get() {
            self.camera.affine(
                time_ms,
                scene_dimensions,
                frame.overrides.get(crate::timeline::camera::CAMERA_TARGET),
            )
        } else {
            kurbo::Affine::IDENTITY
        };
        let region = match self.effect_scope_region(track, scene_dimensions, time_ms, camera) {
            Some(region) => Some(region),
            None => {
                let mut content: Option<kurbo::Rect> = None;
                for child in self.world.child_labels(node_id) {
                    self.subtree_bounds_union(child, &mut content);
                }
                content.and_then(|rect| {
                    Self::region_from_rect(
                        rect,
                        track.effects.worst_case_support(),
                        scene_dimensions,
                    )
                })
            },
        };

        if let Some(r) = region {
            if r.origin[0] >= scene_dimensions.width as f32
                || r.origin[1] >= scene_dimensions.height as f32
                || r.origin[0] + r.size.width as f32 <= 0.0
                || r.origin[1] + r.size.height as f32 <= 0.0
            {
                // The effect scope's region of interest is entirely off-screen.
                return;
            }
        }

        // Always take the zero-readback path when a backend exists: the
        // pending composites are blitted after the main scene render in
        // declaration order, and the CPU readback path is the one that loses
        // full-canvas scopes in exported video (see docs/roadmap.md Known
        // Issues).
        if allow_pending_composites {
            if let Some(backend) = out.filter_backend.as_mut() {
                match backend.render_scene_to_pending_composite(
                    &sub_scene,
                    scene_dimensions,
                    region,
                    &chain,
                    global_opacity,
                ) {
                    Ok(()) => {
                        // Filter output is stored as a pending GPU composite.
                        // The renderer will blit it after the main scene render.
                        return;
                    },
                    Err(e) => {
                        tracing::warn!(
                            "Zero-readback filter path failed, falling back to readback: {e}"
                        );
                    },
                }
            }
        }

        // Render sub-scene to image via backend, apply GPU filters, draw result
        if let Some(backend) = out.filter_backend.as_mut() {
            match backend.render_scene_to_image_gpu_filtered(
                &sub_scene,
                scene_dimensions,
                region,
                &chain,
            ) {
                Ok(filtered) => {
                    // A region-scoped result is composited back at its origin;
                    // a full-scene result covers the target exactly.
                    let transform = match region {
                        Some(region) => kurbo::Affine::translate((
                            region.origin[0] as f64,
                            region.origin[1] as f64,
                        )),
                        None => kurbo::Affine::IDENTITY,
                    };
                    let brush = vello::peniko::ImageBrush::new(filtered.data.clone())
                        .with_extend(vello::peniko::Extend::Pad)
                        .with_quality(vello::peniko::ImageQuality::Medium)
                        .with_alpha(global_opacity);
                    out.scene.draw_image(&brush, transform);
                },
                Err(e) => {
                    tracing::warn!(
                        "Filter backend error, falling back to unfiltered rendering: {e}"
                    );
                    self.eval_caches.runtime_diagnostics.borrow_mut().push(
                        crate::diagnostics::Diagnostic::warning(
                            crate::diagnostics::DiagnosticCode::RenderFailure,
                            crate::diagnostics::DiagnosticPhase::Render,
                            format!(
                                "Filter '{node_label}' backend failed ({e}); \
                                     rendering children unfiltered"
                            ),
                        ),
                    );
                    out.scene.encoding_mut().append(sub_scene.encoding(), &None);
                },
            }
        }
    }

    /// Apply the Filter strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_filter_children_ctx(
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
        self.render_filter_children(
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
