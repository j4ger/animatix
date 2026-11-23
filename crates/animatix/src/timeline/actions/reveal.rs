use super::registry::{BuiltinAction, CallableDescriptor};
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::actions::BuiltinInvocation;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{DEFAULT_WHITE, Timeline};

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

/// Seed a scalar track's pre-entrance value.
///
/// A declaration writes its settled value as the track's first keyframe
/// (`stroke_progress: 1.0` by default). A later `draw_in` adds a start
/// keyframe at `t_start`, but [`PropertyTrack`](crate::timeline::PropertyTrack)
/// evaluates to its *first* keyframe for every earlier time, so the actor
/// paints its settled state from frame 0 and only pops invisible when the
/// entrance begins. Replace that first keyframe with the pre-state so the
/// entrance owns its whole window.
fn seed_pre_entrance_value(
    track: &mut Option<crate::timeline::PropertyTrack<f32>>,
    t_start_ms: u64,
    value: f32,
) {
    let Some(first_time) =
        track.as_ref().and_then(|t| t.keyframes().as_slice().first().map(|k| k.time_ms))
    else {
        return;
    };
    if first_time >= t_start_ms {
        return;
    }
    if let Some(track) = track.as_mut() {
        track.add_keyframe(first_time, value, Easing::Linear);
    }
}

/// Returns true when a primitive emits text glyph paths.
pub(super) fn is_text_like(timeline: &Timeline, id: crate::timeline::EntityId) -> bool {
    timeline
        .primitive_registry
        .info_of(timeline.world.actor_type(id))
        .is_some_and(|info| info.capabilities.text_paths)
}

/// Draws in vector targets by animating stroke progress first, then revealing fill.
pub struct DrawIn;

const SPLIT_PARAMS: &[&str] = &["by", "stagger", "offset_y", "mask", "order"];

fn text_option<'a>(modifiers: &'a [crate::ast::Modifier], key: &str) -> Option<&'a str> {
    modifiers
        .iter()
        .find(|m| m.name.as_deref() == Some(key))
        .and_then(|m| match &m.value {
            crate::ast::Expr::Ident(value) | crate::ast::Expr::Str(value) => Some(value.as_str()),
            _ => None,
        })
}

pub(super) fn set_text_wipe_direction(
    text: &mut crate::timeline::animation_track::TextTracks,
    modifiers: &[crate::ast::Modifier],
    start: u64,
) {
    if let Some(direction) = text_option(modifiers, "direction") {
        text.wipe_direction.ensure(String::new()).add_keyframe(
            start,
            direction.to_owned(),
            Easing::Linear,
        );
    } else if let Some(track) = &mut text.wipe_direction {
        track.add_keyframe(start, "left".into(), Easing::Linear);
    }
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
            crate::ast::Expr::Duration(seconds) => Some(*seconds as f32),
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
            crate::ast::Expr::Ident(s) | crate::ast::Expr::Str(s) => s == "true" || s == "baseline",
            _ => false,
        })
}

fn for_each_reveal_target(
    action: &BuiltinInvocation,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    params: &[&str],
    mut apply: impl FnMut(&mut Timeline, crate::timeline::EntityId, &super::ActionTiming),
) {
    let timing = super::ActionTiming::parse_excluding(action, params, time_ms, diagnostics);
    for target in &action.targets {
        if !super::ensure_vector_reveal_target(timeline, target, &action.verb, diagnostics, None) {
            continue;
        }

        let Some(id) = timeline.world.get_id(target) else {
            continue;
        };

        apply(timeline, id, &timing);
    }
}

fn apply_text_split_setup(
    text: &mut crate::timeline::animation_track::TextTracks,
    modifiers: &[crate::ast::Modifier],
    timing: &super::ActionTiming,
    entering: bool,
) {
    let by_opt = reveal_granularity(modifiers);
    let stagger_opt = reveal_stagger(modifiers);
    let offset_y_opt = reveal_offset_y(modifiers);
    let mask_opt = reveal_mask(modifiers);
    let guard = timing.t_start_ms.checked_sub(1);
    let order = text_option(modifiers, "order");
    if order.is_some() || text.split_reverse.is_some() {
        let reverse = if entering {
            order == Some("reverse")
        } else {
            order == Some("forward")
        };
        text.split_reverse
            .ensure(false)
            .add_keyframe(timing.t_start_ms, reverse, Easing::Linear);
    }

    if let Some(by) = &by_opt {
        if let Some(guard) = guard {
            super::ensure_guard_keyframe(&mut text.split_by, guard, "char".to_owned());
        }
        text.split_by.ensure("char".to_string()).add_keyframe(
            timing.t_start_ms,
            by.clone(),
            Easing::Linear,
        );
    }
    if let Some(stagger) = stagger_opt {
        if let Some(guard) = guard {
            super::ensure_guard_keyframe(&mut text.split_stagger, guard, 0.0);
        }
        let s = if timing.duration_ms > 0.0 && stagger > 0.0 {
            (stagger * 1000.0 / timing.duration_ms as f32).min(1.0)
        } else {
            stagger
        };
        text.split_stagger
            .ensure(0.0)
            .add_keyframe(timing.t_start_ms, s, Easing::Linear);
    }
    if let Some(offset_y) = offset_y_opt {
        if let Some(guard) = guard {
            super::ensure_guard_keyframe(&mut text.split_offset_y, guard, 0.0);
        }
        text.split_offset_y
            .ensure(0.0)
            .add_keyframe(timing.t_start_ms, offset_y, Easing::Linear);
    }
    if let Some(mask) = mask_opt {
        if let Some(guard) = guard {
            super::ensure_guard_keyframe(&mut text.split_mask, guard, false);
        }
        text.split_mask
            .ensure(false)
            .add_keyframe(timing.t_start_ms, mask, Easing::Linear);
    }
}

/// Text reveal uses shaped glyphs. Keep the old state up to this action
/// when the actor already has a progress track, or when the exit starts later.
pub(super) fn set_text_reveal(
    text: &mut crate::timeline::animation_track::TextTracks,
    timing: &super::ActionTiming,
    entering: bool,
    wipe: bool,
) {
    let start = timing.t_start_ms;
    let end = timing.t_end_ms;
    let initial = if entering {
        0.0
    } else {
        text.char_progress.get(start, 1.0)
    };
    if start > 0
        && (text.char_progress.is_some() || !entering || (start == end && timing.delay_ms > 0.0))
    {
        super::ensure_guard_keyframe(&mut text.char_progress, start - 1, 1.0);
    }
    if start < end {
        text.char_progress.ensure(1.0).add_keyframe(start, initial, Easing::Linear);
    }
    let whole_groups = matches!(
        text.split_by.get(start, String::new()).as_str(),
        "word" | "words" | "line" | "lines"
    ) && text.split_stagger.get(start, 0.0) <= 0.0
        && text.split_offset_y.get(start, 0.0) == 0.0
        && !text.split_mask.get(start, false);
    let easing = if !timing.ease_authored && (wipe || whole_groups) {
        Easing::Linear
    } else {
        timing.easing
    };
    text.char_progress
        .ensure(1.0)
        .add_keyframe(end, if entering { 1.0 } else { 0.0 }, easing);
    text.wipe_reveal.ensure(false).add_keyframe(start, wipe, Easing::Linear);
}

impl BuiltinAction for DrawIn {
    fn signature(&self) -> CallableDescriptor {
        animatix_core::builtins::callable::TimelineBuiltin::DrawIn.descriptor().clone()
    }

    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let timing =
            super::ActionTiming::parse_excluding(action, SPLIT_PARAMS, time_ms, diagnostics);

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

            let Some(id) = timeline.world.get_id(target) else {
                continue;
            };
            let is_text = is_text_like(timeline, id);
            let caps = timeline.world.caps(id);
            let style = timeline.world.style.get_mut(id);

            // Reveal pre-keyframe ("hidden by default") targets: draw_in
            // animates stroke_progress/fill_opacity/char_progress but never
            // opacity, so lift the seeded opacity 0 alongside the draw.
            let meta = timeline
                .world
                .meta
                .get_or_insert_with(id, crate::timeline::world::EntityMeta::default);
            let was_hidden = meta.hidden_by_default;
            super::lift_hidden_by_default(
                style,
                meta,
                timing.t_start_ms,
                timing.t_end_ms,
                timing.easing,
            );
            if !was_hidden {
                // An explicitly authored `opacity: 0` needs the same lift.
                super::reveal_authored_zero_opacity(
                    style,
                    timing.t_start_ms,
                    timing.t_end_ms,
                    timing.easing,
                );
            }

            if is_text {
                let text = timeline.world.ensure_text(id);
                apply_text_split_setup(text, &action.modifiers, &timing, true);
                set_text_reveal(text, &timing, true, false);
            } else {
                // Reveal the fill to its authored value, not blindly to 1.0: a
                // stroke-only Path (fill_opacity seeded 0) must stay unfilled
                // after the draw completes. Read it before seeding the
                // pre-entrance state below, which rewrites the first keyframe.
                let authored_fill = style.fill_opacity.get(timing.t_start_ms, 1.0);

                // A scheduled `draw_in` must not paint the settled stroke/fill
                // from frame 0: the declaration's first keyframe is the settled
                // value, so seed that keyframe with the entrance pre-state.
                seed_pre_entrance_value(&mut style.stroke_progress, timing.t_start_ms, 0.0);
                seed_pre_entrance_value(&mut style.fill_opacity, timing.t_start_ms, 0.0);
                ensure_reveal_stroke(style, caps, timing.t_start_ms);
                if timing.delay_ms > 0.0 && timing.duration_ms == 0.0 && timing.t_start_ms > 0 {
                    let guard_time = timing.t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                    super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
                }

                style.stroke_progress.ensure(1.0).add_keyframe(
                    timing.t_start_ms,
                    0.0,
                    Easing::Linear,
                );
                style
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(timing.t_start_ms, 0.0, Easing::Linear);

                if timing.duration_ms > 0.0 && timing.t_end_ms > timing.t_start_ms {
                    style.fill_opacity.ensure(1.0).add_keyframe(
                        timing.t_end_ms.saturating_sub(1),
                        0.0,
                        Easing::Linear,
                    );
                }

                style
                    .stroke_progress
                    .ensure(1.0)
                    .add_keyframe(timing.t_end_ms, 1.0, timing.easing);
                style.fill_opacity.ensure(1.0).add_keyframe(
                    timing.t_end_ms,
                    authored_fill,
                    timing.easing,
                );
            }
        }
    }
}

/// Reveals vector targets by drawing stroke progress first, then popping fill at the end.
pub struct RevealIn;

impl BuiltinAction for RevealIn {
    fn signature(&self) -> CallableDescriptor {
        animatix_core::builtins::callable::TimelineBuiltin::RevealIn
            .descriptor()
            .clone()
    }

    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let timing =
            super::ActionTiming::parse_excluding(action, SPLIT_PARAMS, time_ms, diagnostics);

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

            let Some(id) = timeline.world.get_id(target) else {
                continue;
            };
            let is_text = is_text_like(timeline, id);
            let caps = timeline.world.caps(id);
            let style = timeline.world.style.get_mut(id);

            // Reveal pre-keyframe ("hidden by default") targets: reveal_in
            // animates stroke_progress/fill_opacity but never opacity, so
            // lift the seeded opacity 0 alongside the reveal.
            let meta = timeline
                .world
                .meta
                .get_or_insert_with(id, crate::timeline::world::EntityMeta::default);
            let was_hidden = meta.hidden_by_default;
            super::lift_hidden_by_default(
                style,
                meta,
                timing.t_start_ms,
                timing.t_end_ms,
                timing.easing,
            );
            if !was_hidden {
                // An explicitly authored `opacity: 0` needs the same lift.
                super::reveal_authored_zero_opacity(
                    style,
                    timing.t_start_ms,
                    timing.t_end_ms,
                    timing.easing,
                );
            }

            if is_text {
                let text = timeline.world.ensure_text(id);
                apply_text_split_setup(text, &action.modifiers, &timing, true);
                set_text_reveal(text, &timing, true, false);
            } else {
                let has_prior_stroke = style
                    .stroke_progress
                    .as_ref()
                    .map(|t| t.keyframes.keys().any(|&k| k < timing.t_start_ms))
                    .unwrap_or(false);
                let start_stroke = if has_prior_stroke {
                    style.stroke_progress.get(timing.t_start_ms, 1.0)
                } else {
                    0.0
                };

                ensure_reveal_stroke(style, caps, timing.t_start_ms);

                // Reveal the fill to its authored value, not blindly to 1.0: a
                // stroke-only Path (fill_opacity seeded 0) must stay unfilled
                // after the reveal completes.
                let authored_fill = style.fill_opacity.get(timing.t_start_ms, 1.0);

                if timing.duration_ms > 0.0 {
                    style.stroke_progress.ensure(1.0).add_keyframe(
                        timing.t_start_ms,
                        start_stroke,
                        Easing::Linear,
                    );
                    style.fill_opacity.ensure(1.0).add_keyframe(
                        timing.t_start_ms,
                        0.0,
                        Easing::Linear,
                    );
                    if timing.t_end_ms > timing.t_start_ms {
                        style.fill_opacity.ensure(1.0).add_keyframe(
                            timing.t_end_ms.saturating_sub(1),
                            0.0,
                            Easing::Linear,
                        );
                    }
                } else if timing.delay_ms > 0.0 && timing.t_start_ms > 0 {
                    let guard_time = timing.t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                    super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
                }

                style.fill_opacity.ensure(1.0).add_keyframe(
                    timing.t_end_ms,
                    authored_fill,
                    Easing::Linear,
                );
                style
                    .stroke_progress
                    .ensure(1.0)
                    .add_keyframe(timing.t_end_ms, 1.0, timing.easing);
            }
        }
    }
}

/// Wipes out vector targets by animating stroke progress and fill opacity down together.
pub struct WipeOut;

impl BuiltinAction for WipeOut {
    fn signature(&self) -> CallableDescriptor {
        animatix_core::builtins::callable::TimelineBuiltin::WipeOut.descriptor().clone()
    }

    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for_each_reveal_target(
            action,
            time_ms,
            timeline,
            diagnostics,
            &["direction"],
            |timeline, id, timing| {
                if is_text_like(timeline, id) {
                    let text = timeline.world.ensure_text(id);
                    set_text_wipe_direction(text, &action.modifiers, timing.t_start_ms);
                    set_text_reveal(text, timing, false, true);
                    return;
                }
                let style = timeline.world.style_mut(id);

                let start_stroke = style.stroke_progress.get(timing.t_start_ms, 1.0);
                let start_fill = style.fill_opacity.get(timing.t_start_ms, 1.0);

                if timing.duration_ms > 0.0 {
                    style.stroke_progress.ensure(1.0).add_keyframe(
                        timing.t_start_ms,
                        start_stroke,
                        Easing::Linear,
                    );
                    style.fill_opacity.ensure(1.0).add_keyframe(
                        timing.t_start_ms,
                        start_fill,
                        Easing::Linear,
                    );
                } else if timing.delay_ms > 0.0 && timing.t_start_ms > 0 {
                    let guard_time = timing.t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                    super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
                }

                style
                    .stroke_progress
                    .ensure(1.0)
                    .add_keyframe(timing.t_end_ms, 0.0, timing.easing);
                style.fill_opacity.ensure(1.0).add_keyframe(timing.t_end_ms, 0.0, timing.easing);
            },
        );
    }
}

/// Exits vector targets by hiding fill at the start, then erasing stroke progress over time.
pub struct RevealOut;

impl BuiltinAction for RevealOut {
    fn signature(&self) -> CallableDescriptor {
        animatix_core::builtins::callable::TimelineBuiltin::RevealOut
            .descriptor()
            .clone()
    }

    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for_each_reveal_target(
            action,
            time_ms,
            timeline,
            diagnostics,
            SPLIT_PARAMS,
            |timeline, id, timing| {
                if is_text_like(timeline, id) {
                    let text = timeline.world.ensure_text(id);
                    apply_text_split_setup(text, &action.modifiers, timing, false);
                    set_text_reveal(text, timing, false, false);
                    return;
                }
                let style = timeline.world.style_mut(id);

                let start_stroke = style.stroke_progress.get(timing.t_start_ms, 1.0);

                if timing.duration_ms > 0.0 {
                    style.stroke_progress.ensure(1.0).add_keyframe(
                        timing.t_start_ms,
                        start_stroke,
                        Easing::Linear,
                    );
                } else if timing.delay_ms > 0.0 && timing.t_start_ms > 0 {
                    let guard_time = timing.t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                    super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
                }

                style
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(timing.t_start_ms, 0.0, Easing::Linear);
                style
                    .stroke_progress
                    .ensure(1.0)
                    .add_keyframe(timing.t_end_ms, 0.0, timing.easing);
            },
        );
    }
}

/// Exits vector targets by erasing stroke progress over time while keeping fill until the end.
pub struct DrawOut;

impl BuiltinAction for DrawOut {
    fn signature(&self) -> CallableDescriptor {
        animatix_core::builtins::callable::TimelineBuiltin::DrawOut.descriptor().clone()
    }

    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for_each_reveal_target(
            action,
            time_ms,
            timeline,
            diagnostics,
            SPLIT_PARAMS,
            |timeline, id, timing| {
                if is_text_like(timeline, id) {
                    let text = timeline.world.ensure_text(id);
                    apply_text_split_setup(text, &action.modifiers, timing, false);
                    set_text_reveal(text, timing, false, false);
                    return;
                }
                let style = timeline.world.style_mut(id);

                let start_stroke = style.stroke_progress.get(timing.t_start_ms, 1.0);
                let start_fill = style.fill_opacity.get(timing.t_start_ms, 1.0);

                if timing.duration_ms > 0.0 {
                    style.stroke_progress.ensure(1.0).add_keyframe(
                        timing.t_start_ms,
                        start_stroke,
                        Easing::Linear,
                    );
                    style.fill_opacity.ensure(1.0).add_keyframe(
                        timing.t_start_ms,
                        start_fill,
                        Easing::Linear,
                    );
                    style.fill_opacity.ensure(1.0).add_keyframe(
                        timing.t_end_ms.saturating_sub(1),
                        start_fill,
                        Easing::Linear,
                    );
                } else if timing.delay_ms > 0.0 && timing.t_start_ms > 0 {
                    let guard_time = timing.t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut style.stroke_progress, guard_time, 1.0);
                    super::ensure_guard_keyframe(&mut style.fill_opacity, guard_time, 1.0);
                }

                style
                    .stroke_progress
                    .ensure(1.0)
                    .add_keyframe(timing.t_end_ms, 0.0, timing.easing);
                style
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(timing.t_end_ms, 0.0, Easing::Linear);
            },
        );
    }
}

#[cfg(test)]
mod tests;
