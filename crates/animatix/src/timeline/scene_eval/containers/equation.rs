use crate::renderer::types::TextPath;
use crate::timeline::{AnimationTrack, DEFAULT_WHITE, EntityId, Timeline, TrackAccessor};

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

        let child_tracks: Vec<&AnimationTrack> = self
            .world
            .hierarchy(node_id)
            .children
            .iter()
            .filter_map(|id| self.world.tracks.get_by_id(*id))
            .collect();

        for child_track in child_tracks {
            // A fragment is any child whose primitive opts into
            // `equation_fragment` — never a hard-coded type name.
            let Some(primitive) = self.track_primitive_at(child_track, time_ms) else {
                continue;
            };
            let child_vector_paths =
                child_track.evaluate_vector_paths(self.world.shape(child_track.id), time_ms);
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
                legend: self.world.legend(child_track.id),
                world: Some(&self.world),
                highlight: self.world.highlight(child_track.id),
                shape: Some(self.world.shape(child_track.id)),
                property_plan: Some(self.world.property_plan(child_track.id)),
                caps: self.world.caps(child_track.id),
                payload: self.world.payload(child_track.id),
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
}
