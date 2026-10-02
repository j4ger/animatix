//! Easing curve definitions and helpers for the Animatix syntax crate.
//!
//! Provides the `Easing` enum, a canonical easing name registry, and
//! functions to apply easing to a progress value and to parse easing names.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Available easing curves for animation interpolation.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Easing {
    /// No easing; progress is linear.
    Linear,
    /// Ease in — starts slow and accelerates.
    EaseIn,
    /// Ease out — starts fast and decelerates.
    EaseOut,
    /// Ease in-out — slow start and slow end.
    EaseInOut,
    /// Bounce easing with natural bounce behavior.
    Bounce,
    /// Elastic easing with spring-like overshoot.
    Elastic,
    /// Back easing with a slight backward pull at the start.
    Back,
    /// Exponential easing with rapid acceleration.
    Expo,
    /// Custom cubic-bezier easing with two control points.
    ///
    /// The four values are `(p1x, p1y, p2x, p2y)` where P1 and P2 are the
    /// control points of a cubic Bézier curve with P0=(0,0) and P3=(1,1).
    CubicBezier([f32; 4]),
}

/// Registry of canonical easing names and their human-readable labels.
///
/// Each pair maps a lowercase identifier (e.g. `"easein"`) to a display label
/// (e.g. `"Ease In"`). Used for editor completion and UI presentation.
pub const EASING_REGISTRY: &[(&str, &str)] = &[
    ("linear", "Linear"),
    ("easein", "Ease In"),
    ("easeout", "Ease Out"),
    ("easeinout", "Ease In Out"),
    ("bounce", "Bounce"),
    ("elastic", "Elastic"),
    ("back", "Back"),
    ("expo", "Expo"),
    ("custom", "Custom"),
];

/// Default cubic-bezier control points that approximate `EaseInOut`.
pub const DEFAULT_CUSTOM_EASING: [f32; 4] = [0.42, 0.0, 0.58, 1.0];

/// Apply an easing curve to a normalized progress value.
///
/// `progress` is clamped to the range `[0.0, 1.0]` before the easing formula
/// is evaluated. Returns the eased value, also in `[0.0, 1.0]`.
pub fn apply_easing(progress: f32, easing: Easing) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::EaseIn => t * t,
        Easing::EaseOut => t * (2.0 - t),
        Easing::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        },
        Easing::Bounce => {
            let n1 = 7.5625;
            let d1 = 2.75;
            if t < 1.0 / d1 {
                n1 * t * t
            } else if t < 2.0 / d1 {
                let t = t - 1.5 / d1;
                n1 * t * t + 0.75
            } else if t < 2.5 / d1 {
                let t = t - 2.25 / d1;
                n1 * t * t + 0.9375
            } else {
                let t = t - 2.625 / d1;
                n1 * t * t + 0.984375
            }
        },
        Easing::Elastic => {
            if t == 0.0 || t == 1.0 {
                return t;
            }
            let c4 = (2.0 * std::f32::consts::PI) / 3.0;
            -(2.0_f32.powf(10.0 * (t - 1.0))) * ((t * 10.0 - 10.75) * c4).sin()
        },
        Easing::Back => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            c3 * t * t * t - c1 * t * t
        },
        Easing::Expo => {
            if t == 0.0 {
                0.0
            } else {
                2.0_f32.powf(10.0 * (t - 1.0))
            }
        },
        Easing::CubicBezier(cp) => evaluate_cubic_bezier(t, cp),
    }
}

/// Evaluate a cubic Bézier curve at a given x coordinate.
///
/// Given control points `cp = [p1x, p1y, p2x, p2y]` defining a cubic Bézier
/// with P0=(0,0) and P3=(1,1), finds the parameter `t` such that
/// `Bezier_x(t) ≈ x` using binary search, then returns `Bezier_y(t)`.
fn evaluate_cubic_bezier(x: f32, cp: [f32; 4]) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    // Binary search for t where Bezier_x(t) = x
    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    let mut t = 0.5f32;
    for _ in 0..12 {
        let bx = cubic_bezier_x(t, cp);
        if bx < x {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) / 2.0;
    }
    cubic_bezier_y(t, cp)
}

/// Cubic Bézier X(t) = (1-t)³·0 + 3(1-t)²t·p1x + 3(1-t)t²·p2x + t³·1
fn cubic_bezier_x(t: f32, cp: [f32; 4]) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let omt = 1.0 - t;
    3.0 * omt * omt * t * cp[0] + 3.0 * omt * t * t * cp[2] + t * t * t
}

/// Cubic Bézier Y(t) = (1-t)³·0 + 3(1-t)²t·p1y + 3(1-t)t²·p2y + t³·1
fn cubic_bezier_y(t: f32, cp: [f32; 4]) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let omt = 1.0 - t;
    3.0 * omt * omt * t * cp[1] + 3.0 * omt * t * t * cp[3] + t * t * t
}

/// Parse an easing name string into an [`Easing`] variant.
///
/// Accepts both hyphenated and unhyphenated lowercase forms (e.g.
/// `"ease-in"` or `"easein"`). Returns `None` if the name is not recognized.
///
/// For `"custom"`, returns [`Easing::CubicBezier`] with default control points.
pub fn parse_easing_name(raw: &str) -> Option<Easing> {
    match raw {
        "ease-in" | "easein" => Some(Easing::EaseIn),
        "ease-out" | "easeout" => Some(Easing::EaseOut),
        "ease-in-out" | "easeinout" => Some(Easing::EaseInOut),
        "bounce" => Some(Easing::Bounce),
        "elastic" => Some(Easing::Elastic),
        "back" => Some(Easing::Back),
        "expo" => Some(Easing::Expo),
        "linear" => Some(Easing::Linear),
        "custom" => Some(Easing::CubicBezier(DEFAULT_CUSTOM_EASING)),
        _ => None,
    }
}

/// Format a cubic-bezier easing as a CSS-like string.
///
/// Example: `format_cubic_bezier([0.42, 0.0, 0.58, 1.0])` → `"cubic-bezier(0.42, 0, 0.58, 1)"`
pub fn format_cubic_bezier(cp: [f32; 4]) -> String {
    format!("cubic-bezier({:.2}, {:.2}, {:.2}, {:.2})", cp[0], cp[1], cp[2], cp[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry is what the GUI, the curve panel and LSP completion list,
    /// and this parser is what `.amx` source hits. A name on one side and not
    /// the other means the editor offers an easing the engine then rejects.
    #[test]
    fn every_registry_id_parses() {
        for (id, label) in EASING_REGISTRY {
            assert!(
                parse_easing_name(id).is_some(),
                "registry id {id:?} ({label:?}) does not parse"
            );
        }
    }

    /// Every curve must be anchored at both ends: a keyframe's target value is
    /// what the stage shows once it settles, so a curve that lands anywhere
    /// but 1.0 silently moves the resting composition.
    #[test]
    fn every_curve_holds_its_endpoints() {
        let curves = [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::Bounce,
            Easing::Elastic,
            Easing::Back,
            Easing::Expo,
            Easing::CubicBezier(DEFAULT_CUSTOM_EASING),
        ];
        for easing in curves {
            assert!(
                apply_easing(0.0, easing).abs() < 1e-6,
                "{easing:?} does not start at 0"
            );
            assert!(
                (apply_easing(1.0, easing) - 1.0).abs() < 1e-3,
                "{easing:?} does not land on 1"
            );
        }
    }
}
