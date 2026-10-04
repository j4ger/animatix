//! Entrance presets that bundle the two craft moves the plain verbs leave out:
//! a scale *arrival* (`settle-in`) and an overshooting one (`pop-in`).
//!
//! Both are a `fade-in` plus a scale ramp onto the actor's authored scale, so
//! they read as an object coming to rest instead of a value being dialled up.
//! They exist because a bare opacity fade is the single most over-used
//! entrance in the corpus and, per the motion-design craft tables, an entrance
//! that only changes opacity reads as "floaty".

use super::registry::{ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

/// How far below its final size a `settle-in` starts.
const SETTLE_START_FACTOR: f32 = 0.92;
/// How far below its final size a `pop-in` starts.
const POP_START_FACTOR: f32 = 0.6;

/// The shared body of the two entrance presets: the per-verb shape differs only
/// in how far the scale starts and how it arrives.
struct Preset {
    /// How the ramp starts, relative to the actor's sampled scale.
    start_factor: f32,
    /// The curve the scale ramp follows when the author passes no `ease:`.
    arrival: Easing,
    /// How far *past* its final size the ramp peaks, or `None` for a ramp that
    /// only arrives. The overshoot is an explicit intermediate keyframe rather
    /// than an easing curve, because a segment curve that exceeds its own end is
    /// what the easing layer clamps.
    overshoot: Option<f32>,
}

const SETTLE: Preset = Preset {
    start_factor: SETTLE_START_FACTOR,
    arrival: Easing::ExpoOut,
    overshoot: None,
};
const POP: Preset = Preset {
    start_factor: POP_START_FACTOR,
    arrival: Easing::EaseOut,
    overshoot: Some(0.08),
};

/// Where along the ramp the overshoot peak sits, as a fraction of the duration.
const OVERSHOOT_AT: f64 = 0.7;

/// Run one entrance preset over the action's targets.
fn run_preset(
    preset: &Preset,
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
    // An explicit `ease:` wins; otherwise the preset picks its own arrival.
    let easing = if parsed.easing == Easing::Linear && !parsed.ease_authored {
        preset.arrival
    } else {
        parsed.easing
    };

    let t_start_ms = (time_ms + delay_ms) as u64;
    let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

    for target in &action.targets {
        if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
            continue;
        }

        // Opacity: the same lift `fade-in` performs, so a hidden-by-default
        // actor is revealed through the subtree-aware path and an authored
        // `opacity: 0` seed is lifted past rather than settled on.
        let was_hidden = timeline.tracks.get(target).is_some_and(|t| t.hidden_by_default);
        super::lift_hidden_by_default_subtree(timeline, target, t_start_ms, t_end_ms, easing);
        if !was_hidden {
            let Some(track) = timeline.tracks.get(target) else {
                continue;
            };
            let end_opacity = super::entrance_opacity_target(track, t_start_ms);
            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };
            track.style.opacity.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);
            track.style.opacity.ensure(1.0).add_keyframe(t_end_ms, end_opacity, easing);
        }

        // Scale: ramp *relative to* the scale the actor already samples at the
        // entrance's start, so a preset never fights an authored transform.
        let authored = {
            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };
            let authored = track.geometry.scale.get(t_start_ms, 1.0);
            track.geometry.scale.ensure(1.0).add_keyframe(
                t_start_ms,
                authored * preset.start_factor,
                Easing::Linear,
            );
            if let Some(peak) = preset.overshoot {
                if t_end_ms > t_start_ms {
                    let peak_ms =
                        t_start_ms + ((t_end_ms - t_start_ms) as f64 * OVERSHOOT_AT) as u64;
                    track.geometry.scale.ensure(1.0).add_keyframe(
                        peak_ms,
                        authored * (1.0 + peak),
                        Easing::EaseIn,
                    );
                }
            }
            track.geometry.scale.ensure(1.0).add_keyframe(t_end_ms, authored, easing);
            authored
        };
        if authored <= 0.0 {
            tracing::warn!(
                "{}: {} was authored with scale {authored}; the entrance ramp starts at a non-positive scale",
                target,
                action.verb
            );
        }
    }
}

/// The `settle-in` signature.
fn signature(name: &str, description: &str) -> ActionSignature {
    ActionSignature {
        name: name.to_string(),
        category: "Entrance".to_string(),
        description: description.to_string(),
        params: vec![],
        modifiers: base_timing_params(),
    }
}

/// Fades in while growing a few percent into place.
pub struct SettleIn;

impl BuiltinAction for SettleIn {
    fn signature(&self) -> ActionSignature {
        signature(
            "settle-in",
            "Fades in and eases the target up from a slightly smaller scale, so it arrives \
             instead of appearing.",
        )
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        run_preset(&SETTLE, action, time_ms, timeline, diagnostics);
    }
}

/// Fades in while scaling up past its final size and settling back.
pub struct PopIn;

impl BuiltinAction for PopIn {
    fn signature(&self) -> ActionSignature {
        signature(
            "pop-in",
            "Fades in and scales up with a back overshoot, for playful or emphatic arrivals.",
        )
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        run_preset(&POP, action, time_ms, timeline, diagnostics);
    }
}
