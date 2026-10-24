use super::motion::timing_modifiers_without_keys;
use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{DEFAULT_WHITE, ModifierHost, Timeline, parse_timing_modifiers};

/// Ensure a filled shape without a visible authored stroke has an outline for
/// draw/reveal actions. The outline uses the actor's fill color so the default
/// declaration stays clean while the entrance effect remains visible.
fn ensure_reveal_stroke(
    style: &mut crate::timeline::animation_track::StyleTracks,
    caps: crate::timeline::ActorCaps,
    time_ms: u64,
) {
    let current_width = style.stroke_width.get(time_ms, caps.default_stroke_width());
    if current_width > 0.0 {
        return;
    }
    let color = style.color.get(time_ms, DEFAULT_WHITE);
    style.stroke_width.ensure(2.0).add_keyframe(time_ms, 2.0, Easing::Linear);
    style
        .stroke_color
        .ensure(DEFAULT_WHITE)
        .add_keyframe(time_ms, color, Easing::Linear);
}

/// Returns true when a primitive emits text glyph paths.
fn is_text_like(timeline: &Timeline, track: &crate::timeline::AnimationTrack) -> bool {
    timeline
        .primitive_registry
        .info_of(timeline.world.actor_type(track.id))
        .is_some_and(|info| info.capabilities.text_paths)
}

/// Draws in vector targets by animating stroke progress first, then revealing fill.
pub struct DrawIn;

/// Cumulative character progress after each whole word of `text`, as fractions
/// of its length. `"a bb ccc"` gives `[1/6, 3/6, 6/6]`.
fn word_progress_steps(text: &str) -> Vec<f32> {
    let total = text.chars().count().max(1);
    let mut out = Vec::new();
    let mut consumed = 0usize;
    for word in text.split_whitespace() {
        consumed += word.chars().count() + 1; // the separating space too
        out.push(((consumed.min(total)) as f32) / total as f32);
    }
    if let Some(last) = out.last_mut() {
        *last = 1.0;
    }
    out
}

/// Read the `by:` modifier, if the action carries one.
fn reveal_granularity(modifiers: &[crate::ast::Modifier]) -> Option<String> {
    modifiers
        .iter()
        .find(|m| m.name.as_deref() == Some("by"))
        .map(|m| match &m.value {
            crate::ast::Expr::Ident(name) | crate::ast::Expr::Str(name) => name.clone(),
            _ => String::new(),
        })
}

/// Read the `stagger:` modifier, if the action carries one.
fn reveal_stagger(modifiers: &[crate::ast::Modifier]) -> Option<f32> {
    modifiers
        .iter()
        .find(|m| m.name.as_deref() == Some("stagger"))
        .and_then(|m| match &m.value {
            crate::ast::Expr::Num(n) => Some(*n as f32),
            crate::ast::Expr::Ident(raw) => {
                crate::timeline::timing::parse_duration_literal(raw).map(|ms| ms as f32 / 1000.0)
            },
            _ => None,
        })
}

/// Read the `offset_y:` modifier, if the action carries one.
fn reveal_offset_y(modifiers: &[crate::ast::Modifier]) -> Option<f32> {
    modifiers
        .iter()
        .find(|m| m.name.as_deref() == Some("offset_y"))
        .and_then(|m| match &m.value {
            crate::ast::Expr::Num(n) => Some(*n as f32),
            _ => None,
        })
}

/// Read the `mask:` modifier, if the action carries one.
fn reveal_mask(modifiers: &[crate::ast::Modifier]) -> Option<bool> {
    modifiers
        .iter()
        .find(|m| m.name.as_deref() == Some("mask"))
        .map(|m| match &m.value {
            crate::ast::Expr::Bool(b) => *b,
            crate::ast::Expr::Ident(s) => s == "true" || s == "baseline",
            _ => false,
        })
}

fn split_reveal_params() -> Vec<ActionParam> {
    vec![
        ActionParam {
            name: "by".to_string(),
            description: "Reveal granularity for text targets: `by: char` (the default), `by: word`, or `by: line`.".to_string(),
            type_info: "char | word | line".to_string(),
        },
        ActionParam {
            name: "stagger".to_string(),
            description: "Stagger delay between units (time or fraction).".to_string(),
            type_info: "time | num".to_string(),
        },
        ActionParam {
            name: "offset_y".to_string(),
            description: "Vertical translation offset for split reveal.".to_string(),
            type_info: "num".to_string(),
        },
        ActionParam {
            name: "mask".to_string(),
            description: "Mask below resting baseline (`baseline` or `true`).".to_string(),
            type_info: "baseline | bool".to_string(),
        },
    ]
}

impl BuiltinAction for DrawIn {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "draw-in".to_string(),
            category: "Entrance".to_string(),
            description:
                "Draws in vector targets by animating stroke progress first, then revealing fill at the end."
                    .to_string(),
            params: vec![],
            modifiers: {
                let mut mods = base_timing_params();
                mods.extend(split_reveal_params());
                mods
            },
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let parsed = parse_timing_modifiers(
            &timing_modifiers_without_keys(
                &action.modifiers,
                &["by", "stagger", "offset_y", "mask"],
            ),
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        for target in &action.targets {
            if !super::ensure_vector_reveal_target(
                timeline,
                target,
                &action.verb,
                diagnostics,
                None,
            ) {
                continue;
            }

            let is_text = timeline
                .world
                .tracks
                .get(target)
                .is_some_and(|track| is_text_like(timeline, track));
            let caps = match timeline.world.tracks.get_id(target) {
                Some(id) => timeline.world.caps(id),
                None => continue,
            };

            let Some(id) = timeline.world.tracks.get_id(target) else {
                continue;
            };
            let style = timeline.world.style.get_mut(id);

            // Reveal pre-keyframe ("hidden by default") targets: draw-in
            // animates stroke_progress/fill_opacity/char_progress but never
            // opacity, so lift the seeded opacity 0 alongside the draw.
            let meta = timeline
                .world
                .meta
                .get_or_insert_with(id, crate::timeline::world::EntityMeta::default);
            let was_hidden = meta.hidden_by_default;
            super::lift_hidden_by_default(style, meta, t_start_ms, t_end_ms, easing);
            if !was_hidden {
                // An explicitly authored `opacity: 0` needs the same lift.
                super::reveal_authored_zero_opacity(style, t_start_ms, t_end_ms, easing);
            }

            if is_text {
                let text = timeline.world.text.get_mut(id);
                let by_opt = reveal_granularity(&action.modifiers);
                let stagger_opt = reveal_stagger(&action.modifiers);
                let offset_y_opt = reveal_offset_y(&action.modifiers);
                let mask_opt = reveal_mask(&action.modifiers);

                if let Some(by) = &by_opt {
                    text.split_by.ensure("char".to_string()).add_keyframe(
                        t_start_ms,
                        by.clone(),
                        Easing::Linear,
                    );
                }
                if let Some(stagger) = stagger_opt {
                    let s = if duration_ms > 0.0 && stagger > 0.0 {
                        (stagger * 1000.0 / duration_ms as f32).min(0.5)
                    } else {
                        stagger
                    };
                    text.split_stagger.ensure(0.0).add_keyframe(t_start_ms, s, Easing::Linear);
                }
                if let Some(offset_y) = offset_y_opt {
                    text.split_offset_y.ensure(0.0).add_keyframe(
                        t_start_ms,
                        offset_y,
                        Easing::Linear,
                    );
                }
                if let Some(mask) = mask_opt {
                    text.split_mask.ensure(false).add_keyframe(t_start_ms, mask, Easing::Linear);
                }

                // Typewriter effect: animate char_progress 0→1, or in whole-word
                // steps when the author asks for `by: word`.
                if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                    let guard_time = t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut text.char_progress, guard_time, 1.0);
                }

                let is_split_mode = offset_y_opt.is_some()
                    || mask_opt.is_some()
                    || stagger_opt.is_some()
                    || text.split_offset_y.is_some()
                    || text.split_mask.is_some()
                    || text.split_stagger.is_some();
                let by_word =
                    !is_split_mode && matches!(by_opt.as_deref(), Some("word") | Some("words"));
                let steps = by_word.then(|| {
                    let text = text.text_content.get(t_start_ms, String::new());
                    word_progress_steps(&text)
                });

                match steps {
                    Some(fractions) if fractions.len() > 1 => {
                        let span = t_end_ms.saturating_sub(t_start_ms);
                        // Each word gets an equal slice of the duration, and the
                        // ramp inside a slice is one frame long so the word
                        // arrives rather than being typed.
                        let n = fractions.len();
                        for (i, fraction) in fractions.iter().enumerate() {
                            let at = t_start_ms + (span as f64 * (i as f64 / n as f64)) as u64;
                            let settle = (at + span / (n as u64 * 4).max(1)).min(t_end_ms);
                            text.char_progress.ensure(1.0).add_keyframe(
                                at,
                                if i == 0 { 0.0 } else { fractions[i - 1] },
                                Easing::Linear,
                            );
                            text.char_progress.ensure(1.0).add_keyframe(
                                settle,
                                *fraction,
                                Easing::Linear,
                            );
                        }
                    },
                    _ => {
                        text.char_progress.ensure(1.0).add_keyframe(
                            t_start_ms,
                            0.0,
                            Easing::Linear,
                        );
                        text.char_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
                    },
                }
            } else {
                ensure_reveal_stroke(style, caps, t_start_ms);
                if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                    let guard_time = t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                    super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
                }

                // Reveal the fill to its authored value, not blindly to 1.0: a
                // stroke-only Path (fill_opacity seeded 0) must stay unfilled
                // after the draw completes.
                let authored_fill = style.fill_opacity.get(t_start_ms, 1.0);

                style.stroke_progress.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);
                style.fill_opacity.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);

                if duration_ms > 0.0 && t_end_ms > t_start_ms {
                    style.fill_opacity.ensure(1.0).add_keyframe(
                        t_end_ms.saturating_sub(1),
                        0.0,
                        Easing::Linear,
                    );
                }

                style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
                style.fill_opacity.ensure(1.0).add_keyframe(t_end_ms, authored_fill, easing);
            }
        }
    }
}

/// Reveals vector targets by drawing stroke progress first, then popping fill at the end.
pub struct RevealIn;

impl BuiltinAction for RevealIn {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "reveal-in".to_string(),
            category: "Entrance".to_string(),
            description:
                "Reveals vector targets by drawing stroke progress first, then popping fill at the end."
                    .to_string(),
            params: vec![],
            modifiers: {
                let mut mods = base_timing_params();
                mods.extend(split_reveal_params());
                mods
            },
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let parsed = parse_timing_modifiers(
            &timing_modifiers_without_keys(
                &action.modifiers,
                &["by", "stagger", "offset_y", "mask"],
            ),
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        for target in &action.targets {
            if !super::ensure_vector_reveal_target(
                timeline,
                target,
                &action.verb,
                diagnostics,
                None,
            ) {
                continue;
            }

            let is_text = timeline
                .world
                .tracks
                .get(target)
                .is_some_and(|track| is_text_like(timeline, track));
            let caps = match timeline.world.tracks.get_id(target) {
                Some(id) => timeline.world.caps(id),
                None => continue,
            };

            let Some(id) = timeline.world.tracks.get_id(target) else {
                continue;
            };
            let style = timeline.world.style.get_mut(id);

            // Reveal pre-keyframe ("hidden by default") targets: reveal-in
            // animates stroke_progress/fill_opacity but never opacity, so
            // lift the seeded opacity 0 alongside the reveal.
            let meta = timeline
                .world
                .meta
                .get_or_insert_with(id, crate::timeline::world::EntityMeta::default);
            let was_hidden = meta.hidden_by_default;
            super::lift_hidden_by_default(style, meta, t_start_ms, t_end_ms, easing);
            if !was_hidden {
                // An explicitly authored `opacity: 0` needs the same lift.
                super::reveal_authored_zero_opacity(style, t_start_ms, t_end_ms, easing);
            }

            if is_text {
                let text = timeline.world.text.get_mut(id);
                let by_opt = reveal_granularity(&action.modifiers);
                let stagger_opt = reveal_stagger(&action.modifiers);
                let offset_y_opt = reveal_offset_y(&action.modifiers);
                let mask_opt = reveal_mask(&action.modifiers);

                if let Some(by) = &by_opt {
                    text.split_by.ensure("char".to_string()).add_keyframe(
                        t_start_ms,
                        by.clone(),
                        Easing::Linear,
                    );
                }
                if let Some(stagger) = stagger_opt {
                    let s = if duration_ms > 0.0 && stagger > 0.0 {
                        (stagger * 1000.0 / duration_ms as f32).min(0.5)
                    } else {
                        stagger
                    };
                    text.split_stagger.ensure(0.0).add_keyframe(t_start_ms, s, Easing::Linear);
                }
                if let Some(offset_y) = offset_y_opt {
                    text.split_offset_y.ensure(0.0).add_keyframe(
                        t_start_ms,
                        offset_y,
                        Easing::Linear,
                    );
                }
                if let Some(mask) = mask_opt {
                    text.split_mask.ensure(false).add_keyframe(t_start_ms, mask, Easing::Linear);
                }

                if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                    let guard_time = t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut text.char_progress, guard_time, 1.0);
                }

                text.char_progress.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);
                text.char_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
            } else {
                let has_prior_stroke = style
                    .stroke_progress
                    .as_ref()
                    .map(|t| t.keyframes.keys().any(|&k| k < t_start_ms))
                    .unwrap_or(false);
                let start_stroke = if has_prior_stroke {
                    style.stroke_progress.get(t_start_ms, 1.0)
                } else {
                    0.0
                };

                ensure_reveal_stroke(style, caps, t_start_ms);

                // Reveal the fill to its authored value, not blindly to 1.0: a
                // stroke-only Path (fill_opacity seeded 0) must stay unfilled
                // after the reveal completes.
                let authored_fill = style.fill_opacity.get(t_start_ms, 1.0);

                if duration_ms > 0.0 {
                    style.stroke_progress.ensure(1.0).add_keyframe(
                        t_start_ms,
                        start_stroke,
                        Easing::Linear,
                    );
                    style.fill_opacity.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);
                    if t_end_ms > t_start_ms {
                        style.fill_opacity.ensure(1.0).add_keyframe(
                            t_end_ms.saturating_sub(1),
                            0.0,
                            Easing::Linear,
                        );
                    }
                } else if delay_ms > 0.0 && t_start_ms > 0 {
                    let guard_time = t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                    super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
                }

                style.fill_opacity.ensure(1.0).add_keyframe(
                    t_end_ms,
                    authored_fill,
                    Easing::Linear,
                );
                style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
            }
        }
    }
}

/// Wipes out vector targets by animating stroke progress and fill opacity down together.
pub struct WipeOut;

impl BuiltinAction for WipeOut {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "wipe-out".to_string(),
            category: "Exit".to_string(),
            description:
                "Wipes out vector targets by animating stroke progress and fill opacity down together."
                    .to_string(),
            params: vec![],
            modifiers: base_timing_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let parsed = parse_timing_modifiers(
            &action.modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        for target in &action.targets {
            if !super::ensure_vector_reveal_target(
                timeline,
                target,
                &action.verb,
                diagnostics,
                None,
            ) {
                continue;
            }

            let Some(id) = timeline.world.tracks.get_id(target) else {
                continue;
            };
            let style = timeline.world.style_mut(id);

            let start_stroke = style.stroke_progress.get(t_start_ms, 1.0);
            let start_fill = style.fill_opacity.get(t_start_ms, 1.0);

            if duration_ms > 0.0 {
                style.stroke_progress.ensure(1.0).add_keyframe(
                    t_start_ms,
                    start_stroke,
                    Easing::Linear,
                );
                style
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, start_fill, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
            }

            style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
            style.fill_opacity.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

/// Exits vector targets by hiding fill at the start, then erasing stroke progress over time.
pub struct RevealOut;

impl BuiltinAction for RevealOut {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "reveal-out".to_string(),
            category: "Exit".to_string(),
            description:
                "Exits vector targets by hiding fill at the start, then erasing stroke progress over time."
                    .to_string(),
            params: vec![],
            modifiers: base_timing_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let parsed = parse_timing_modifiers(
            &action.modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        for target in &action.targets {
            if !super::ensure_vector_reveal_target(
                timeline,
                target,
                &action.verb,
                diagnostics,
                None,
            ) {
                continue;
            }

            let Some(id) = timeline.world.tracks.get_id(target) else {
                continue;
            };
            let style = timeline.world.style_mut(id);

            let start_stroke = style.stroke_progress.get(t_start_ms, 1.0);

            if duration_ms > 0.0 {
                style.stroke_progress.ensure(1.0).add_keyframe(
                    t_start_ms,
                    start_stroke,
                    Easing::Linear,
                );
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
            }

            style.fill_opacity.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);
            style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

/// Exits vector targets by erasing stroke progress over time while keeping fill until the end.
pub struct DrawOut;

impl BuiltinAction for DrawOut {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "draw-out".to_string(),
            category: "Exit".to_string(),
            description:
                "Exits vector targets by erasing stroke progress over time while keeping fill until the end."
                    .to_string(),
            params: vec![],
            modifiers: base_timing_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let parsed = parse_timing_modifiers(
            &action.modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        for target in &action.targets {
            if !super::ensure_vector_reveal_target(
                timeline,
                target,
                &action.verb,
                diagnostics,
                None,
            ) {
                continue;
            }

            let Some(id) = timeline.world.tracks.get_id(target) else {
                continue;
            };
            let style = timeline.world.style_mut(id);

            let start_stroke = style.stroke_progress.get(t_start_ms, 1.0);
            let start_fill = style.fill_opacity.get(t_start_ms, 1.0);

            if duration_ms > 0.0 {
                style.stroke_progress.ensure(1.0).add_keyframe(
                    t_start_ms,
                    start_stroke,
                    Easing::Linear,
                );
                style
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, start_fill, Easing::Linear);
                style.fill_opacity.ensure(1.0).add_keyframe(
                    t_end_ms.saturating_sub(1),
                    start_fill,
                    Easing::Linear,
                );
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
            }

            style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
            style.fill_opacity.ensure(1.0).add_keyframe(t_end_ms, 0.0, Easing::Linear);
        }
    }
}

#[cfg(test)]
mod tests;
