//! Animation helpers using design token durations and easing.

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

impl Lerp for Color32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        crate::tokens::util::lerp_color(self, other, t)
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
/// seconds. If `transition` is `INSTANT` or the current `MotionPreference`
/// is `MotionPreference::Reduced`, returns `target` immediately.
pub fn animate_toward(ctx: &Context, id: Id, target: f32, transition: Transition) -> f32 {
    let pref = motion_preference_from_ctx(ctx);
    let duration = resolve_duration(pref, transition);
    ctx.animate_value_with_time(id, target, duration)
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

#[derive(Clone, Copy)]
struct EasedValue {
    from: f32,
    target: f32,
    started: f64,
    transition: Transition,
}

impl EasedValue {
    fn sample(self, now: f64) -> f32 {
        let elapsed = (now - self.started) as f32;
        if self.from == self.target || elapsed >= self.transition.duration {
            return self.target;
        }
        let progress = self.transition.easing.sample(elapsed / self.transition.duration);
        self.from + (self.target - self.from) * progress
    }
}

/// Animate a scalar in its own coordinate space. A reversal starts from the
/// currently visible value, even when the next transition has a different curve.
pub fn animate_toward_eased(ctx: &Context, id: Id, target: f32, transition: Transition) -> f32 {
    let duration = resolve_duration(motion_preference_from_ctx(ctx), transition);
    let now = ctx.input(|i| i.time);
    let (value, moving) = ctx.data_mut(|d| {
        let state = d.get_temp_mut_or(
            id,
            EasedValue {
                from: target,
                target,
                started: now,
                transition,
            },
        );
        if duration == INSTANT {
            state.from = target;
            state.target = target;
        } else if state.target != target {
            *state = EasedValue {
                from: state.sample(now),
                target,
                started: now,
                transition,
            };
        }
        let moving =
            state.from != target && now - state.started < f64::from(state.transition.duration);
        (state.sample(now), moving)
    });
    if moving {
        ctx.request_repaint();
    }
    value
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
        let id = egui::Id::unique("test_reduced");
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
        let id = egui::Id::unique("test_full");
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
    fn eased_indicator_retargets_from_its_visible_position_and_reduced_motion_settles() {
        use crate::tokens::motion::{HOVER, MotionPreference, PANEL, set_motion_preference};
        let ctx = Context::default();
        let id = Id::unique("indicator");
        let frame = |time, target, transition| {
            let mut value = 0.0;
            let _ = crate::run_ui(
                &ctx,
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |_| {
                    value = animate_toward_eased(&ctx, id, target, transition);
                },
            );
            value
        };
        assert_eq!(frame(0.0, 1.0, PANEL), 1.0);
        assert_eq!(frame(0.1, 3.0, PANEL), 1.0);
        let midpoint = frame(0.2, 3.0, PANEL);
        assert!(midpoint > 1.0 && midpoint < 3.0);
        assert_eq!(frame(0.2, 0.0, HOVER), midpoint);
        let returning = frame(0.23, 0.0, HOVER);
        assert!(returning > 0.0 && returning < midpoint);
        set_motion_preference(&ctx, MotionPreference::Reduced);
        assert_eq!(frame(0.24, 2.0, HOVER), 2.0);
        set_motion_preference(&ctx, MotionPreference::Full);
        assert_eq!(frame(0.25, 2.0, HOVER), 2.0);
        assert_eq!(frame(0.26, 0.0, HOVER), 2.0);
        assert_eq!(frame(0.37, 0.0, HOVER), 0.0);
    }

    #[test]
    fn animate_toward_eased_reduced_motion_snaps() {
        use crate::tokens::motion::{
            MotionPreference, NORMAL, STANDARD, Transition, set_motion_preference,
        };
        let ctx = Context::default();
        set_motion_preference(&ctx, MotionPreference::Reduced);
        let id = egui::Id::unique("test_eased_reduced");
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
