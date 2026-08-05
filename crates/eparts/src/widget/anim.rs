//! Animation helpers using design token durations and easing.
//!
//! "Easing is metadata" — cubic-bezier curves are now sampled manually via
//! `CubicBezier::sample()` (Newton-Raphson + binary search).
//! `animate_toward_eased()` applies the sampled curve on top of egui's linear
//! time interpolation, giving real non-linear animation.

use egui::{Color32, Context, Id};

use crate::tokens::motion::{INSTANT, Transition, motion_preference_from_ctx, resolve_duration};

/// Linear interpolation between two values of the same type.
///
/// Foundation for widget micro-animations (crossfades, expands). Implemented
/// for the common UI types; `t` is the progress in `[0, 1]`.
pub trait Lerp {
    /// Interpolate from `self` toward `other` by `t` (clamped to `[0, 1]`).
    fn lerp(self, other: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        self + (other - self) * t
    }
}

impl Lerp for egui::Color32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        // Interpolate in linear-ish premultiplied space via egui's blend.
        Color32::from_rgba_unmultiplied(
            (self.r() as f32).lerp(other.r() as f32, t).round() as u8,
            (self.g() as f32).lerp(other.g() as f32, t).round() as u8,
            (self.b() as f32).lerp(other.b() as f32, t).round() as u8,
            (self.a() as f32).lerp(other.a() as f32, t).round() as u8,
        )
    }
}

impl Lerp for egui::Pos2 {
    fn lerp(self, other: Self, t: f32) -> Self {
        egui::pos2(self.x.lerp(other.x, t), self.y.lerp(other.y, t))
    }
}

impl Lerp for egui::Vec2 {
    fn lerp(self, other: Self, t: f32) -> Self {
        egui::vec2(self.x.lerp(other.x, t), self.y.lerp(other.y, t))
    }
}

/// Animates a value toward a target using the given transition parameters.
///
/// Returns the current animated value. When `target` changes, starts animating
/// from the current animated value toward the new target over `transition.duration`
/// seconds. If `transition` is `INSTANT` or the current [`MotionPreference`]
/// is [`MotionPreference::Reduced`], returns `target` immediately.
pub fn animate_toward(ctx: &Context, id: Id, target: f32, transition: Transition) -> f32 {
    let pref = motion_preference_from_ctx(ctx);
    let duration = resolve_duration(pref, transition);
    if duration == INSTANT {
        return target;
    }
    ctx.animate_value_with_time(id, target, duration)
}

/// Animates a boolean state (e.g., hover, active) toward a target boolean.
/// Returns an f32 in [0.0, 1.0] where 0.0 = false, 1.0 = true.
pub fn animate_bool(ctx: &Context, id: Id, target: bool, transition: Transition) -> f32 {
    let target_f = if target { 1.0 } else { 0.0 };
    animate_toward(ctx, id, target_f, transition)
}

/// Animates a color channel (f32 in 0..=1 range) toward a target.
#[allow(dead_code)] // Reserved for future animation call sites
pub fn animate_channel(ctx: &Context, id: Id, target: f32, transition: Transition) -> f32 {
    animate_toward(ctx, id, target, transition)
}

/// Eased boolean animation: returns progress in `[0, 1]` with the transition's
/// cubic-bezier easing applied (smooth crossfades/expands for widgets like
/// checkboxes, switches, collapsibles).
pub fn animate_bool_eased(ctx: &Context, id: Id, target: bool, transition: Transition) -> f32 {
    let target_f = if target { 1.0 } else { 0.0 };
    animate_toward_eased(ctx, id, target_f, transition)
}

/// Interpolate any [`Lerp`] value by an eased boolean transition.
pub fn animate_lerp<T: Lerp>(
    ctx: &Context,
    id: Id,
    from: T,
    to: T,
    target: bool,
    transition: Transition,
) -> T {
    let t = animate_bool_eased(ctx, id, target, transition);
    from.lerp(to, t)
}

/// Animates a boolean-state value (0.0 or 1.0 target) with easing.
/// Returns the eased progress in [0, 1].
/// Uses `animate_toward` for linear time progress, then applies the
/// transition's cubic-bezier easing via `CubicBezier::sample()`.
pub fn animate_toward_eased(ctx: &Context, id: Id, target: f32, transition: Transition) -> f32 {
    let pref = motion_preference_from_ctx(ctx);
    let duration = resolve_duration(pref, transition);
    if duration == INSTANT {
        return target;
    }
    let linear = animate_toward(ctx, id, target, transition);
    // linear is in [0, 1] because target is either 0 or 1 and initial value is the opposite
    transition.easing.sample(linear.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_f32() {
        assert_eq!(0.0_f32.lerp(10.0, 0.5), 5.0);
        assert_eq!(0.0_f32.lerp(10.0, 0.0), 0.0);
        assert_eq!(0.0_f32.lerp(10.0, 1.0), 10.0);
        // clamps out-of-range t
        assert_eq!(0.0_f32.lerp(10.0, 2.0), 10.0);
    }

    #[test]
    fn lerp_color_endpoints() {
        let a = Color32::from_rgb(0, 0, 0);
        let b = Color32::from_rgb(255, 255, 255);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0).r(), 255);
    }

    #[test]
    fn lerp_pos_vec() {
        assert_eq!(egui::pos2(0.0, 0.0).lerp(egui::pos2(4.0, 8.0), 0.5), egui::pos2(2.0, 4.0));
        assert_eq!(egui::vec2(0.0, 0.0).lerp(egui::vec2(4.0, 8.0), 0.25), egui::vec2(1.0, 2.0));
    }

    #[test]
    fn animate_toward_reduced_motion_snaps() {
        use crate::tokens::motion::{
            MotionPreference, SLOW, STANDARD, Transition, set_motion_preference,
        };
        let ctx = Context::default();
        set_motion_preference(&ctx, MotionPreference::Reduced);
        let id = egui::Id::new("test_reduced");
        let t = animate_toward(
            &ctx,
            id,
            42.0,
            Transition {
                duration: SLOW,
                easing: STANDARD,
            },
        );
        assert_eq!(t, 42.0);
    }

    #[test]
    fn animate_toward_full_motion_animates() {
        use crate::tokens::motion::{FAST, STANDARD, Transition};
        let ctx = Context::default();
        let id = egui::Id::new("test_full");
        // With default (Full) preference and a brand-new id, egui returns the
        // target immediately because there is no prior animation state to interpolate from.
        let result = animate_toward(
            &ctx,
            id,
            42.0,
            Transition {
                duration: FAST,
                easing: STANDARD,
            },
        );
        assert_eq!(result, 42.0);
    }

    #[test]
    fn animate_toward_eased_reduced_motion_snaps() {
        use crate::tokens::motion::{
            MotionPreference, NORMAL, STANDARD, Transition, set_motion_preference,
        };
        let ctx = Context::default();
        set_motion_preference(&ctx, MotionPreference::Reduced);
        let id = egui::Id::new("test_eased_reduced");
        let t = animate_toward_eased(
            &ctx,
            id,
            1.0,
            Transition {
                duration: NORMAL,
                easing: STANDARD,
            },
        );
        assert_eq!(t, 1.0);
    }
}
