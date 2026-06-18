//! Animation helpers using design token durations and easing.
//!
//! "Easing is metadata" — cubic-bezier curves are now sampled manually via
//! `CubicBezier::sample()` (Newton-Raphson + binary search).
//! `animate_toward_eased()` applies the sampled curve on top of egui's linear
//! time interpolation, giving real non-linear animation.

use crate::app::design_tokens::motion::{INSTANT, Transition};
use egui::{Context, Id};

/// Animates a value toward a target using the given transition parameters.
///
/// Returns the current animated value. When `target` changes, starts animating
/// from the current animated value toward the new target over `transition.duration`
/// seconds. If `transition` is `INSTANT`, returns `target` immediately.
pub fn animate_toward(ctx: &Context, id: Id, target: f32, transition: Transition) -> f32 {
    if transition.duration == INSTANT {
        return target;
    }
    ctx.animate_value_with_time(id, target, transition.duration)
}

/// Animates a boolean state (e.g., hover, active) toward a target boolean.
/// Returns an f32 in [0.0, 1.0] where 0.0 = false, 1.0 = true.
#[allow(dead_code)] // Reserved for future animation call sites
pub fn animate_bool(ctx: &Context, id: Id, target: bool, transition: Transition) -> f32 {
    let target_f = if target { 1.0 } else { 0.0 };
    animate_toward(ctx, id, target_f, transition)
}

/// Animates a color channel (f32 in 0..=1 range) toward a target.
#[allow(dead_code)] // Reserved for future animation call sites
pub fn animate_channel(ctx: &Context, id: Id, target: f32, transition: Transition) -> f32 {
    animate_toward(ctx, id, target, transition)
}

/// Animates a boolean-state value (0.0 or 1.0 target) with easing.
/// Returns the eased progress in [0, 1].
/// Uses `animate_toward` for linear time progress, then applies the
/// transition's cubic-bezier easing via `CubicBezier::sample()`.
#[allow(dead_code)] // Reserved for future animation call sites; dialog now handles easing inline
pub fn animate_toward_eased(ctx: &Context, id: Id, target: f32, transition: Transition) -> f32 {
    if transition.duration == INSTANT {
        return target;
    }
    let linear = animate_toward(ctx, id, target, transition);
    // linear is in [0, 1] because target is either 0 or 1 and initial value is the opposite
    transition.easing.sample(linear.clamp(0.0, 1.0))
}
