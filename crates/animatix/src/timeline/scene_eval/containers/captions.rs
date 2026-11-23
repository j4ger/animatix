use crate::renderer::text::{TextCompileRequest, TextStyle};
use crate::timeline::plan::PropertyPlan;
use crate::timeline::{EntityId, Timeline, TrackAccessor};
use std::ops::Range;

mod sidecar;
mod styling;
mod word_ranges;
use word_ranges::{
    CaptionWordStyle, map_timed_words_to_char_ranges, style_for_glyph, word_index_for_char,
};

/// One subtitle cue active or transcript-derived for a Captions container.
struct ActiveCue {
    text: String,
    start_s: f64,
    end_s: f64,
    words: Vec<animatix_text::transcript::TranscriptWord>,
    speech_start_s: f64,
    real_word_timings: bool,
    word_char_ranges: Vec<Option<Range<usize>>>,
    active_color: Option<[f32; 4]>,
}

impl Timeline {
    /// Apply the Captions strategy from a `RenderChildrenCtx`.
    pub(crate) fn render_captions_children_ctx(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
    ) {
        let host_id = ctx.host_id;
        let node_label = ctx.node_label;
        let global_transform = ctx.global_transform;
        let global_opacity = ctx.global_opacity;
        let allow_pending_composites = ctx.allow_pending_composites;
        ctx.with_frame_outputs(|timeline, frame, out| {
            timeline.render_captions_children(
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

        // Declared caption caps: `text_max_width` is the container's TextTracks
        // wrap lane (not Legend's tagged slot); `max_lines` 0 means unlimited;
        // `transition_time` fades from cue start (0 snaps).
        let property_plan = self.world.property_plan(node_id);
        let text_max_width =
            self.world.text_or_default(node_id).text_max_width.get(time_ms, 0.0).max(0.0);
        let max_lines = property_plan.sample_u32_prop("max_lines", time_ms, 2);

        let all_cues = self.collect_caption_cues(node_id, node_label, frame);

        // Render children so state is evaluated
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

        let time_s = time_ms as f64 / 1000.0;
        let active_cue = all_cues
            .iter()
            .filter(|c| c.start_s <= time_s && time_s < c.end_s)
            .max_by(|a, b| a.start_s.total_cmp(&b.start_s));

        if let Some(cue) = active_cue {
            let paint_overrides = frame.overrides.get(node_label);
            let pause = match paint_overrides.and_then(|values| values.get("caption_pause")) {
                Some(crate::timeline::Value::Num(value)) => *value as f32,
                _ => property_plan.sample_f32_prop("caption_pause", time_ms, 0.0),
            };
            let phrase = styling::cue_phrase(cue, f64::from(pause), time_s);
            self.render_active_caption(
                node_id,
                phrase.as_ref().unwrap_or(cue),
                time_s,
                time_ms,
                global_transform,
                global_opacity,
                text_max_width,
                max_lines,
                property_plan,
                paint_overrides,
                out,
            );
        }
    }

    /// Gather the cues this container is showing: child `caption_cue`s, or the
    /// transcript's segments when no child contributes one.
    fn collect_caption_cues(
        &self,
        node_id: EntityId,
        node_label: &str,
        frame: &crate::primitives::RenderFrame<'_>,
    ) -> Vec<ActiveCue> {
        let time_ms = frame.time_ms;
        let scene_dimensions = frame.scene_dimensions;
        let overrides = frame.overrides;

        let mut all_cues: Vec<ActiveCue> = Vec::new();
        let child_ids: Vec<EntityId> = self.world.hierarchy(node_id).children.to_vec();

        for &child_id in &child_ids {
            if self.world.first_seen(child_id) > time_ms {
                continue;
            }
            let Some(def) = self.behavior_for_node(child_id, time_ms) else {
                continue;
            };
            let child_label = self.world.label(child_id).unwrap_or("");
            let ctx = crate::primitives::EvaluateCtx {
                id: child_id,
                label: child_label,
                time_ms,
                local_transform: kurbo::Affine::IDENTITY,
                opacity: 1.0,
                scene_dimensions,
                background_color: self.eval_caches.background_color.get(),
                overrides: overrides.get(child_label),
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
            if let Some(cue) = def.caption_cue(&ctx) {
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
                let word_char_ranges =
                    map_timed_words_to_char_ranges(&cue.text, &words, child_label);
                all_cues.push(ActiveCue {
                    text: cue.text,
                    start_s,
                    end_s,
                    words,
                    speech_start_s: 0.0,
                    real_word_timings: false,
                    word_char_ranges,
                    active_color: cue.active_color,
                });
            }
        }

        if all_cues.is_empty()
            && let Some((transcript, start, limit)) = self.caption_transcript(node_id, time_ms)
        {
            for seg in &transcript.segments {
                let start_s = start + seg.start_s;
                let end_s = (start + seg.end_s).min(limit);
                if end_s <= start_s {
                    continue;
                }
                all_cues.push(ActiveCue {
                    text: seg.text.clone(),
                    start_s,
                    end_s,
                    words: seg.words.clone(),
                    speech_start_s: start,
                    real_word_timings: !seg.words.is_empty(),
                    word_char_ranges: map_timed_words_to_char_ranges(
                        &seg.text, &seg.words, node_label,
                    ),
                    active_color: None,
                });
            }
        }

        all_cues
    }

    /// Draw the cue spanning `time_s`: its backdrop and its styled glyphs.
    fn render_active_caption(
        &self,
        node_id: EntityId,
        cue: &ActiveCue,
        time_s: f64,
        time_ms: u64,
        global_transform: kurbo::Affine,
        global_opacity: f32,
        text_max_width: f32,
        max_lines: u32,
        property_plan: &PropertyPlan,
        paint_overrides: Option<&std::collections::HashMap<String, crate::timeline::Value>>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let text = self.world.text_or_default(node_id);
        let font_size = text.font_size.get(time_ms, crate::primitives::captions::DEFAULT_FONT_SIZE);
        let font_family = text.font_family.get(time_ms, String::new());
        let font_style = text.font_style.get(time_ms, "normal".to_string());
        let font_weight = text.font_weight.get(time_ms, 400.0);
        let line_height = text.line_height.get(time_ms, 1.2);
        let letter_spacing = text.letter_spacing.get(time_ms, 0.0);
        let word_spacing = text.word_spacing.get(time_ms, 0.0);
        let backdrop = property_plan.sample_bool_prop("backdrop", time_ms, true);
        let default_color = if backdrop {
            [1.0; 4]
        } else {
            self.colorscheme.color("text.primary").unwrap_or([1.0; 4])
        };
        let base_color = self.world.style(node_id).color.get(time_ms, default_color);
        let active_color = cue.active_color.unwrap_or_else(|| {
            property_plan.sample_color_prop("active_color", time_ms, [1.0, 0.84, 0.0, 1.0])
        });
        let inactive_color =
            property_plan.sample_color_prop("inactive_color", time_ms, [0.7, 0.7, 0.7, 0.7]);
        let caption_mode = property_plan.sample_string_prop("caption_mode", time_ms, "word");
        let backdrop_color =
            property_plan.sample_color_prop("backdrop_color", time_ms, [0.0, 0.0, 0.0, 0.6]);
        let backdrop_radius = property_plan.sample_f32_prop("backdrop_radius", time_ms, 8.0);
        let backdrop_padding =
            property_plan.sample_vec2_prop("backdrop_padding", time_ms, [16.0, 8.0]);
        let pop_scale = property_plan.sample_f32_prop("pop_scale", time_ms, 1.25);
        let word_style = CaptionWordStyle {
            caption_mode: &caption_mode,
            active_color,
            inactive_color,
            base_color,
            pop_scale,
        };

        let speech_time_s = time_s - cue.speech_start_s;
        let active_word_idx = cue
            .words
            .iter()
            .position(|w| w.start_s <= speech_time_s && speech_time_s < w.end_s);
        let latest_started = cue.words.iter().rposition(|word| word.start_s <= speech_time_s);

        let request = TextCompileRequest {
            max_width: text_max_width,
            text_align: "center",
            allow_fast_path: true,
            ..TextCompileRequest::new(
                crate::renderer::text::TextKind::Text,
                &cue.text,
                TextStyle {
                    font_weight,
                    font_style: &font_style,
                    line_height,
                    letter_spacing,
                    word_spacing,
                    color: base_color,
                    ..TextStyle::new(&font_family, font_size)
                },
            )
        };
        let compiled = match crate::renderer::text::compile_text_cached(
            &request,
            self.font_context.as_ref(),
        ) {
            Ok(compiled) => compiled,
            Err(error) => {
                use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
                self.eval_caches.runtime_diagnostics.borrow_mut().push(
                    Diagnostic::error(
                        DiagnosticCode::RenderFailure,
                        DiagnosticPhase::Render,
                        format!("Caption failed to compile: {error}"),
                    )
                    .with_subject(self.world.label(node_id).unwrap_or("Captions"))
                    .with_ast_span(self.world.decl_span(node_id).cloned()),
                );
                return;
            },
        };
        let (page, start, end) = caption_page(&compiled.paths, cue, max_lines, time_s);
        let caption_opacity = global_opacity
            * styling::transition_alpha(
                cue,
                time_s,
                time_ms,
                (start, end),
                property_plan,
                paint_overrides,
            );
        if caption_opacity <= 0.0 {
            return;
        }
        let on_page =
            |g: &&crate::renderer::types::TextPath| page.contains(&usize::from(g.line_idx));
        let glyphs = compiled.paths.iter().filter(on_page);
        let bounds = glyphs.clone().map(|glyph| glyph.bounds).reduce(|a, b| a.union(b));
        let active_bounds = active_word_idx.and_then(|active| {
            glyphs
                .clone()
                .filter(|g| {
                    word_index_for_char(&cue.word_char_ranges, usize::from(g.char_idx))
                        == Some(active)
                })
                .map(|g| g.bounds)
                .reduce(|a, b| a.union(b))
        });

        if let Some(b) = bounds {
            let center = b.center();
            let global_transform =
                global_transform * kurbo::Affine::translate((-center.x, -center.y));
            let pop = active_bounds.map_or(kurbo::Affine::IDENTITY, |rect| {
                let center = rect.center();
                kurbo::Affine::translate((center.x, center.y))
                    * kurbo::Affine::scale(f64::from(pop_scale))
                    * kurbo::Affine::translate((-center.x, -center.y))
            });
            if backdrop {
                draw_caption_backdrop(
                    out,
                    global_transform,
                    b,
                    caption_opacity,
                    backdrop_padding,
                    backdrop_radius,
                    backdrop_color,
                );
            }
            let paint = styling::CaptionPaint::sample(property_plan, time_ms, paint_overrides);
            if let Some(rect) = active_bounds
                && paint.word_backdrop[3] > 0.0
            {
                let transform = if caption_mode == "pop" {
                    global_transform * pop
                } else {
                    global_transform
                };
                draw_caption_backdrop(
                    out,
                    transform,
                    rect,
                    caption_opacity,
                    [4.0, 2.0],
                    3.0,
                    paint.word_backdrop,
                );
            }

            for g in glyphs {
                let char_idx =
                    g.source_range.map_or(usize::from(g.char_idx), |range| range[0] as usize);
                let (color, scale) = if caption_mode == "karaoke_sweep" {
                    (sweep_color(cue, char_idx, speech_time_s, &word_style), 1.0)
                } else {
                    style_for_glyph(
                        &cue.word_char_ranges,
                        char_idx,
                        active_word_idx,
                        latest_started,
                        &word_style,
                    )
                };

                let alpha = color[3] * caption_opacity * g.opacity;
                if alpha <= 0.0 {
                    continue;
                }
                let peniko_color = peniko::Color::from_rgba8(
                    (color[0] * 255.0).clamp(0.0, 255.0) as u8,
                    (color[1] * 255.0).clamp(0.0, 255.0) as u8,
                    (color[2] * 255.0).clamp(0.0, 255.0) as u8,
                    (alpha * 255.0).clamp(0.0, 255.0) as u8,
                );

                let glyph_transform = if (scale - 1.0).abs() > 0.001 {
                    global_transform * pop
                } else {
                    global_transform
                };

                paint.draw(out.scene, g, glyph_transform, peniko_color, caption_opacity);
            }
        }
    }
}

fn caption_page(
    paths: &[crate::renderer::types::TextPath],
    cue: &ActiveCue,
    max_lines: u32,
    time_s: f64,
) -> (Range<usize>, f64, f64) {
    if max_lines == 0 {
        return (0..usize::MAX, cue.start_s, cue.end_s);
    }
    let page_size = max_lines as usize;
    let line_count = paths.iter().map(|g| usize::from(g.line_idx) + 1).max().unwrap_or(0);
    let page_count = line_count.div_ceil(page_size).max(1);
    if page_count == 1 {
        return (0..page_size, cue.start_s, cue.end_s);
    }
    let mut first_chars = vec![None; page_count];
    for path in paths {
        let page = usize::from(path.line_idx) / page_size;
        let index = path.source_range.map_or(usize::from(path.char_idx), |range| range[0] as usize);
        first_chars[page] = Some(first_chars[page].map_or(index, |first: usize| first.min(index)));
    }
    let mut page = 0;
    let mut previous_start = cue.start_s;
    let mut end = cue.end_s;
    for (index, first_char) in first_chars.into_iter().enumerate().skip(1) {
        let spoken_start = first_char.and_then(|first| {
            let word_idx = word_index_for_char(&cue.word_char_ranges, first)?;
            let word = cue.words.get(word_idx)?;
            let range = cue.word_char_ranges[word_idx].as_ref()?;
            let fraction = (first - range.start) as f64 / range.len().max(1) as f64;
            Some(cue.speech_start_s + word.start_s + fraction * (word.end_s - word.start_s))
        });
        let even_start = cue.start_s + (cue.end_s - cue.start_s) * index as f64 / page_count as f64;
        let start = spoken_start.unwrap_or(even_start).clamp(previous_start, cue.end_s);
        if start > time_s {
            end = start;
            break;
        }
        previous_start = start;
        page = index;
    }
    (page * page_size..(page + 1) * page_size, previous_start, end)
}

fn sweep_color(
    cue: &ActiveCue,
    char_idx: usize,
    time: f64,
    style: &CaptionWordStyle<'_>,
) -> [f32; 4] {
    if !cue.real_word_timings {
        return style.base_color;
    }
    let Some(word_idx) = word_index_for_char(&cue.word_char_ranges, char_idx) else {
        return style.base_color;
    };
    let word = &cue.words[word_idx];
    let range = cue.word_char_ranges[word_idx].as_ref().expect("matched word range");
    let duration = word.end_s - word.start_s;
    let progress = if duration > 0.0 {
        (((time - word.start_s) / duration) * range.len() as f64 - (char_idx - range.start) as f64)
            .clamp(0.0, 1.0) as f32
    } else if time >= word.start_s {
        1.0
    } else {
        0.0
    };
    std::array::from_fn(|i| {
        style.inactive_color[i] + (style.active_color[i] - style.inactive_color[i]) * progress
    })
}

/// Fill the rounded backdrop behind one caption cue's text bounds.
fn draw_caption_backdrop(
    out: &mut crate::primitives::RenderOutputs<'_, '_>,
    global_transform: kurbo::Affine,
    bounds: kurbo::Rect,
    caption_opacity: f32,
    backdrop_padding: [f32; 2],
    backdrop_radius: f32,
    backdrop_color: [f32; 4],
) {
    let pad_x = backdrop_padding[0] as f64;
    let pad_y = backdrop_padding[1] as f64;
    let bg_rect =
        crate::primitives::surface::rounded_rect(bounds.inflate(pad_x, pad_y), backdrop_radius);
    let bg_col = crate::primitives::surface::paint_color(backdrop_color, caption_opacity);
    out.scene.fill(peniko::Fill::NonZero, global_transform, bg_col, None, &bg_rect);
}

/// Resolve one caption glyph's fill and scale for the active mode.
fn caption_glyph_style(
    caption_mode: &str,
    is_current: bool,
    has_passed: bool,
    active_color: [f32; 4],
    inactive_color: [f32; 4],
    base_color: [f32; 4],
    pop_scale: f32,
) -> ([f32; 4], f32) {
    match caption_mode {
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
    }
}

#[cfg(test)]
mod sweep_tests {
    use super::*;

    #[test]
    fn sweep_uses_recorded_word_intervals_and_falls_back_to_cue_color() {
        let mut cue = ActiveCue {
            text: "Hi".into(),
            start_s: 1.0,
            end_s: 3.0,
            speech_start_s: 0.0,
            real_word_timings: true,
            words: vec![animatix_text::transcript::TranscriptWord {
                word: "Hi".into(),
                start_s: 1.2,
                end_s: 2.2,
                confidence: 1.0,
            }],
            word_char_ranges: vec![Some(0..2)],
            active_color: None,
        };
        let style = CaptionWordStyle {
            caption_mode: "karaoke_sweep",
            active_color: [1.0, 0.0, 0.0, 1.0],
            inactive_color: [0.0, 0.0, 0.0, 1.0],
            base_color: [0.0, 1.0, 0.0, 1.0],
            pop_scale: 1.0,
        };
        assert_eq!(sweep_color(&cue, 0, 1.45, &style)[0], 0.5);
        assert_eq!(sweep_color(&cue, 1, 1.45, &style)[0], 0.0);
        assert_eq!(sweep_color(&cue, 1, 2.2, &style), style.active_color);
        cue.real_word_timings = false;
        assert_eq!(sweep_color(&cue, 0, 1.45, &style), style.base_color);
    }
}
