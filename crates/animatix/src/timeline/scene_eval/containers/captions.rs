use crate::timeline::{AnimationTrack, EntityId, Timeline, TrackAccessor};

impl Timeline {
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
        let text_max_width = track
            .sample_f32_prop(self.world.property_plan(track.id), "text_max_width", time_ms, 0.0)
            .max(0.0);
        let max_lines =
            track.sample_u32_prop(self.world.property_plan(track.id), "max_lines", time_ms, 2);
        let transition_s = track
            .sample_f32_prop(self.world.property_plan(track.id), "transition_time", time_ms, 0.0)
            .max(0.0) as f64;
        let child_tracks: Vec<&AnimationTrack> = self
            .world
            .hierarchy(node_id)
            .children
            .iter()
            .filter_map(|id| self.world.tracks.get_by_id(*id))
            .collect();

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
                legend: self.world.legend(child_track.id),
                world: Some(&self.world),
                highlight: self.world.highlight(child_track.id),
                property_plan: Some(self.world.property_plan(child_track.id)),
                payload: self.world.payload(child_track.id),
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
            let transcript = self.transcript(node_label).or_else(|| {
                let source = track.sample_string_prop(
                    self.world.property_plan(track.id),
                    "source",
                    time_ms,
                    "",
                );
                if !source.is_empty() {
                    self.transcript(&source)
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
                track.sample_color_prop(
                    self.world.property_plan(track.id),
                    "active_color",
                    time_ms,
                    [1.0, 0.84, 0.0, 1.0],
                )
            });
            let inactive_color = track.sample_color_prop(
                self.world.property_plan(track.id),
                "inactive_color",
                time_ms,
                [0.7, 0.7, 0.7, 0.7],
            );
            let caption_mode = track.sample_string_prop(
                self.world.property_plan(track.id),
                "caption_mode",
                time_ms,
                "word",
            );
            let backdrop = track.sample_bool_prop(
                self.world.property_plan(track.id),
                "backdrop",
                time_ms,
                true,
            );
            let backdrop_color = track.sample_color_prop(
                self.world.property_plan(track.id),
                "backdrop_color",
                time_ms,
                [0.0, 0.0, 0.0, 0.6],
            );
            let backdrop_radius = track.sample_f32_prop(
                self.world.property_plan(track.id),
                "backdrop_radius",
                time_ms,
                8.0,
            );
            let backdrop_padding = track.sample_vec2_prop(
                self.world.property_plan(track.id),
                "backdrop_padding",
                time_ms,
                [16.0, 8.0],
            );
            let pop_scale = track.sample_f32_prop(
                self.world.property_plan(track.id),
                "pop_scale",
                time_ms,
                1.25,
            );

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
