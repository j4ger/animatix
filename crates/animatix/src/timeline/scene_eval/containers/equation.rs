use crate::renderer::types::TextPath;
use crate::timeline::{DEFAULT_WHITE, EntityId, Timeline, TrackAccessor};

mod animate;
mod source;

/// A Fragment child's content and highlight state, collected for one Equation.
struct FragInfo {
    id: EntityId,
    order: f32,
    paint: Option<[u8; 4]>,
    content: String,
    hl_color: [f32; 4],
    hl_opacity: f32,
    hl_padding: f32,
    hl_radius: f32,
    hl_blend: peniko::Mix,
}

impl Timeline {
    /// Check the authored equation states with the same compiler used for rendering.
    pub(crate) fn validate_equations(&self, diagnostics: &mut Vec<crate::diagnostics::Diagnostic>) {
        use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};

        let overrides = std::collections::HashMap::new();
        let scene_dimensions =
            self.configured_canvas_size().map_or_else(Default::default, |(width, height)| {
                crate::timeline::SceneDimensions { width, height }
            });
        for (label, id) in self.world.tracks.iter() {
            if self.world.actor_type(id) != "Equation" {
                continue;
            }
            let mut times = std::collections::BTreeSet::from([self.world.first_seen(id)]);
            for &child in &self.world.hierarchy(id).children {
                times.insert(self.world.first_seen(child));
                if let Some(track) =
                    self.world.text(child).and_then(|text| text.text_content.as_ref())
                {
                    times.extend(track.keyframes().keys().copied());
                }
            }
            if let Some(key) = crate::timeline::property_id("equation_source") {
                times.extend(self.world.property_plan(id).keyframe_times(key));
            }
            for time_ms in times {
                let frame = crate::primitives::RenderFrame {
                    paint_omissions: &[],
                    time_ms,
                    scene_dimensions,
                    debug_options: Default::default(),
                    overrides: &overrides,
                    frame_env: None,
                };
                let frags = self.collect_equation_fragments(id, &frame);
                let result = if self.has_equation_source(id) {
                    if frags.is_empty() {
                        self.compile_source_equation(
                            id,
                            time_ms,
                            &self.equation_source(id, time_ms),
                        )
                        .map(|_| ())
                    } else {
                        Err(crate::renderer::error::RenderError::TextCompilation(
                            "equation_source and Fragment children cannot be used on the same Equation".into()
                        ))
                    }
                } else if !frags.is_empty() {
                    self.compile_equation(id, time_ms, &frags).map(|_| ())
                } else {
                    Ok(())
                };
                if let Err(error) = result {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!(
                                "Equation '{label}' at {:.3}s: {error}",
                                time_ms as f64 / 1000.0
                            ),
                        )
                        .with_subject(label)
                        .with_ast_span(self.world.decl_span(id).cloned()),
                    );
                }
            }
        }
    }

    /// Equation strategy: fragment children aggregate into one Typst document.
    /// Selected by `ChildProcessingKind::Equation` in the scene evaluator.
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
        // Collect Fragment children with their content and highlight state.
        let frags = self.collect_equation_fragments(node_id, frame);
        if self.has_equation_source(node_id) {
            self.render_source_equation(
                node_id,
                time_ms,
                global_transform,
                global_opacity,
                out.scene,
            );
        } else if !self.render_equation_transition(
            node_id,
            &frags,
            global_transform,
            global_opacity,
            frame,
            out,
        ) {
            self.render_equation_document(
                node_id,
                time_ms,
                &frags,
                global_transform,
                global_opacity,
                out,
            );
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

    /// Collect the scope's Fragment children through `equation_fragment`.
    fn collect_equation_fragments(
        &self,
        node_id: EntityId,
        frame: &crate::primitives::RenderFrame<'_>,
    ) -> Vec<FragInfo> {
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let overrides = frame.overrides;
        let mut frags: Vec<FragInfo> = Vec::new();

        let child_ids: Vec<EntityId> = self.world.hierarchy(node_id).children.to_vec();

        for &child_id in &child_ids {
            if self.world.first_seen(child_id) > time_ms {
                continue;
            }
            // A fragment is any child whose behavior opts into
            // `equation_fragment` — never a hard-coded type name.
            let Some(def) = self.behavior_for_node(child_id, time_ms) else {
                continue;
            };
            let ctx = crate::primitives::EvaluateCtx {
                id: child_id,
                label: self.world.label(child_id).unwrap_or(""),
                time_ms,
                time_s: time_ms as f64 / 1000.0,
                local_transform: kurbo::Affine::IDENTITY,
                opacity: 1.0,
                scene_dimensions,
                background_color: self.eval_caches.background_color.get(),
                overrides: overrides.get(self.world.label(child_id).unwrap_or("")),
                vector_paths: &[],
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
                payload: None,
            };
            if let Some(fragment) = def.equation_fragment(&ctx) {
                if fragment.content.trim().is_empty() {
                    continue;
                }
                let mut order = 0.0;
                let mut paint = None;
                if !ctx.property_plan.is_empty() {
                    order = ctx.property_plan.sample_f32_prop("fragment_order", time_ms, 0.0);
                    if let Some(crate::timeline::PropertyValue::Color(color)) =
                        ctx.property_plan.sample_property_named("fragment_color", time_ms)
                    {
                        paint = Some(animatix_text::f32_color_to_rgba8(&color));
                    }
                }
                if let Some(color) = ctx.overrides.and_then(|values| {
                    crate::primitives::styling::override_color(values, "fragment_color")
                }) {
                    paint = Some(animatix_text::f32_color_to_rgba8(&color));
                }
                frags.push(FragInfo {
                    id: child_id,
                    order,
                    paint,
                    content: fragment.content,
                    hl_color: fragment.highlight_color,
                    hl_opacity: fragment.highlight_opacity,
                    hl_padding: fragment.highlight_padding,
                    hl_radius: fragment.highlight_radius,
                    hl_blend: fragment.highlight_blend,
                });
            }
        }

        if frags.iter().any(|fragment| fragment.order != 0.0) {
            frags.sort_by(|a, b| a.order.total_cmp(&b.order));
        }
        frags
    }

    fn compile_equation(
        &self,
        node_id: EntityId,
        time_ms: u64,
        frags: &[FragInfo],
    ) -> Result<std::sync::Arc<animatix_text::CachedGroupedText>, crate::renderer::error::RenderError>
    {
        // Keep fragment identity explicit in the frame stream. Typst adds
        // transparent layout groups around the whole paragraph, so group depth
        // alone cannot reliably recover the authored Fragment boundaries.
        let fragments = frags
            .iter()
            .enumerate()
            .map(|(index, f)| {
                let source = f.content.trim();
                let math = source.strip_prefix('$').and_then(|s| s.strip_suffix('$')).unwrap_or(source);
                // Alphanumeric labels such as a1 predate mathematical layout.
                let literal_label = math.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                    && math.as_bytes().last().is_some_and(u8::is_ascii_digit)
                    && math.bytes().all(|byte| byte.is_ascii_alphanumeric());
                let math = if literal_label {
                    std::borrow::Cow::Owned(format!("#text(\"{math}\")"))
                } else {
                    std::borrow::Cow::Borrowed(math)
                };
                format!(
                    "#metadata(\"animatix:equation-fragment:start:{index}\") {math} #metadata(\"animatix:equation-fragment:end:{index}\")",
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let typst_body = format!("$ {fragments} $");

        self.compile_equation_markup(node_id, time_ms, &typst_body)
    }

    fn compile_equation_markup(
        &self,
        node_id: EntityId,
        time_ms: u64,
        typst_body: &str,
    ) -> Result<std::sync::Arc<animatix_text::CachedGroupedText>, crate::renderer::error::RenderError>
    {
        // Use equation-level font_size and color from the Equation track.
        let text = self.world.text_or_default(node_id);
        let font_size = text.font_size.get(time_ms, 48.0);
        let eq_color = self.world.style(node_id).color.get(time_ms, DEFAULT_WHITE);
        let font_family = text.font_family.get(time_ms, String::new());
        let font_weight = text.font_weight.get(time_ms, 400.0);
        let font_style = text.font_style.get(time_ms, "normal".to_string());
        let line_height = text.line_height.get(time_ms, 1.2);
        let letter_spacing = text.letter_spacing.get(time_ms, 0.0);
        let word_spacing = text.word_spacing.get(time_ms, 0.0);

        // Compile the Typst markup. Memoized process-wide: fragment
        // content and font properties rarely change between frames, so
        // scrubbing an equation-heavy out.scene reuses one compilation
        // instead of re-running Typst every frame.
        crate::renderer::text::compile_typst_grouped_cached(
            typst_body,
            font_size,
            eq_color,
            &font_family,
            self.font_context.as_ref(),
            font_weight,
            &font_style,
            line_height,
            letter_spacing,
            word_spacing,
        )
    }

    /// Compile and draw the aggregated Typst document for `frags`.
    fn render_equation_document(
        &self,
        node_id: EntityId,
        time_ms: u64,
        frags: &[FragInfo],
        global_transform: kurbo::Affine,
        global_opacity: f32,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        if frags.is_empty() {
            return;
        }
        match self.compile_equation(node_id, time_ms, frags) {
            Ok(compiled) => {
                // One glyph range per authored Fragment, resolved at compile
                // time and shared through the memo.
                let all_glyphs: &[TextPath] = &compiled.glyphs;
                let ranges: &[std::ops::Range<usize>] = &compiled.ranges;

                // Compute highlight bounding boxes from the shared
                // glyph groups before drawing them.
                let highlight_cmds = equation_highlight_commands(frags, all_glyphs, ranges);
                draw_highlights(
                    &highlight_cmds,
                    true,
                    out.scene,
                    &global_transform,
                    global_opacity,
                );

                // Render all glyphs as a single text command. The memo
                // owns the glyph slice, so share it by refcount instead
                // of re-wrapping a freshly built Vec.
                if !all_glyphs.is_empty() {
                    let paths = if frags.iter().any(|frag| frag.paint.is_some()) {
                        let mut paths = all_glyphs.to_vec();
                        for (frag, range) in frags.iter().zip(ranges) {
                            if let Some(color) = frag.paint {
                                for path in &mut paths[range.clone()] {
                                    path.color = color;
                                }
                            }
                        }
                        paths.into()
                    } else {
                        std::sync::Arc::clone(&compiled.glyphs)
                    };
                    let cmd = crate::primitives::RenderCommand::Text {
                        paths,
                        fill_gradient: None,
                        clip_path: None,
                        tint_color: None,
                        hardware_layout: None,
                    };
                    cmd.execute(out.scene, &global_transform, global_opacity);
                }

                // Render highlight overlays.
                draw_highlights(
                    &highlight_cmds,
                    false,
                    out.scene,
                    &global_transform,
                    global_opacity,
                );
            },
            Err(e) => {
                use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
                self.eval_caches.runtime_diagnostics.borrow_mut().push(
                    Diagnostic::error(
                        DiagnosticCode::RenderFailure,
                        DiagnosticPhase::Render,
                        format!("Equation failed to compile: {e}"),
                    )
                    .with_subject(self.world.label(node_id).unwrap_or("Equation"))
                    .with_ast_span(self.world.decl_span(node_id).cloned()),
                );
            },
        }
    }

    /// Apply the Equation strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_equation_children_ctx(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
    ) {
        let host_id = ctx.host_id;
        let node_label = ctx.node_label;
        let global_transform = ctx.global_transform;
        let global_opacity = ctx.global_opacity;
        let allow_pending_composites = ctx.allow_pending_composites;
        ctx.with_frame_outputs(|timeline, frame, out| {
            timeline.render_equation_children(
                host_id,
                node_label,
                global_transform,
                global_opacity,
                allow_pending_composites,
                frame,
                out,
            );
        });
    }
}

fn draw_highlights(
    commands: &[crate::primitives::RenderCommand],
    behind_text: bool,
    scene: &mut crate::scene::Scene,
    transform: &kurbo::Affine,
    opacity: f32,
) {
    for command in commands {
        if let crate::primitives::RenderCommand::HighlightLayer { blend, .. } = command
            && (*blend == peniko::Mix::Normal) == behind_text
        {
            command.execute(scene, transform, opacity);
        }
    }
}

/// Highlight-layer commands for the fragments that declare one, clipped to the
/// glyph runs the grouped compile resolved per authored fragment.
fn equation_highlight_commands(
    frags: &[FragInfo],
    all_glyphs: &[TextPath],
    ranges: &[std::ops::Range<usize>],
) -> Vec<crate::primitives::RenderCommand> {
    frags
        .iter()
        .zip(ranges)
        .filter_map(|(fragment, range)| {
            equation_highlight(fragment, all_glyphs.get(range.clone())?, 1.0)
        })
        .collect()
}

fn equation_highlight(
    fragment: &FragInfo,
    paths: &[TextPath],
    opacity: f32,
) -> Option<crate::primitives::RenderCommand> {
    let alpha = fragment.hl_opacity * opacity;
    if alpha <= 0.001 {
        return None;
    }
    let bounds = paths.iter().map(|path| path.bounds).reduce(|a, b| a.union(b))?;
    let padding = f64::from(fragment.hl_padding.max(0.0));
    let [r, g, b, a] = animatix_text::f32_color_to_rgba8(&fragment.hl_color);
    Some(crate::primitives::RenderCommand::HighlightLayer {
        rect: bounds.inflate(padding, padding),
        color: peniko::Color::from_rgba8(r, g, b, a),
        blend: fragment.hl_blend,
        alpha,
        corner_radius: f64::from(fragment.hl_radius.max(0.0)),
    })
}
