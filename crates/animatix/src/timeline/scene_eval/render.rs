//! Node rendering and child recursion.

use super::*;

/// The historical full-surface blend layer. Kept as the fallback whenever a
/// tight, world-space clip cannot be proven correct; a wrong clip is a visible
/// correctness regression, while this rect is only a perf cost.
#[inline]
fn unbounded_blend_layer_rect() -> kurbo::Rect {
    kurbo::Rect::new(-1.0e9, -1.0e9, 1.0e9, 1.0e9)
}

/// Whether a rect is usable as a clip: all coordinates finite and positive
/// area. Non-finite or degenerate bounds must never reach `push_layer`.
#[inline]
fn blend_rect_usable(r: kurbo::Rect) -> bool {
    r.x0.is_finite()
        && r.y0.is_finite()
        && r.x1.is_finite()
        && r.y1.is_finite()
        && r.width() > 0.0
        && r.height() > 0.0
}

/// A node that renders its children through a clip, offscreen effect scope, or
/// filter backend. `precise_bounds` from such subtrees cannot be trusted as a
/// complete world-space ink union, so a tight blend clip is unsafe around them.
fn blend_node_is_special(caps: animatix_core::caps::ActorCaps, has_effects: bool) -> bool {
    caps.is_effect_scope()
        || caps.child_processing == animatix_core::caps::ChildProcessingKind::Mask
        || has_effects
}

/// Whether an affine is the identity within a tight epsilon. `precise_bounds`
/// are world-space (the full accumulated ancestor transform), while the layer
/// clip shape is expressed in the caller's `parent_transform` space; those two
/// spaces only coincide when `parent_transform` is the identity. Rather than
/// inverting a possibly-singular affine to map the rect back, a non-identity
/// parent transform keeps the unbounded fallback.
#[inline]
fn affine_is_identity(a: kurbo::Affine) -> bool {
    const EPS: f64 = 1.0e-9;
    let c = a.as_coeffs();
    (c[0] - 1.0).abs() < EPS
        && c[1].abs() < EPS
        && c[2].abs() < EPS
        && (c[3] - 1.0).abs() < EPS
        && c[4].abs() < EPS
        && c[5].abs() < EPS
}

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
        let Some(mix) = self.track_blend_mode(node_id, frame.time_ms) else {
            // No blend: draw the subtree straight into the current scene. This
            // fast path is byte-for-byte the pre-change behaviour.
            self.render_node_subtree(
                node_id,
                parent_transform,
                parent_opacity,
                layout_positions,
                allow_pending_composites,
                frame,
                out,
            );
            return;
        };

        // A `blend:` actor wraps its subtree in a Vello clip layer. Historically
        // that layer spanned the whole surface (±1e9) so nothing could be
        // clipped by accident; every blended subtree then forces a
        // full-canvas composite on every frame (the `StageFloor` glows are the
        // hot case, see docs/effects.md §"layer is full-surface"). Where the
        // subtree's world-space ink bounds are provably known and simple we
        // clip to those bounds instead. A wrong clip is a visible correctness
        // regression while the unbounded layer is only a perf cost, so the
        // conservative gate keeps the historical layer whenever there is any
        // doubt (camera, mask/clip, effect scope, non-finite affine).
        if !self.blend_bounds_are_safe(node_id, parent_transform) {
            tracing::debug!(
                target: "animatix::blend_bounds",
                "blend layer '{}': unbounded fallback (subtree not provably simple)",
                self.world.label(node_id).unwrap_or("")
            );
            let unbounded = unbounded_blend_layer_rect();
            out.scene.push_layer(
                vello::peniko::Fill::NonZero,
                vello::peniko::BlendMode::new(mix, vello::peniko::Compose::SrcOver),
                1.0,
                parent_transform,
                &unbounded,
            );
            self.render_node_subtree(
                node_id,
                parent_transform,
                parent_opacity,
                layout_positions,
                allow_pending_composites,
                frame,
                out,
            );
            out.scene.pop_layer();
            return;
        }

        // Two-pass, mirroring the Filter ROI derivation (containers/filter.rs):
        // render the blended subtree once into a scratch scene so its per-node
        // world-space bounds land in the `precise_bounds` slot table
        // (`record_precise_bounds` is global, not per-scene). Ordering matters:
        // `prepare_frame` clears the slot table at frame start, so the scratch
        // render here REPOPULATES it before `blend_layer_clip_rect` reads it
        // below — bounds from a previous frame are never relied upon. Rendering
        // into a scratch scene and appending is the same pattern the
        // static-subtree cache already relies on.
        let mut scratch = vello::Scene::new();
        out.with_scene(&mut scratch, |swapped| {
            self.render_node_subtree(
                node_id,
                parent_transform,
                parent_opacity,
                layout_positions,
                allow_pending_composites,
                frame,
                swapped,
            );
        });

        // `None` means bounds were unavailable, degenerate, non-finite, or a
        // child extended beyond the parent's own bounds: keep the full-surface
        // layer rather than risking a wrong clip. The gate above guarantees the
        // parent transform is (near) identity here, so a world-space rect
        // pushed with `Affine::IDENTITY` clips in exactly the space the draws
        // were encoded in; the fallback keeps the original `parent_transform`.
        let clip = self.blend_layer_clip_rect(node_id);
        let (rect, transform) = match clip {
            Some(r) => (r, kurbo::Affine::IDENTITY),
            None => {
                tracing::debug!(
                    target: "animatix::blend_bounds",
                    "blend layer '{}': unbounded fallback (bounds unavailable or unsafe)",
                    self.world.label(node_id).unwrap_or("")
                );
                (unbounded_blend_layer_rect(), parent_transform)
            },
        };
        out.scene.push_layer(
            vello::peniko::Fill::NonZero,
            vello::peniko::BlendMode::new(mix, vello::peniko::Compose::SrcOver),
            1.0,
            transform,
            &rect,
        );
        out.scene.encoding_mut().append(scratch.encoding(), &None);
        out.scene.pop_layer();
    }

    /// Draw one node and its children into `out` (no blend layer). Shared by the
    /// no-blend and unbounded-fallback paths, and by the tightened scratch pass.
    fn render_node_subtree(
        &self,
        node_id: EntityId,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        layout_positions: &crate::timeline::layout::LayoutPositions,
        allow_pending_composites: bool,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
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
    }

    /// Whether a tight, world-space clip for this blend subtree is provably
    /// safe. Conservative by construction: a camera transform, a non-identity
    /// accumulated `parent_transform` (world-space `precise_bounds` and the
    /// parent-space clip would otherwise disagree), any mask/clip or effect
    /// scope in the ancestor chain or the subtree, or a non-finite affine keeps
    /// the historical unbounded layer.
    fn blend_bounds_are_safe(&self, node_id: EntityId, parent_transform: kurbo::Affine) -> bool {
        if self.camera_used.get() {
            return false;
        }
        let coeffs = parent_transform.as_coeffs();
        if !coeffs.iter().all(|c| c.is_finite()) || !affine_is_identity(parent_transform) {
            return false;
        }
        // Ancestors: a mask/clip or effect scope above the blend node wraps it
        // in another clip / offscreen composite, which a plain world-space
        // rect here cannot be proven to compose with. Walk the ECS `parent`
        // back-reference; bound the walk defensively against a corrupt tree.
        let mut cursor = self.world.hierarchy(node_id).parent;
        let mut guard = 0usize;
        while let Some(id) = cursor {
            guard += 1;
            if guard > 4096 || !self.world.exists(id) {
                return false;
            }
            if blend_node_is_special(self.world.caps(id), self.world.effects(id).is_some()) {
                return false;
            }
            cursor = self.world.hierarchy(id).parent;
        }
        // Descendants: no mask/clip, no effect scope with a support radius.
        !self.blend_subtree_has_special(node_id)
    }

    fn blend_subtree_has_special(&self, id: EntityId) -> bool {
        if !self.world.exists(id) {
            return true;
        }
        if blend_node_is_special(self.world.caps(id), self.world.effects(id).is_some()) {
            return true;
        }
        for &child_id in &self.world.hierarchy(id).children {
            if self.blend_subtree_has_special(child_id) {
                return true;
            }
        }
        false
    }

    /// Derive the tight layer rect from the subtree's recorded world-space
    /// bounds — a `subtree_bounds_union`-style walk over the existing
    /// `precise_bounds` slot table (no new label-keyed map). Returns `None` so
    /// the caller falls back to the unbounded layer whenever bounds are
    /// unavailable, degenerate, non-finite, or a descendant's ink extends
    /// beyond the blend node's own bounds.
    fn blend_layer_clip_rect(&self, node_id: EntityId) -> Option<kurbo::Rect> {
        let label = self.world.label(node_id).unwrap_or("");
        let own = self.precise_bounds_for(label)?;
        if !blend_rect_usable(own) {
            return None;
        }
        let mut accumulated = own;
        let mut outside = false;
        for &child_id in &self.world.hierarchy(node_id).children {
            self.blend_union_bounds(child_id, own, &mut accumulated, &mut outside);
        }
        if outside {
            // A child extends beyond its parent's own bounds: the tight rect
            // cannot be trusted, so keep the full-surface layer.
            return None;
        }
        // A small pad keeps antialiased edge pixels from being clipped by the
        // rect boundary; it is negligible next to the ±1e9 fallback.
        let pad = 2.0;
        let padded = kurbo::Rect::new(
            accumulated.x0 - pad,
            accumulated.y0 - pad,
            accumulated.x1 + pad,
            accumulated.y1 + pad,
        );
        if !blend_rect_usable(padded) {
            return None;
        }
        Some(padded)
    }

    /// Union a descendant's recorded bounds into `accumulated` and flag when it
    /// is not contained by the blend node's own bounds.
    fn blend_union_bounds(
        &self,
        id: EntityId,
        own: kurbo::Rect,
        accumulated: &mut kurbo::Rect,
        outside: &mut bool,
    ) {
        if !self.world.exists(id) {
            return;
        }
        if let Some(label) = self.world.label(id)
            && let Some(rect) = self.precise_bounds_for(label)
        {
            if !blend_rect_usable(rect) {
                // NaN/Inf in a recorded rect: treat as unsafe so the caller
                // keeps the unbounded layer rather than dropping the value.
                *outside = true;
                return;
            }
            if rect.x0 < own.x0 || rect.y0 < own.y0 || rect.x1 > own.x1 || rect.y1 > own.y1 {
                *outside = true;
            }
            *accumulated = accumulated.union(rect);
        }
        for &child_id in &self.world.hierarchy(id).children {
            self.blend_union_bounds(child_id, own, accumulated, outside);
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
    /// Local-space clip geometry for a Mask's `clip_shape` child, obtained from
    /// the child's own behavior evaluation. Placed at the child's declared position
    /// (the default clip geometry is origin-centered) to match the previous Rect/Ellipse behavior.
    /// `None` means the actor has no clip geometry — the caller warns and falls back to a rectangle.
    pub(crate) fn clip_path_for_child(
        &self,
        child_id: EntityId,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Option<kurbo::BezPath> {
        let def = self.behavior_for_node(child_id, time_ms)?;
        if !def.caps.is_shape {
            return None;
        }
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
        let commands = def.evaluate(&ctx, None).ok()??;
        let mut combined = kurbo::BezPath::new();
        for cmd in &commands {
            if let crate::primitives::RenderCommand::Paths { paths } = cmd {
                for vp in paths {
                    combined.extend(vp.path.elements().iter().copied());
                }
            }
        }
        let path = if combined.elements().is_empty() {
            None
        } else {
            Some(combined)
        };
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

        let caps = self.world.caps(node_id);
        match caps.child_processing {
            animatix_core::caps::ChildProcessingKind::Filter => {
                self.render_filter_children_ctx(&mut ctx);
            },
            animatix_core::caps::ChildProcessingKind::Glass => {
                self.render_glass_children_ctx(&mut ctx);
            },
            animatix_core::caps::ChildProcessingKind::Mask => {
                self.render_mask_children_ctx(&mut ctx);
            },
            animatix_core::caps::ChildProcessingKind::Equation => {
                self.render_equation_children_ctx(&mut ctx);
            },
            animatix_core::caps::ChildProcessingKind::Captions => {
                self.render_captions_children_ctx(&mut ctx);
            },
            animatix_core::caps::ChildProcessingKind::Generic => {
                let children: Vec<EntityId> = self.world.hierarchy(node_id).children.to_vec();
                ctx.render_children_ids(&children);
            },
        }
    }
}
