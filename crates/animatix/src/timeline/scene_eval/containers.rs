use kurbo::Shape;

use crate::renderer::types::TextPath;
use crate::timeline::{
    AnimationTrack, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, EntityId, SceneDimensions, Timeline,
    TrackAccessor,
};

/// Escape a Fragment's content for embedding in one #box() markup block.
///
/// Typst parses a leading '+' as an enum item and a leading '-' as a bullet,
/// so a term fragment like "+ sin(3x)/3" otherwise renders as the list marker
/// "1." — escape the marker so equations read as written.
pub(crate) fn equation_markup_escaped(content: &str) -> String {
    let trimmed = content.trim_start();
    if trimmed.starts_with('+') || trimmed.starts_with('-') {
        // Keep any leading whitespace, escape the marker itself.
        let cut = content.len() - trimmed.len();
        format!("{}\\{}", &content[..cut], trimmed)
    } else {
        content.to_string()
    }
}

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
        let Some(track) = self.tracks.get_by_id(node_id).or_else(|| self.tracks.get(node_label))
        else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
        };
        if track.children_ids.is_empty() || global_opacity <= 0.0001 {
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
            for &child_id in &track.children_ids {
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
            for &child_id in &track.children_ids {
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
                for child in &track.children {
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
        track: &AnimationTrack,
        scene_dimensions: SceneDimensions,
        time_ms: u64,
        camera: kurbo::Affine,
    ) -> Option<crate::timeline::effects::EffectRegion> {
        let value = crate::timeline::dispatch::read_property_value(
            track,
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
        let camera = if track.camera_follow && (track.parallax - 1.0).abs() < f32::EPSILON {
            camera
        } else if !track.camera_follow || track.parallax == 0.0 {
            kurbo::Affine::IDENTITY
        } else {
            let p = track.parallax as f64;
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
            track.effects.worst_case_support(),
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
        if let Some(track) = self.tracks.get(label) {
            for child in &track.children {
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
        let Some(track) = self.tracks.get_by_id(node_id).or_else(|| self.tracks.get(node_label))
        else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
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
        let chain = track.effects.build_chain(time_ms);
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
            for &child_id in &track.children_ids {
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
        let half = track.geometry.size.get(time_ms, crate::timeline::DEFAULT_LAYOUT_HALF_SIZE);
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
        let region =
            Self::region_from_rect(panel, track.effects.worst_case_support(), scene_dimensions)
                .unwrap_or(full);

        let mut sub_scene = vello::Scene::new();
        out.with_scene(&mut sub_scene, |out| {
            for &child_id in &track.children_ids {
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
        let corner_radius = track.shape.corner_radius.get(time_ms, 0.0) as f64 * scale;
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
        let Some(track) = self.tracks.get_by_id(node_id).or_else(|| self.tracks.get(node_label))
        else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
        };
        let half_size = track.geometry.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);

        // Resolve the clip geometry. A child labelled `clip_shape` defines
        // the clip and is NOT rendered itself; its primitive supplies the
        // geometry via `Primitive::clip_path`, so any shape (built-in or
        // extension) works. Without one — or when the primitive has no clip
        // geometry — the clip falls back to a rect covering the Mask's own
        // size; the latter case warns instead of silently clipping wrong.
        let clip_shape_track = track
            .children_ids
            .iter()
            .filter_map(|id| self.tracks.get_by_id(*id))
            .find(|c| c.label == "clip_shape")
            .or_else(|| {
                track
                    .children
                    .iter()
                    .filter_map(|c| self.tracks.get(c))
                    .find(|c| c.label == "clip_shape")
            });
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
                                    child.label
                                ),
                            ),
                        );
                        fallback_clip()
                    },
                }
            },
            None => fallback_clip(),
        };
        let clip_child_label = clip_shape_track.map(|c| c.label.as_str());

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
        for &child_id in &track.children_ids {
            if let Some(child_track) = self.tracks.get_by_id(child_id) {
                if clip_child_label == Some(child_track.label.as_str()) {
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

    /// Equation strategy: fragment children aggregate into one Typst document.
    /// Reached only through `EquationPrimitive::render_children`.
    fn render_equation_children(
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
        let Some(track) = self.tracks.get_by_id(node_id).or_else(|| self.tracks.get(node_label))
        else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
        };
        // Collect Fragment children with their content and highlight state.
        struct FragInfo {
            content: String,
            hl_color: [f32; 4],
            hl_opacity: f32,
            hl_padding: f32,
            hl_radius: f32,
            hl_blend: vello::peniko::Mix,
        }
        let mut frags: Vec<FragInfo> = Vec::new();

        let child_tracks: Vec<&AnimationTrack> =
            track.children_ids.iter().filter_map(|id| self.tracks.get_by_id(*id)).collect();

        for child_track in child_tracks {
            // A fragment is any child whose primitive opts into
            // `equation_fragment` — never a hard-coded type name.
            let Some(primitive) = self.track_primitive_at(child_track, time_ms) else {
                continue;
            };
            let child_vector_paths = child_track.evaluate_vector_paths(time_ms);
            let ctx = crate::primitives::EvaluateCtx {
                track: child_track,
                time_ms,
                local_transform: kurbo::Affine::IDENTITY,
                opacity: 1.0,
                scene_dimensions,
                background_color: self.eval_caches.background_color.get(),
                overrides: overrides.get(&child_track.label),
                vector_paths: &child_vector_paths,
                asset_cache: &self.asset_cache,
                target_resolver: Some(self),
            };
            if let Some(fragment) = primitive.equation_fragment(&ctx) {
                frags.push(FragInfo {
                    content: fragment.content,
                    hl_color: fragment.highlight_color,
                    hl_opacity: fragment.highlight_opacity,
                    hl_padding: fragment.highlight_padding,
                    hl_radius: fragment.highlight_radius,
                    hl_blend: fragment.highlight_blend,
                });
            }
        }

        if !frags.is_empty() {
            // Build Typst string: each fragment wrapped in #box() so they
            // produce separate Groups in the output frame.
            let typst_body: String = frags
                .iter()
                .map(|f| format!("#box()[{}]", equation_markup_escaped(&f.content)))
                .collect::<Vec<_>>()
                .join(" ");

            // Use equation-level font_size and color from the Equation track.
            let font_size = track.text.font_size.get(time_ms, 48.0);
            let eq_color = track.style.color.get(time_ms, DEFAULT_WHITE);
            let font_family = track.text.font_family.get(time_ms, String::new());
            let font_weight = track.text.font_weight.get(time_ms, 400.0);
            let font_style = track.text.font_style.get(time_ms, "normal".to_string());
            let line_height = track.text.line_height.get(time_ms, 1.2);
            let letter_spacing = track.text.letter_spacing.get(time_ms, 0.0);
            let word_spacing = track.text.word_spacing.get(time_ms, 0.0);

            // Compile the Typst markup. Memoized process-wide: fragment
            // content and font properties rarely change between frames, so
            // scrubbing an equation-heavy out.scene reuses one compilation
            // instead of re-running Typst every frame.
            match crate::renderer::text::compile_typst_grouped_cached(
                &typst_body,
                font_size,
                eq_color,
                &font_family,
                self.font_context.as_ref(),
                font_weight,
                &font_style,
                line_height,
                letter_spacing,
                word_spacing,
            ) {
                Ok(compiled) => {
                    // One glyph group per #box() wrapper, resolved at
                    // compile time and shared through the memo.
                    let all_glyphs: &[TextPath] = &compiled.glyphs;
                    let ranges: &[std::ops::Range<usize>] = &compiled.ranges;

                    // Compute highlight bounding boxes from the shared
                    // glyph groups before drawing them.
                    let mut highlight_cmds: Vec<crate::primitives::RenderCommand> = Vec::new();
                    for (frag_idx, frag) in frags.iter().enumerate() {
                        if frag.hl_opacity > 0.001 && frag_idx < ranges.len() {
                            let range = &ranges[frag_idx];
                            let mut min_x = f64::INFINITY;
                            let mut max_x = f64::NEG_INFINITY;
                            let mut min_y = f64::INFINITY;
                            let mut max_y = f64::NEG_INFINITY;
                            for tp in &all_glyphs[range.start..range.end] {
                                use kurbo::Shape;
                                let b = tp.path.bounding_box();
                                min_x = min_x.min(b.x0);
                                max_x = max_x.max(b.x1);
                                min_y = min_y.min(b.y0);
                                max_y = max_y.max(b.y1);
                            }
                            if min_x.is_finite() && max_x.is_finite() {
                                let pad = frag.hl_padding as f64;
                                let hl_rect = kurbo::Rect::new(
                                    min_x - pad,
                                    min_y - pad,
                                    max_x + pad,
                                    max_y + pad,
                                );
                                let hl_color = vello::peniko::Color::from_rgba8(
                                    (frag.hl_color[0] * 255.0) as u8,
                                    (frag.hl_color[1] * 255.0) as u8,
                                    (frag.hl_color[2] * 255.0) as u8,
                                    255,
                                );
                                highlight_cmds.push(
                                    crate::primitives::RenderCommand::HighlightLayer {
                                        rect: hl_rect,
                                        color: hl_color,
                                        blend: frag.hl_blend,
                                        alpha: frag.hl_opacity,
                                        corner_radius: frag.hl_radius as f64,
                                    },
                                );
                            }
                        }
                    }

                    // Render all glyphs as a single text command. The memo
                    // owns the glyph slice, so share it by refcount instead
                    // of re-wrapping a freshly built Vec.
                    if !all_glyphs.is_empty() {
                        let cmd = crate::primitives::RenderCommand::Text {
                            paths: std::sync::Arc::clone(&compiled.glyphs),
                            fill_gradient: None,
                            clip_path: None,
                            tint_color: None,
                            hardware_layout: None,
                        };
                        cmd.execute(out.scene, &global_transform, global_opacity);
                    }

                    // Render highlight overlays.
                    for cmd in &highlight_cmds {
                        cmd.execute(out.scene, &global_transform, global_opacity);
                    }
                },
                Err(e) => {
                    tracing::warn!("Equation Typst compilation failed: {e}");
                },
            }
        }

        // Render Fragment children. They return `None` from evaluate (no
        // visual output of their own), so they draw nothing here and
        // record no hit region; the parent Equation aggregates their
        // content into one document.
        for &child_id in &track.children_ids {
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

    /// Apply the Equation strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_equation_children_ctx(
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
        self.render_equation_children(
            ctx.host_id,
            ctx.node_label,
            ctx.global_transform,
            ctx.global_opacity,
            ctx.allow_pending_composites,
            &frame,
            &mut out,
        );
    }

    /// Apply the Captions strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_captions_children_ctx(
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
        self.render_captions_children(
            ctx.host_id,
            ctx.node_label,
            ctx.global_transform,
            ctx.global_opacity,
            ctx.allow_pending_composites,
            &frame,
            &mut out,
        );
    }

    fn render_captions_children(
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
        let Some(track) = self.tracks.get_by_id(node_id).or_else(|| self.tracks.get(node_label))
        else {
            return;
        };

        let child_layout_positions = if self.dynamic_layout {
            self.compute_animated_layout(node_label, time_ms)
        } else {
            std::sync::Arc::new(crate::timeline::layout::LayoutPositions::new())
        };

        struct ActiveCue {
            text: String,
            start_s: f64,
            end_s: f64,
            words: Vec<animatix_text::transcript::TranscriptWord>,
            active_color: Option<[f32; 4]>,
        }

        let mut all_cues: Vec<ActiveCue> = Vec::new();
        // Declared caption caps, read from the container. `text_max_width` is
        // the canonical wrap width (so a long cue wraps instead of running off
        // screen), `max_lines` caps how many wrapped lines the cue may occupy
        // (`0` = unlimited), and `transition_time` fades a cue in from its own
        // start (0 by default, so captions snap unless asked to transition).
        let text_max_width = track.sample_f32_prop("text_max_width", time_ms, 0.0).max(0.0);
        let max_lines = track.sample_u32_prop("max_lines", time_ms, 2);
        let transition_s = track.sample_f32_prop("transition_time", time_ms, 0.0).max(0.0) as f64;
        let child_tracks: Vec<&AnimationTrack> =
            track.children_ids.iter().filter_map(|id| self.tracks.get_by_id(*id)).collect();

        for child_track in child_tracks {
            let Some(primitive) = self.track_primitive_at(child_track, time_ms) else {
                continue;
            };
            let child_vector_paths = child_track.evaluate_vector_paths(time_ms);
            let ctx = crate::primitives::EvaluateCtx {
                track: child_track,
                time_ms,
                local_transform: kurbo::Affine::IDENTITY,
                opacity: 1.0,
                scene_dimensions,
                background_color: self.eval_caches.background_color.get(),
                overrides: overrides.get(&child_track.label),
                vector_paths: &child_vector_paths,
                asset_cache: &self.asset_cache,
                target_resolver: Some(self),
            };
            if let Some(cue) = primitive.caption_cue(&ctx) {
                let start_s = cue.start_ms as f64 / 1000.0;
                let end_s = cue.end_ms as f64 / 1000.0;
                let words_vec: Vec<&str> = cue.text.split_whitespace().collect();
                let word_count = words_vec.len().max(1);
                let dur_per_word = (end_s - start_s).max(0.0) / word_count as f64;
                let mut words = Vec::with_capacity(word_count);
                for (i, w) in words_vec.iter().enumerate() {
                    let w_start = start_s + i as f64 * dur_per_word;
                    let w_end = if i + 1 == word_count {
                        end_s
                    } else {
                        w_start + dur_per_word
                    };
                    words.push(animatix_text::transcript::TranscriptWord {
                        word: w.to_string(),
                        start_s: w_start,
                        end_s: w_end,
                        confidence: 1.0,
                    });
                }
                all_cues.push(ActiveCue {
                    text: cue.text,
                    start_s,
                    end_s,
                    words,
                    active_color: cue.active_color,
                });
            }
        }

        if all_cues.is_empty() {
            let transcript = self.transcripts.get(node_label).or_else(|| {
                let source = track.sample_string_prop("source", time_ms, "");
                if !source.is_empty() {
                    self.transcripts.get(&source)
                } else {
                    None
                }
            });

            if let Some(t) = transcript {
                for seg in &t.segments {
                    all_cues.push(ActiveCue {
                        text: seg.text.clone(),
                        start_s: seg.start_s,
                        end_s: seg.end_s,
                        words: seg.words.clone(),
                        active_color: None,
                    });
                }
            }
        }

        // Render children so state is evaluated
        for &child_id in &track.children_ids {
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

        let time_s = time_ms as f64 / 1000.0;
        let active_cue = all_cues.iter().find(|c| c.start_s <= time_s && time_s < c.end_s);

        if let Some(cue) = active_cue {
            // `transition_time` fades a cue in from its own start. It is 0 by
            // default, so captions snap unless an author asks for a transition.
            let caption_opacity = global_opacity
                * crate::primitives::captions::cue_transition_alpha(
                    time_s,
                    cue.start_s,
                    transition_s,
                );
            let font_size = track.text.font_size.get(time_ms, 32.0);
            let font_family = track.text.font_family.get(time_ms, String::new());
            let font_style = track.text.font_style.get(time_ms, "normal".to_string());
            let font_weight = track.text.font_weight.get(time_ms, 400.0);
            let line_height = track.text.line_height.get(time_ms, 1.2);
            let letter_spacing = track.text.letter_spacing.get(time_ms, 0.0);
            let word_spacing = track.text.word_spacing.get(time_ms, 0.0);
            let base_color = track.style.color.get(time_ms, [1.0, 1.0, 1.0, 1.0]);
            let active_color = cue.active_color.unwrap_or_else(|| {
                track.sample_color_prop("active_color", time_ms, [1.0, 0.84, 0.0, 1.0])
            });
            let inactive_color =
                track.sample_color_prop("inactive_color", time_ms, [0.7, 0.7, 0.7, 0.7]);
            let caption_mode = track.sample_string_prop("caption_mode", time_ms, "word");
            let backdrop = track.sample_bool_prop("backdrop", time_ms, true);
            let backdrop_color =
                track.sample_color_prop("backdrop_color", time_ms, [0.0, 0.0, 0.0, 0.6]);
            let backdrop_radius = track.sample_f32_prop("backdrop_radius", time_ms, 8.0);
            let backdrop_padding = track.sample_vec2_prop("backdrop_padding", time_ms, [16.0, 8.0]);
            let pop_scale = track.sample_f32_prop("pop_scale", time_ms, 1.25);

            let active_word_idx =
                cue.words.iter().position(|w| w.start_s <= time_s && time_s < w.end_s);

            if let Ok(compiled) = crate::renderer::text::compile_text_cached(
                crate::renderer::text::TextKind::Text,
                &cue.text,
                "",
                None,
                &font_family,
                font_size,
                font_weight,
                &font_style,
                line_height,
                letter_spacing,
                word_spacing,
                base_color,
                text_max_width,
                (max_lines > 0).then_some(max_lines as usize),
                "center",
                "visible",
                true,
                self.font_context.as_ref(),
            ) {
                use kurbo::Shape as _;
                let mut bounds: Option<kurbo::Rect> = None;
                for g in compiled.paths.iter() {
                    let b = g.path.bounding_box();
                    bounds = Some(match bounds {
                        Some(prev) => prev.union(b),
                        None => b,
                    });
                }

                if let Some(b) = bounds {
                    if backdrop {
                        let pad_x = backdrop_padding[0] as f64;
                        let pad_y = backdrop_padding[1] as f64;
                        let bg_rect = kurbo::RoundedRect::new(
                            b.x0 - pad_x,
                            b.y0 - pad_y,
                            b.x1 + pad_x,
                            b.y1 + pad_y,
                            backdrop_radius as f64,
                        );
                        let bg_alpha = backdrop_color[3] * caption_opacity;
                        let bg_col = vello::peniko::Color::from_rgba8(
                            (backdrop_color[0] * 255.0).clamp(0.0, 255.0) as u8,
                            (backdrop_color[1] * 255.0).clamp(0.0, 255.0) as u8,
                            (backdrop_color[2] * 255.0).clamp(0.0, 255.0) as u8,
                            (bg_alpha * 255.0).clamp(0.0, 255.0) as u8,
                        );
                        out.scene.fill(
                            vello::peniko::Fill::NonZero,
                            global_transform,
                            bg_col,
                            None,
                            &bg_rect,
                        );
                    }

                    for g in compiled.paths.iter() {
                        let word_idx = g.word_idx as usize;
                        let is_current = active_word_idx == Some(word_idx);
                        let has_passed = active_word_idx.map(|idx| word_idx < idx).unwrap_or(false);

                        let (color, scale) = match caption_mode.as_str() {
                            "karaoke" => {
                                if is_current || has_passed {
                                    (active_color, 1.0f32)
                                } else {
                                    (inactive_color, 1.0f32)
                                }
                            },
                            "pop" => {
                                if is_current {
                                    (active_color, pop_scale)
                                } else {
                                    (inactive_color, 1.0f32)
                                }
                            },
                            "line" => (base_color, 1.0f32),
                            _ => {
                                if is_current {
                                    (active_color, 1.0f32)
                                } else {
                                    (inactive_color, 1.0f32)
                                }
                            },
                        };

                        let alpha = color[3] * caption_opacity * g.opacity;
                        if alpha <= 0.0 {
                            continue;
                        }
                        let peniko_color = vello::peniko::Color::from_rgba8(
                            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
                            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
                            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
                            (alpha * 255.0).clamp(0.0, 255.0) as u8,
                        );

                        let glyph_transform = if (scale - 1.0).abs() > 0.001 {
                            let cx = g.glyph_center[0] as f64;
                            let cy = g.glyph_center[1] as f64;
                            global_transform
                                * kurbo::Affine::translate((cx, cy))
                                * kurbo::Affine::scale(scale as f64)
                                * kurbo::Affine::translate((-cx, -cy))
                        } else {
                            global_transform
                        };

                        out.scene.fill(
                            vello::peniko::Fill::NonZero,
                            glyph_transform,
                            peniko_color,
                            None,
                            &g.path,
                        );
                    }
                }
            }
        }
    }
}
