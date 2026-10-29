//! Node rendering and child recursion.

use super::*;

impl Timeline {
    /// Check whether a filter actor can safely use zero-readback post-render compositing.
    /// This is only safe when the filter is the last child in every ancestor container
    /// (nothing renders after the filter in the scene graph).
    pub(crate) fn can_post_composite_filter(&self, node_id: EntityId) -> bool {
        let Some(label) = self.world.label(node_id) else {
            return false;
        };
        // Find the path from root to this actor
        let Some(path) = self.find_path_to_actor(label) else {
            return false;
        };

        // The actor must be in the root set (no orphan check)
        if path.is_empty() {
            return false;
        }

        // Check that at every level, the actor is the last child. The root
        // node set counts as the outermost container: a root-level filter
        // must be the last root node, or later siblings would render *under*
        // the post-render blit instead of after it.
        for i in 0..path.len() {
            let id = match self.world.get_id(&path[i]) {
                Some(id) => id,
                None => return false,
            };

            if i == 0 {
                if self.world.root_entities().last() != Some(&id) {
                    return false;
                }
                continue;
            }

            // Check it's the last child of its parent
            let Some(parent_id) = self.world.get_id(&path[i - 1]) else {
                return false;
            };
            if self.world.hierarchy(parent_id).children.last() != Some(&id) {
                return false;
            }
        }

        true
    }

    pub(crate) fn evaluate_node_by_id(
        &self,
        node_id: EntityId,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        if !self.world.exists(node_id) {
            return;
        }
        let blend = self.track_blend_mode(node_id, frame.time_ms);
        if let Some(mix) = blend {
            let unbounded = kurbo::Rect::new(-1.0e9, -1.0e9, 1.0e9, 1.0e9);
            out.scene.push_layer(
                vello::peniko::Fill::NonZero,
                vello::peniko::BlendMode::new(mix, vello::peniko::Compose::SrcOver),
                1.0,
                parent_transform,
                &unbounded,
            );
        }

        let (global_transform, global_opacity) = self.render_actor_node_by_id(
            node_id,
            parent_transform,
            parent_opacity,
            layout_positions,
            allow_pending_composites,
            frame,
            out,
        );

        self.render_node_children_by_id(
            node_id,
            global_transform,
            global_opacity,
            allow_pending_composites,
            frame,
            out,
        );

        if blend.is_some() {
            out.scene.pop_layer();
        }
    }

    pub(crate) fn evaluate_node(
        &self,
        node_label: &str,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        if let Some(id) = self.world.tracks.get_id(node_label) {
            self.evaluate_node_by_id(
                id,
                parent_transform,
                parent_opacity,
                layout_positions,
                allow_pending_composites,
                frame,
                out,
            );
        }
    }

    /// Sample the node's `blend:` track; `None` when it is absent or
    /// "normal" (the source-over default, no layer).
    fn track_blend_mode(&self, id: EntityId, time_ms: u64) -> Option<vello::peniko::Mix> {
        use crate::timeline::TrackAccessor;

        if !self.blend_used.get() {
            return None;
        }
        // `get_or_default` keeps the absent-track case allocation-free; the
        // old `.get(time_ms, "normal".to_string())` allocated the default
        // `String` for every node in every blend-using scene.
        let mode = self.world.style(id).blend.get_or_default(time_ms);
        match mode.as_deref().unwrap_or("normal") {
            "normal" | "" => None,
            "multiply" => Some(vello::peniko::Mix::Multiply),
            "screen" => Some(vello::peniko::Mix::Screen),
            "overlay" => Some(vello::peniko::Mix::Overlay),
            "darken" => Some(vello::peniko::Mix::Darken),
            "lighten" => Some(vello::peniko::Mix::Lighten),
            "color-dodge" => Some(vello::peniko::Mix::ColorDodge),
            "color-burn" => Some(vello::peniko::Mix::ColorBurn),
            "hard-light" => Some(vello::peniko::Mix::HardLight),
            "soft-light" => Some(vello::peniko::Mix::SoftLight),
            "difference" => Some(vello::peniko::Mix::Difference),
            "exclusion" => Some(vello::peniko::Mix::Exclusion),
            "hue" => Some(vello::peniko::Mix::Hue),
            "saturation" => Some(vello::peniko::Mix::Saturation),
            "color" => Some(vello::peniko::Mix::Color),
            "luminosity" => Some(vello::peniko::Mix::Luminosity),
            other => {
                tracing::warn!(
                    "{}: unknown blend mode '{other}' (expected a CSS mix-mode name); treating as normal",
                    self.world.label(id).unwrap_or("")
                );
                None
            },
        }
    }

    fn render_actor_node_by_id(
        &self,
        node_id: EntityId,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) -> (kurbo::Affine, f32) {
        if !self.world.exists(node_id) {
            return (parent_transform, parent_opacity);
        }
        self.render_actor_node_impl(
            node_id,
            parent_transform,
            parent_opacity,
            layout_positions,
            allow_pending_composites,
            frame,
            out,
        )
    }

    fn render_actor_node_impl(
        &self,
        node_id: EntityId,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) -> (kurbo::Affine, f32) {
        let node_label = self.world.label(node_id).unwrap_or("");
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let debug_options = frame.debug_options;
        let overrides = frame.overrides;
        let frame_env = frame.frame_env;
        let scene = &mut *out.scene;
        let hit_regions = &mut *out.hit_regions;
        let program_items = &mut *out.program_items;

        // Authored solo: while any actor is soloed, a subtree that contains no
        // soloed actor is pruned whole — unlike `visible`, which hides only the
        // node itself. Ancestors of a soloed actor stay traversable but do not
        // draw their own commands.
        let draws_self = if self.eval_caches.solo_active.get() {
            let solo = self.eval_caches.solo.borrow();
            if !solo.is_reachable(node_id) {
                return (parent_transform, parent_opacity);
            }
            solo.is_soloed(node_id)
        } else {
            true
        };

        // Skip actors that haven't been declared yet.
        // They are not clickable in the preview canvas before they appear;
        // selection is still possible via the layers / inspector tab.
        if time_ms < self.world.meta(node_id).first_seen_ms {
            // Still recurse into children so they are also hidden
            for &child_id in &self.world.hierarchy(node_id).children {
                self.evaluate_node_by_id(
                    child_id,
                    parent_transform,
                    parent_opacity,
                    layout_positions,
                    allow_pending_composites,
                    frame,
                    out,
                );
            }
            return (parent_transform, parent_opacity);
        }

        // ── Evaluate transform first for visibility culling ──
        // Extract node overrides from modifier (always block) so spatial
        // properties (position, rotation, scale, opacity, shift, size, transform)
        // are applied rather than being silently ignored.
        let node_overrides = overrides.get(node_label);

        let layout_pos = if self.dynamic_layout {
            layout_positions.get_by_id(node_id).copied()
        } else {
            None
        };

        // Temporal coherence — cache node transforms to avoid
        // re-sampling properties when the same (time, parent_transform) is
        // evaluated again; keyframe-free entities reuse across times (see
        // `cached_node_transform`).
        let node_transform = self.cached_node_transform(
            node_id,
            time_ms,
            parent_opacity,
            parent_transform,
            scene_dimensions,
            layout_pos,
            node_overrides,
        );
        let half_size = node_transform.half_size;
        let opacity = node_transform.opacity;
        let local_transform = node_transform.local_transform;

        // Skip hidden actors (visibility toggle in GUI). Children are still
        // evaluated in render_node_children with the correct parent transform.
        if !self.world.meta(node_id).visible {
            return (local_transform, opacity);
        }

        // Viewport culling — skip rendering for off-screen actors.
        // Compute a conservative world-space bounding box and check intersection
        // with the viewport. Children are still evaluated since they may extend
        // back into view even when the parent is off-screen.
        let half_extent =
            (half_size[0].max(half_size[1]) as f64 * node_transform.scale.abs()) + 100.0;
        let coeffs = local_transform.as_coeffs();
        let is_visible = coeffs[4] + half_extent > 0.0
            && coeffs[4] - half_extent < scene_dimensions.width as f64
            && coeffs[5] + half_extent > 0.0
            && coeffs[5] - half_extent < scene_dimensions.height as f64;
        if !is_visible || !draws_self {
            return (local_transform, opacity);
        }

        // Vector-path sampling (shape tier, bar-chart transitions, procedural
        // plots) and behavior dispatch live in their own phases; both are pure
        // reads of this node's frame state.
        let vector_paths = self.evaluate_node_vector_paths(
            node_id,
            time_ms,
            scene_dimensions,
            overrides,
            frame_env,
        );
        let primitive_dispatch = self.evaluate_behavior_commands(
            node_id,
            node_label,
            time_ms,
            local_transform,
            opacity,
            scene_dimensions,
            node_overrides,
            &vector_paths,
        );
        if let Some(commands) = primitive_dispatch {
            self.draw_node_commands(
                node_id,
                node_label,
                time_ms,
                debug_options,
                local_transform,
                opacity,
                half_size,
                &vector_paths,
                commands,
                scene,
                hit_regions,
                program_items,
            );
            return (local_transform, opacity);
        }

        (local_transform, opacity)
    }

    /// Resolve the primitive an entity renders with at `time_ms` as an owned
    /// handle (cheap clone). The entity's `actor_type` is the registry key —
    /// except across a cross-type morph, where the per-frame `shape_type`
    /// value picks the primitive; see `ShapeTracks::render_type_name`.
    /// Served from the per-entity cache (`timeline_impl::primitive_cache`).
    pub(crate) fn track_primitive_at(
        &self,
        node_id: EntityId,
        time_ms: u64,
    ) -> Option<crate::primitives::PrimitiveHandle> {
        self.cached_primitive_for(node_id, time_ms)
    }

    /// Local-space clip geometry for a Mask's `clip_shape` child, obtained from
    /// the child's own primitive via [`Primitive::clip_path`]. Placed at the
    /// child's declared position (the default clip geometry is origin-centered)
    /// to match the previous Rect/Ellipse behavior. `None` means the primitive
    /// has no clip geometry — the caller warns and falls back to a rectangle.
    pub(crate) fn clip_path_for_child(
        &self,
        child_id: EntityId,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Option<kurbo::BezPath> {
        let handle = self.cached_primitive_for(child_id, time_ms)?;
        let primitive = handle.as_dyn();
        let vector_paths = self
            .world
            .shape_or_default(child_id)
            .evaluate_vector_paths(self.world.style(child_id), time_ms);
        self.world.begin_shape_commands(child_id);
        let payload = self.world.payload(child_id);
        let ctx = crate::primitives::EvaluateCtx {
            id: child_id,
            label: self.world.label(child_id).unwrap_or(""),
            time_ms,
            local_transform: kurbo::Affine::IDENTITY,
            opacity: 1.0,
            scene_dimensions,
            background_color: self.eval_caches.background_color.get(),
            overrides: overrides.get(self.world.label(child_id).unwrap_or("")),
            vector_paths: &vector_paths,
            asset_cache: &self.asset_cache,
            target_resolver: Some(self),
            legend: self.world.legend(child_id),
            world: Some(&self.world),
            highlight: self.world.highlight(child_id),
            style: self.world.style(child_id),
            shape: self.world.shape(child_id),
            geometry: self.world.geometry(child_id),
            text: self.world.text(child_id),
            property_plan: self.world.property_plan(child_id),
            caps: self.world.caps(child_id),
            payload: payload.as_ref(),
        };
        let path = primitive.clip_path(&ctx, &ctx.caps);
        // Consume the memo-bounds handoff so it can't leak into the next node.
        let _ = self.world.take_shape_command_bounds(child_id);
        // Place the (origin-centered) clip geometry with the child's fully
        // resolved local transform — anchor/offset/rotation/scale included, not
        // just its raw position.
        let node = self.evaluate_node_transform(
            child_id,
            time_ms,
            1.0,
            kurbo::Affine::IDENTITY,
            scene_dimensions,
            None,
            overrides.get(self.world.label(child_id).unwrap_or("")),
        );
        path.map(|mut path| {
            path.apply_affine(node.local_transform);
            path
        })
    }

    /// Recursively render child nodes using the primitive capability hook by EntityId.
    pub(crate) fn render_node_children_by_id(
        &self,
        node_id: EntityId,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        if !self.world.exists(node_id) {
            return;
        }
        self.render_node_children_impl(
            node_id,
            global_transform,
            global_opacity,
            allow_pending_composites,
            frame,
            out,
        );
    }

    fn render_node_children_impl(
        &self,
        node_id: EntityId,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let node_label = self.world.label(node_id).unwrap_or("");
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let debug_options = frame.debug_options;
        let overrides = frame.overrides;
        let frame_env = frame.frame_env;
        // The primitive is the single child-rendering entry point; the pipeline
        // does not branch on the child-processing strategy. A primitive without
        // a registered type renders nothing (validated at build time).
        let Some(handle) = self.cached_primitive_for(node_id, time_ms) else {
            return;
        };
        let primitive = handle.as_dyn();

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_id, time_ms)
        } else {
            crate::timeline::layout::empty_layout_positions()
        };
        let mut ctx = crate::primitives::RenderChildrenCtx {
            timeline: self,
            host_id: node_id,
            node_label,
            time_ms,
            global_transform,
            global_opacity,
            scene_dimensions,
            debug_options,
            overrides,
            layout_positions: child_layout_positions,
            frame_env,
            allow_pending_composites,
            scene: &mut *out.scene,
            hit_regions: &mut *out.hit_regions,
            program_items: &mut *out.program_items,
            filter_backend: &mut *out.filter_backend,
        };
        let res =
            primitive.render_children_by_id(&mut ctx, &self.world.hierarchy(node_id).children);
        if let Err(e) = res {
            let label = node_label.to_string();
            self.eval_caches.runtime_diagnostics.borrow_mut().push(
                crate::diagnostics::Diagnostic::warning(
                    crate::diagnostics::DiagnosticCode::RenderFailure,
                    crate::diagnostics::DiagnosticPhase::Render,
                    format!("failed to render children of '{label}': {e}"),
                ),
            );
        }
    }
}
