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
    ///
    /// Named `bounce-in` in source: this is the CSS curve, which modulates
    /// progress *along* the segment between two values. It is not gravity —
    /// the `bounce` action is the thing that moves an actor through space.
    Bounce,
    /// Elastic easing with spring-like overshoot.
    Elastic,
    /// Back easing with a slight backward pull at the start.
    Back,
    /// Exponential easing with rapid acceleration.
    Expo,
    /// Exponential ease-out — fast start, long settle.
    ExpoOut,
    /// Exponential ease-in-out.
    ExpoInOut,
    /// Damped harmonic settle: leaves for the target, overshoots once, and
    /// decays onto it. Unlike [`Easing::Bounce`] this is a real spring, so it
    /// reads as an object coming to rest rather than a value wobbling.
    Spring {
        /// Exponential decay rate of the overshoot, in units of the segment.
        damping: f32,
        /// Oscillation rate, in radians over the segment.
        frequency: f32,
    },
    /// Damped harmonic settle with initial velocity: preserves kinetic continuity
    /// when interrupting an ongoing motion.
    SpringV0 {
        /// Exponential decay rate of the overshoot, in units of the segment.
        damping: f32,
        /// Oscillation rate, in radians over the segment.
        frequency: f32,
        /// Normalized initial scalar velocity projected onto the travel direction.
        v0: f32,
    },
    /// Custom cubic-bezier easing with two control points.
    ///
    /// The four values are `(p1x, p1y, p2x, p2y)` where P1 and P2 are the
    /// control points of a cubic Bézier curve with P0=(0,0) and P3=(1,1).
    CubicBezier([f32; 4]),
}

/// Registry of canonical easing names and their human-readable labels.
///
/// Each pair maps a source identifier (e.g. `"ease-in-out"`) to a display
/// label (e.g. `"Ease In Out"`). Used for editor completion, UI presentation,
/// and the build layer's "supported values" diagnostic.
///
/// The ids here are what the GUI writes back into source, so they are the
/// hyphenated forms the hand-written corpus already uses — and a name only
/// exists as a legacy alias (`bounce`) or takes arguments (`spring(6, 9)`) gets
/// exactly one row, or none.
pub const EASING_REGISTRY: &[(&str, &str)] = &[
    ("linear", "Linear"),
    ("ease-in", "Ease In"),
    ("ease-out", "Ease Out"),
    ("ease-in-out", "Ease In Out"),
    ("expo-out", "Expo Out"),
    ("expo-in-out", "Expo In Out"),
    ("spring", "Spring"),
    ("bounce-in", "Bounce In"),
    ("elastic", "Elastic"),
    ("back", "Back"),
    ("expo", "Expo"),
];

/// Default cubic-bezier control points that approximate `EaseInOut`.
pub const DEFAULT_CUSTOM_EASING: [f32; 4] = [0.42, 0.0, 0.58, 1.0];

/// Default spring parameters: about one and a half visible oscillations and a
/// ~13% first overshoot, which is the settle motion that reads as deliberate
/// rather than cartoonish.
pub const DEFAULT_SPRING: [f32; 2] = [6.0, 9.0];

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
        Easing::ExpoOut => {
            if t >= 1.0 {
                1.0
            } else {
                1.0 - 2.0_f32.powf(-10.0 * t)
            }
        },
        Easing::ExpoInOut => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else if t < 0.5 {
                0.5 * 2.0_f32.powf(20.0 * t - 10.0)
            } else {
                1.0 - 0.5 * 2.0_f32.powf(10.0 - 20.0 * t)
            }
        },
        // 1 - e^(-d t) cos(f t): starts at 0, leaves at full speed, and rings
        // down onto 1. The endpoint is pinned because the residual is only
        // asymptotically zero, and a keyframe that lands 0.2% off its target
        // moves the resting composition.
        Easing::Spring { damping, frequency } => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                1.0 - (-damping * t).exp() * (frequency * t).cos()
            }
        },
        Easing::SpringV0 { damping, frequency, v0 } => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                let decay = (-damping * t).exp();
                let b = if frequency.abs() > 1e-4 {
                    (damping - v0) / frequency
                } else {
                    0.0
                };
                1.0 - decay * ((frequency * t).cos() + b * (frequency * t).sin())
            }
        },
        Easing::CubicBezier(cp) => evaluate_cubic_bezier(t, cp),
    }
}

/// Closed-form velocity derivative x'(t) for an easing curve.
pub fn easing_derivative(progress: f32, easing: Easing) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => 1.0,
        Easing::EaseIn => 2.0 * t,
        Easing::EaseOut => 2.0 * (1.0 - t),
        Easing::EaseInOut => {
            if t < 0.5 {
                4.0 * t
            } else {
                4.0 * (1.0 - t)
            }
        },
        Easing::Expo => {
            if t <= 0.0 {
                0.0
            } else {
                10.0 * 2.0_f32.ln() * 2.0_f32.powf(10.0 * (t - 1.0))
            }
        },
        Easing::ExpoOut => {
            if t >= 1.0 {
                0.0
            } else {
                10.0 * 2.0_f32.ln() * 2.0_f32.powf(-10.0 * t)
            }
        },
        Easing::ExpoInOut => {
            let ln2 = 2.0_f32.ln();
            if t <= 0.0 || t >= 1.0 {
                0.0
            } else if t < 0.5 {
                10.0 * ln2 * 2.0_f32.powf(20.0 * t - 10.0)
            } else {
                10.0 * ln2 * 2.0_f32.powf(10.0 - 20.0 * t)
            }
        },
        Easing::Back => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            3.0 * c3 * t * t - 2.0 * c1 * t
        },
        Easing::Spring { damping, frequency } => {
            let decay = (-damping * t).exp();
            decay * (damping * (frequency * t).cos() + frequency * (frequency * t).sin())
        },
        Easing::SpringV0 { damping, frequency, v0 } => {
            let decay = (-damping * t).exp();
            let b = if frequency.abs() > 1e-4 {
                (damping - v0) / frequency
            } else {
                0.0
            };
            let cos_term = (frequency * t).cos();
            let sin_term = (frequency * t).sin();
            decay * (v0 * cos_term + (damping * b + frequency) * sin_term)
        },
        // Numerical central difference for curves with piecewise or binary-search definitions
        Easing::Bounce | Easing::Elastic | Easing::CubicBezier(_) => {
            let eps = 1e-4;
            let t0 = (t - eps).max(0.0);
            let t1 = (t + eps).min(1.0);
            if (t1 - t0) > 0.0 {
                (apply_easing(t1, easing) - apply_easing(t0, easing)) / (t1 - t0)
            } else {
                1.0
            }
        },
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
/// Names that take arguments (`cubic-bezier`, `spring`) resolve to their
/// defaults here; see [`parse_easing_call`] for the argument form.
pub fn parse_easing_name(raw: &str) -> Option<Easing> {
    match raw {
        "ease-in" | "easein" => Some(Easing::EaseIn),
        "ease-out" | "easeout" => Some(Easing::EaseOut),
        "ease-in-out" | "easeinout" => Some(Easing::EaseInOut),
        "expo-out" | "expoout" => Some(Easing::ExpoOut),
        "expo-in-out" | "expoinout" => Some(Easing::ExpoInOut),
        "spring" => Some(Easing::Spring {
            damping: DEFAULT_SPRING[0],
            frequency: DEFAULT_SPRING[1],
        }),
        // `bounce-in` is the canonical name; `bounce` stays accepted because
        // years of source use it. See the `Bounce` variant doc for why the
        // older name is a trap.
        "bounce-in" | "bouncein" | "bounce" => Some(Easing::Bounce),
        "elastic" => Some(Easing::Elastic),
        "back" => Some(Easing::Back),
        "expo" => Some(Easing::Expo),
        "linear" => Some(Easing::Linear),
        "custom" | "cubic-bezier" | "cubicbezier" => {
            Some(Easing::CubicBezier(DEFAULT_CUSTOM_EASING))
        },
        _ => None,
    }
}

/// Parse a parameterized easing call: `cubic-bezier(0.16, 1, 0.3, 1)` or
/// `spring(6, 9)`.
///
/// This is the only place that knows which easing names take arguments and how
/// many, so the editor, the parser and the build layer cannot each invent their
/// own arity. Anything it rejects — an unknown name, or a known name with the
/// wrong number of arguments — stays in the modifier list so the build layer
/// can report it; the parse layer has no diagnostics.
pub fn parse_easing_call(name: &str, args: &[f64]) -> Option<Easing> {
    match name {
        "cubic-bezier" | "cubicbezier" | "custom" if args.len() == 4 => {
            let cp = args.iter().map(|v| *v as f32).collect::<Vec<_>>();
            Some(Easing::CubicBezier([cp[0], cp[1], cp[2], cp[3]]))
        },
        "spring" if matches!(args.len(), 0..=2) => {
            let damping = args.first().copied().unwrap_or(f64::from(DEFAULT_SPRING[0])) as f32;
            let frequency = args.get(1).copied().unwrap_or(f64::from(DEFAULT_SPRING[1])) as f32;
            Some(Easing::Spring { damping, frequency })
        },
        "spring-v0" | "spring_v0" if matches!(args.len(), 1..=3) => {
            let damping = args.first().copied().unwrap_or(f64::from(DEFAULT_SPRING[0])) as f32;
            let frequency = args.get(1).copied().unwrap_or(f64::from(DEFAULT_SPRING[1])) as f32;
            let v0 = args.get(2).copied().unwrap_or(0.0) as f32;
            Some(Easing::SpringV0 { damping, frequency, v0 })
        },
        _ => None,
    }
}

/// Whether an easing name expects arguments, for the build layer's diagnostic.
pub fn easing_expects_args(name: &str) -> Option<&'static str> {
    match name {
        "cubic-bezier" | "cubicbezier" => Some("cubic-bezier(x1, y1, x2, y2)"),
        "spring" => Some("spring(damping, frequency)"),
        "spring-v0" | "spring_v0" => Some("spring-v0(damping, frequency, v0)"),
        _ => None,
    }
}

/// Resolve an easing from the value the source actually wrote: either a bare
/// name (`ease: expo-out`) or a parameterized call
/// (`ease: cubic-bezier(0.16, 1, 0.3, 1)`).
///
/// The parser and the build layer both go through here so neither can develop
/// its own idea of which names take arguments.
pub fn parse_easing_expr(value: &crate::ast::Expr) -> Option<Easing> {
    use crate::ast::Expr;
    match value {
        Expr::Ident(raw) => parse_easing_name(raw),
        Expr::Call(name, args) => numeric_args(args).and_then(|a| parse_easing_call(name, &a)),
        _ => None,
    }
}

/// Literal number arguments of an easing call.
///
/// `-0.5` is accepted because the tokenizer produces a negation of a literal
/// rather than a negative literal, and bezier control points go negative.
fn numeric_args(args: &[crate::ast::Expr]) -> Option<Vec<f64>> {
    use crate::ast::{Expr, UnaryOp};
    args.iter()
        .map(|arg| match arg {
            Expr::Num(v) => Some(*v),
            Expr::Unary(UnaryOp::Neg, inner) => match inner.as_ref() {
                Expr::Num(v) => Some(-v),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The modifier value that parses back to this easing — an identifier for the
/// named curves, a call for the parameterized ones.
///
/// Lives here so an editor writing an easing into an AST cannot invent its own
/// name table (the GUI's did, and it dropped custom curves onto `linear`).
pub fn easing_to_expr(easing: Easing) -> crate::ast::Expr {
    use crate::ast::Expr;
    match easing {
        Easing::CubicBezier(cp) => Expr::Call(
            "cubic-bezier".to_string(),
            cp.iter().map(|v| Expr::Num(f64::from(*v))).collect(),
        ),
        Easing::Spring { damping, frequency } => Expr::Call(
            "spring".to_string(),
            vec![
                Expr::Num(f64::from(damping)),
                Expr::Num(f64::from(frequency)),
            ],
        ),
        Easing::SpringV0 { damping, frequency, v0 } => Expr::Call(
            "spring-v0".to_string(),
            vec![
                Expr::Num(f64::from(damping)),
                Expr::Num(f64::from(frequency)),
                Expr::Num(f64::from(v0)),
            ],
        ),
        other => Expr::Ident(easing_source_form(other)),
    }
}

/// Render an easing back into the source form that parses to it.
///
/// Used by the inspector's curve readout and by anything that has to show or
/// write an easing it did not parse itself.
pub fn easing_source_form(easing: Easing) -> String {
    match easing {
        Easing::Linear => "linear".to_string(),
        Easing::EaseIn => "ease-in".to_string(),
        Easing::EaseOut => "ease-out".to_string(),
        Easing::EaseInOut => "ease-in-out".to_string(),
        Easing::ExpoOut => "expo-out".to_string(),
        Easing::ExpoInOut => "expo-in-out".to_string(),
        Easing::Bounce => "bounce-in".to_string(),
        Easing::Elastic => "elastic".to_string(),
        Easing::Back => "back".to_string(),
        Easing::Expo => "expo".to_string(),
        Easing::Spring { damping, frequency } => format!("spring({damping}, {frequency})"),
        Easing::SpringV0 { damping, frequency, v0 } => format!("spring-v0({damping}, {frequency}, {v0})"),
        Easing::CubicBezier(cp) => format_cubic_bezier(cp),
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
            Easing::ExpoOut,
            Easing::ExpoInOut,
            Easing::Spring {
                damping: DEFAULT_SPRING[0],
                frequency: DEFAULT_SPRING[1],
            },
            Easing::Spring {
                damping: 2.0,
                frequency: 24.0,
            },
            Easing::CubicBezier(DEFAULT_CUSTOM_EASING),
        ];
        for easing in curves {
            assert!(apply_easing(0.0, easing).abs() < 1e-6, "{easing:?} does not start at 0");
            assert!(
                (apply_easing(1.0, easing) - 1.0).abs() < 1e-3,
                "{easing:?} does not land on 1"
            );
        }
    }

    /// Whatever the library can render as source must parse back to the same
    /// curve, or the inspector's readout and the formatter drift from the plan.
    #[test]
    fn source_form_round_trips() {
        let curves = [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::Bounce,
            Easing::Elastic,
            Easing::Back,
            Easing::Expo,
            Easing::ExpoOut,
            Easing::ExpoInOut,
            Easing::Spring {
                damping: 6.0,
                frequency: 9.0,
            },
            Easing::SpringV0 {
                damping: 6.0,
                frequency: 9.0,
                v0: 1.5,
            },
            Easing::CubicBezier([0.16, 1.0, 0.3, 1.0]),
        ];
        for easing in curves {
            let form = easing_source_form(easing);
            let parsed = form.find('(').map_or_else(
                || parse_easing_name(&form),
                |i| {
                    let (name, rest) = form.split_at(i);
                    let args: Vec<f64> = rest
                        .trim_matches(|c| c == '(' || c == ')')
                        .split(',')
                        .filter_map(|v| v.trim().parse().ok())
                        .collect();
                    parse_easing_call(name, &args)
                },
            );
            assert_eq!(parsed, Some(easing), "{easing:?} rendered as {form:?}");
        }
    }

    /// A bezier with the wrong number of control points must be rejected so the
    /// build layer can say so, rather than quietly falling back to a default.
    #[test]
    fn parameterized_eases_check_their_arity() {
        assert!(parse_easing_call("cubic-bezier", &[0.16, 1.0, 0.3]).is_none());
        assert!(
            parse_easing_call("cubic-bezier", &[0.16, 1.0, 0.3, 1.0]).is_some(),
            "the four-point form is the whole reason this exists"
        );
        assert!(parse_easing_call("spring", &[6.0, 9.0]).is_some());
        assert!(parse_easing_call("spring", &[6.0, 9.0, 1.0]).is_none());
        assert!(parse_easing_call("spring-v0", &[6.0, 9.0, 1.0]).is_some());
        assert!(parse_easing_call("ease-out", &[1.0]).is_none());
        assert_eq!(
            parse_easing_call("cubic-bezier", &[0.16, 1.0, 0.3, 1.0]),
            Some(Easing::CubicBezier([0.16, 1.0, 0.3, 1.0]))
        );
    }
}
