//! Easing curves: the `Easing` enum, name registry, application, and parsing.

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
    /// Bounce easing, named `bounce-in` in source; it is not gravity (`bounce` action).
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
    /// Damped harmonic settle: overshoots once and decays onto the target.
    Spring {
        /// Exponential decay rate of the overshoot, in units of the segment.
        damping: f32,
        /// Oscillation rate, in radians over the segment.
        frequency: f32,
    },
    /// Damped harmonic settle with initial velocity, preserving kinetic continuity.
    SpringV0 {
        /// Exponential decay rate of the overshoot, in units of the segment.
        damping: f32,
        /// Oscillation rate, in radians over the segment.
        frequency: f32,
        /// Normalized initial scalar velocity projected onto the travel direction.
        v0: f32,
    },
    /// Custom cubic-bezier easing: `(p1x, p1y, p2x, p2y)` with P0=(0,0), P3=(1,1).
    CubicBezier([f32; 4]),
}

/// Registry of canonical easing names and human-readable labels, used for editor
/// completion, UI presentation, and the "supported values" diagnostic. The ids
/// are what the GUI writes back into source (hyphenated forms).
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
/// ~13% first overshoot — deliberate rather than cartoonish.
pub const DEFAULT_SPRING: [f32; 2] = [6.0, 9.0];

/// Apply an easing curve to a normalized progress value clamped to `[0.0, 1.0]`;
/// returns a value also in `[0.0, 1.0]`.
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
        // 1 - e^(-d t) cos(f t); endpoints pinned because the residual is asymptotic.
        Easing::Spring { damping, frequency } => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                1.0 - (-damping * t).exp() * (frequency * t).cos()
            }
        },
        Easing::SpringV0 {
            damping,
            frequency,
            v0,
        } => {
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
        Easing::SpringV0 {
            damping,
            frequency,
            v0,
        } => {
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
        // Numerical central difference for piecewise/binary-search curves.
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

/// Evaluate a cubic Bézier at x by binary search over `t`, then return `y(t)`.
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

/// Parse an easing name; accepts hyphenated and unhyphenated lowercase forms.
/// Argument-taking names resolve to defaults; see [`parse_easing_call`].
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
        // `bounce-in` is canonical; `bounce` stays accepted for old source.
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

/// Parse a parameterized easing call. The only place that knows argument arity;
/// rejected forms stay for the build layer to diagnose.
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
            Some(Easing::SpringV0 {
                damping,
                frequency,
                v0,
            })
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

/// Resolve an easing from the written value: a bare name (`ease: expo-out`) or a
/// call (`ease: cubic-bezier(...)`). Shared by parser and build layer so neither
/// invents its own argument rules.
pub fn parse_easing_expr(value: &crate::ast::Expr) -> Option<Easing> {
    use crate::ast::Expr;
    match value {
        Expr::Ident(raw) => parse_easing_name(raw),
        Expr::Call(name, args) => numeric_args(args).and_then(|a| parse_easing_call(name, &a)),
        _ => None,
    }
}

/// Literal number arguments of an easing call. `-0.5` is accepted because the
/// tokenizer produces a negation of a literal, not a negative literal.
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

/// The modifier value that parses back to this easing: an identifier for named
/// curves, a call for parameterized ones. Lives here so editors cannot invent
/// their own name table (the GUI's did, and dropped custom curves onto `linear`).
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
        Easing::SpringV0 {
            damping,
            frequency,
            v0,
        } => Expr::Call(
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

/// Render an easing into the source form that parses back to it, for the
/// inspector's curve readout and anything that writes an easing it did not parse.
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
        Easing::SpringV0 {
            damping,
            frequency,
            v0,
        } => format!("spring-v0({damping}, {frequency}, {v0})"),
        Easing::CubicBezier(cp) => format_cubic_bezier(cp),
    }
}

/// Format a cubic-bezier easing as a CSS-like string, e.g.
/// `"cubic-bezier(0.42, 0, 0.58, 1)"`.
pub fn format_cubic_bezier(cp: [f32; 4]) -> String {
    format!("cubic-bezier({:.2}, {:.2}, {:.2}, {:.2})", cp[0], cp[1], cp[2], cp[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry feeds GUI/LSP completion and this parser consumes `.amx`; a
    /// one-sided name means the editor offers an easing the engine rejects.
    #[test]
    fn every_registry_id_parses() {
        for (id, label) in EASING_REGISTRY {
            assert!(
                parse_easing_name(id).is_some(),
                "registry id {id:?} ({label:?}) does not parse"
            );
        }
    }

    /// Every curve must be anchored at both ends: the settle target is what the
    /// stage shows, so a curve landing anywhere but 1.0 moves the composition.
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

    /// Whatever the library renders as source must parse back to the same curve.
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
    /// build layer can say so, rather than falling back to a default.
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
