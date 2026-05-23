//! Easing curve definitions and helpers for the Animatix syntax crate.
//!
//! Provides the [`Easing`] enum, a canonical easing name registry, and
//! functions to apply easing to a progress value and to parse easing names.

/// Available easing curves for animation interpolation.
#[derive(Debug, Clone, Copy, PartialEq)]
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
];

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
        }
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
        }
        Easing::Elastic => {
            if t == 0.0 || t == 1.0 {
                return t;
            }
            let c4 = (2.0 * std::f32::consts::PI) / 3.0;
            -(2.0_f32.powf(10.0 * (t - 1.0))) * ((t * 10.0 - 10.75) * c4).sin()
        }
        Easing::Back => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            c3 * t * t * t - c1 * t * t
        }
        Easing::Expo => {
            if t == 0.0 {
                0.0
            } else {
                2.0_f32.powf(10.0 * (t - 1.0))
            }
        }
    }
}

/// Parse an easing name string into an [`Easing`] variant.
///
/// Accepts both hyphenated and unhyphenated lowercase forms (e.g.
/// `"ease-in"` or `"easein"`). Returns `None` if the name is not recognized.
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
        _ => None,
    }
}
