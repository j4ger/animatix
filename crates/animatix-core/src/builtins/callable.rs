//! Canonical names, typed parameters and documentation of standard functions.

use std::sync::LazyLock;

use crate::callable::{
    CallableDescriptor, CallableParam, CallablePhase, CallableType as T, DefaultValue as D,
};

/// Stable internal identity of standard timeline-producing operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum TimelineBuiltin {
    /// Fade an actor into view.
    FadeIn,
    /// Wipe an actor into view.
    WipeIn,
    /// Set an absolute motion offset.
    Move,
    /// Add a relative motion offset.
    Shift,
    /// Set rotation.
    Rotate,
    /// Set scale.
    Scale,
    /// Draw stroke and fill into view.
    DrawIn,
    /// Reveal content.
    RevealIn,
    /// Erase stroke and fill.
    DrawOut,
    /// Fade an actor out.
    FadeOut,
    /// Carry an actor into the following scene.
    Persist,
    /// Stop carrying an actor.
    Remove,
    /// Wipe content out.
    WipeOut,
    /// Reverse a reveal.
    RevealOut,
    /// Shake the actor.
    Shake,
    /// Pulse the actor.
    Pulse,
    /// Bounce the actor.
    Bounce,
    /// Fade and scale into place.
    SettleIn,
    /// Pop into place with overshoot.
    PopIn,
    /// Grow from an origin.
    GrowIn,
    /// Shrink toward an origin.
    ShrinkOut,
    /// Add a highlight.
    Highlight,
    /// Clear a highlight.
    Unhighlight,
    /// Exchange two actors' layout positions.
    Swap,
    /// Set a container's child order.
    Reorder,
}

impl TimelineBuiltin {
    /// Every built-in timeline operation, in catalog order.
    pub const ALL: [Self; 25] = [
        Self::FadeIn,
        Self::WipeIn,
        Self::Move,
        Self::Shift,
        Self::Rotate,
        Self::Scale,
        Self::DrawIn,
        Self::RevealIn,
        Self::DrawOut,
        Self::FadeOut,
        Self::Persist,
        Self::Remove,
        Self::WipeOut,
        Self::RevealOut,
        Self::Shake,
        Self::Pulse,
        Self::Bounce,
        Self::SettleIn,
        Self::PopIn,
        Self::GrowIn,
        Self::ShrinkOut,
        Self::Highlight,
        Self::Unhighlight,
        Self::Swap,
        Self::Reorder,
    ];

    /// Resolve a canonical unqualified name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.descriptor().name == name)
    }

    /// Read this operation's authoritative signature.
    pub fn descriptor(self) -> &'static CallableDescriptor {
        &CALLABLES[self as usize]
    }
}

/// Every standard callable. Tooling and execution consume this same table.
pub fn callables() -> &'static [CallableDescriptor] {
    &CALLABLES
}

/// Find a standard function by its canonical name or `std::`-qualified name.
pub fn callable(name: &str) -> Option<&'static CallableDescriptor> {
    let name = name.strip_prefix("std::").unwrap_or(name);
    CALLABLES.iter().find(|entry| entry.name == name)
}

fn param(name: &str, ty: T) -> CallableParam {
    CallableParam::required(name, ty)
}

fn opt(name: &str, ty: T) -> CallableParam {
    CallableParam::optional(name, ty)
}

fn descriptor(name: &str, params: Vec<CallableParam>, result: T) -> CallableDescriptor {
    let phase = if result == T::Timeline {
        CallablePhase::Build
    } else {
        CallablePhase::BuildAndFrame
    };
    CallableDescriptor {
        name: name.into(),
        namespace: "std".into(),
        params,
        return_type: result,
        phase,
        help: String::new(),
        category: "Calculation".into(),
    }
}

fn timing() -> Vec<CallableParam> {
    vec![
        opt("duration", T::Duration).with_default(D::Duration(0.0)),
        opt("ease", T::Easing),
        opt("delay", T::Duration).with_default(D::Duration(0.0)),
    ]
}

fn timeline(
    name: &str,
    category: &str,
    help: &str,
    extra: Vec<CallableParam>,
) -> CallableDescriptor {
    let mut params = if name == "swap" {
        vec![param("a", T::Actor), param("b", T::Actor)]
    } else {
        vec![opt("target", T::Actor)]
    };
    params.extend(extra);
    params.extend(timing());
    // Keep the alternate batch form at the end, so a second positional
    // argument never accidentally binds to `targets`.
    if name != "swap" {
        params.push(opt("targets", T::List(Box::new(T::Actor))));
    }
    CallableDescriptor {
        category: category.into(),
        help: help.into(),
        ..descriptor(name, params, T::Timeline)
    }
}

fn reveal_params() -> Vec<CallableParam> {
    vec![
        opt("by", T::Enum(vec!["char".into(), "word".into(), "line".into()])),
        opt("stagger", T::Union(vec![T::Duration, T::Num])),
        opt("offset_y", T::Num),
        opt("mask", T::Union(vec![T::Bool, T::Enum(vec!["baseline".into()])])),
    ]
}

fn timeline_catalog() -> Vec<CallableDescriptor> {
    let origin = || {
        T::Union(vec![
            T::Vec2,
            T::Enum(["center", "left", "right", "top", "bottom"].map(String::from).to_vec()),
        ])
    };
    vec![
        timeline("fade_in", "Entrance", "Fade from transparent to authored opacity.", vec![]),
        timeline("wipe_in", "Entrance", "Reveal vector stroke and fill.", vec![]),
        timeline(
            "move",
            "Motion",
            "Set local motion offset or follow a route.",
            vec![
                opt("to", T::Vec2),
                opt("along", T::List(Box::new(T::Any))),
                opt("orient", T::Bool),
                opt("anticipate", T::Duration),
            ],
        ),
        timeline(
            "shift",
            "Motion",
            "Add a relative local motion offset.",
            vec![param("by", T::Vec2), opt("anticipate", T::Duration)],
        ),
        timeline(
            "rotate",
            "Motion",
            "Add a relative rotation in radians.",
            vec![param("by", T::Num)],
        ),
        timeline(
            "scale",
            "Motion",
            "Multiply the authored scale by a factor.",
            vec![param("by", T::Num)],
        ),
        timeline(
            "draw_in",
            "Entrance",
            "Draw vector or split-text content into view.",
            reveal_params(),
        ),
        timeline("reveal_in", "Entrance", "Reveal vector or split-text content.", reveal_params()),
        timeline("draw_out", "Exit", "Erase stroke and fill.", vec![]),
        timeline("fade_out", "Exit", "Fade to transparent.", vec![]),
        timeline(
            "persist",
            "Persistence",
            "Carry a root or external actor into the next scene.",
            vec![],
        ),
        timeline("remove", "Persistence", "Fade out and stop carrying an actor.", vec![]),
        timeline("wipe_out", "Exit", "Wipe vector content out.", vec![]),
        timeline("reveal_out", "Exit", "Reverse a reveal.", vec![]),
        timeline(
            "shake",
            "Effects",
            "Shake with a horizontal oscillation.",
            vec![opt("intensity", T::Num), opt("frequency", T::Num)],
        ),
        timeline(
            "pulse",
            "Effects",
            "Pulse the actor's authored scale.",
            vec![opt("intensity", T::Num)],
        ),
        timeline(
            "bounce",
            "Effects",
            "Bounce with a decaying offset.",
            vec![opt("intensity", T::Num), opt("restitution", T::Num)],
        ),
        timeline("settle_in", "Entrance", "Fade and scale into place.", vec![]),
        timeline("pop_in", "Entrance", "Fade and scale into place with overshoot.", vec![]),
        timeline(
            "grow_in",
            "Entrance",
            "Grow from zero scale around an origin.",
            vec![opt("from", origin())],
        ),
        timeline(
            "shrink_out",
            "Exit",
            "Shrink toward an origin without fading.",
            vec![opt("to", origin())],
        ),
        timeline(
            "highlight",
            "Effects",
            "Add a highlight behind content.",
            vec![
                opt("color", T::Color),
                opt(
                    "blend",
                    T::Enum(
                        [
                            "difference",
                            "exclusion",
                            "normal",
                            "multiply",
                            "screen",
                            "overlay",
                        ]
                        .map(String::from)
                        .to_vec(),
                    ),
                ),
                opt("padding", T::Num),
                opt("radius", T::Num),
            ],
        ),
        timeline("unhighlight", "Effects", "Remove a highlight.", vec![]),
        timeline("swap", "Reorder", "Exchange two actors' layout positions.", vec![]),
        timeline(
            "reorder",
            "Reorder",
            "Set a container's child order.",
            vec![param("order", T::List(Box::new(T::Actor)))],
        ),
    ]
}

fn numeric_catalog() -> Vec<CallableDescriptor> {
    let mut out = Vec::new();
    for name in [
        "abs",
        "acos",
        "asin",
        "atan",
        "ceil",
        "cos",
        "deg",
        "deg_to_rad",
        "exp",
        "factorial",
        "floor",
        "fract",
        "ln",
        "log",
        "log10",
        "noise",
        "rad",
        "rad_to_deg",
        "round",
        "signum",
        "sin",
        "sqrt",
        "tan",
    ] {
        out.push(descriptor(name, vec![param("value", T::Num)], T::Num));
    }
    for name in [
        "atan2", "hypot", "max", "min", "pow", "rem", "step", "noise2",
    ] {
        out.push(descriptor(name, vec![param("a", T::Num), param("b", T::Num)], T::Num));
    }
    out.extend([
        descriptor(
            "smoothstep",
            vec![
                param("edge0", T::Num),
                param("edge1", T::Num),
                param("x", T::Num),
            ],
            T::Num,
        ),
        descriptor(
            "clamp",
            vec![
                param("value", T::Num),
                param("min", T::Num),
                param("max", T::Num),
            ],
            T::Num,
        ),
        descriptor(
            "lerp",
            vec![
                param("from", T::Num),
                param("to", T::Num),
                param("t", T::Num),
            ],
            T::Num,
        ),
        descriptor("fbm", vec![param("value", T::Num), param("octaves", T::Num)], T::Num),
        descriptor("seeded_rand", vec![param("seed", T::Num)], T::Num),
        descriptor("seeded_noise", vec![param("seed", T::Num), param("value", T::Num)], T::Num),
        descriptor(
            "seeded_noise2",
            vec![
                param("seed", T::Num),
                param("x", T::Num),
                param("y", T::Num),
            ],
            T::Num,
        ),
        descriptor(
            "seeded_fbm",
            vec![
                param("seed", T::Num),
                param("value", T::Num),
                param("octaves", T::Num),
            ],
            T::Num,
        ),
        CallableDescriptor {
            phase: CallablePhase::Build,
            ..descriptor("rand", vec![], T::Num)
        },
        descriptor("sum", vec![param("values", T::List(Box::new(T::Num)))], T::Num),
        descriptor("seconds", vec![param("value", T::Num)], T::Duration),
        descriptor("milliseconds", vec![param("value", T::Num)], T::Duration),
        descriptor("as_seconds", vec![param("value", T::Duration)], T::Num),
    ]);
    for name in [
        "ease_linear",
        "ease_in",
        "ease_out",
        "ease_in_out",
        "bounce_in",
        "spring_curve",
        "elastic",
        "back",
        "expo",
        "expo_out",
        "expo_in_out",
    ] {
        out.push(descriptor(name, vec![param("t", T::Num)], T::Num));
    }
    out
}

fn utility_catalog() -> Vec<CallableDescriptor> {
    let mut out = vec![
        descriptor("vec2", vec![param("x", T::Num), param("y", T::Num)], T::Vec2),
        descriptor(
            "vec3",
            vec![param("x", T::Num), param("y", T::Num), param("z", T::Num)],
            T::Vec3,
        ),
        descriptor(
            "vec4",
            vec![
                param("x", T::Num),
                param("y", T::Num),
                param("z", T::Num),
                param("w", T::Num),
            ],
            T::Vec4,
        ),
        descriptor(
            "rgb",
            vec![param("r", T::Num), param("g", T::Num), param("b", T::Num)],
            T::Color,
        ),
        descriptor(
            "rgba",
            vec![
                param("r", T::Num),
                param("g", T::Num),
                param("b", T::Num),
                param("a", T::Num),
            ],
            T::Color,
        ),
        descriptor(
            "hsv",
            vec![param("h", T::Num), param("s", T::Num), param("v", T::Num)],
            T::Color,
        ),
        descriptor(
            "hsla",
            vec![
                param("h", T::Num),
                param("s", T::Num),
                param("l", T::Num),
                param("a", T::Num),
            ],
            T::Color,
        ),
        descriptor("is_animating", vec![param("property", T::Any)], T::Bool),
        descriptor(
            "sum_range",
            vec![
                param(
                    "function",
                    T::Function {
                        params: vec![("value".into(), T::Num)],
                        ret: Box::new(T::Num),
                    },
                ),
                param("start", T::Num),
                param("end", T::Num),
            ],
            T::Num,
        ),
        descriptor(
            "list_set",
            vec![
                param("list", T::List(Box::new(T::Any))),
                param("index", T::Num),
                param("value", T::Any),
            ],
            T::List(Box::new(T::Any)),
        ),
        descriptor(
            "list_swap",
            vec![
                param("list", T::List(Box::new(T::Any))),
                param("i", T::Num),
                param("j", T::Num),
            ],
            T::List(Box::new(T::Any)),
        ),
        descriptor(
            "curve_at",
            vec![
                param("curve", T::List(Box::new(T::Num))),
                param("t", T::Num),
            ],
            T::Num,
        ),
        descriptor(
            "curve_smooth",
            vec![
                param("curve", T::List(Box::new(T::Num))),
                param("t", T::Num),
                param("tau", T::Num),
            ],
            T::Num,
        ),
        descriptor(
            "stretch",
            vec![param("animation", T::Timeline), param("factor", T::Num)],
            T::Timeline,
        ),
        descriptor(
            "fit_duration",
            vec![
                param("animation", T::Timeline),
                param("duration", T::Duration),
            ],
            T::Timeline,
        ),
    ];
    out.extend(construction_catalog());
    out
}

fn construction_catalog() -> Vec<CallableDescriptor> {
    let mut out = Vec::new();
    out.push(descriptor(
        "lerp_vec2",
        vec![
            param("from", T::Vec2),
            param("to", T::Vec2),
            param("t", T::Num),
        ],
        T::Vec2,
    ));
    for name in ["lerp_color", "lerp_color_oklab"] {
        out.push(descriptor(
            name,
            vec![
                param("from", T::Union(vec![T::Color, T::Vec3])),
                param("to", T::Union(vec![T::Color, T::Vec3])),
                param("t", T::Num),
            ],
            T::Color,
        ));
    }
    let mut args = opt("args", T::Any);
    args.variadic = true;
    out.push(descriptor("format", vec![param("template", T::Str), args], T::Str));
    out.push(descriptor(
        "linear",
        vec![
            param("angle", T::Num),
            param("stops", T::List(Box::new(T::Any))),
        ],
        T::Any,
    ));
    out.push(descriptor(
        "radial",
        vec![
            param("center", T::Vec2),
            param("radius", T::Num),
            param("stops", T::List(Box::new(T::Any))),
        ],
        T::Any,
    ));
    out.push(descriptor(
        "sweep",
        vec![
            param("angle", T::Num),
            param("stops", T::List(Box::new(T::Any))),
            opt("center", T::Vec2),
        ],
        T::Any,
    ));
    for name in ["move_to", "line_to"] {
        out.push(descriptor(name, vec![param("x", T::Num), param("y", T::Num)], T::Any));
    }
    out.push(descriptor(
        "curve_to",
        ["x1", "y1", "x2", "y2", "x", "y"].map(|n| param(n, T::Num)).to_vec(),
        T::Any,
    ));
    out.push(descriptor(
        "quad_to",
        ["x1", "y1", "x", "y"].map(|n| param(n, T::Num)).to_vec(),
        T::Any,
    ));
    out.push(descriptor("close", vec![], T::Any));
    out.push(descriptor(
        "spring",
        vec![
            opt("damping", T::Num).with_default(D::Num(6.0)),
            opt("frequency", T::Num).with_default(D::Num(9.0)),
        ],
        T::Easing,
    ));
    out.push(descriptor(
        "spring_v0",
        vec![
            opt("damping", T::Num).with_default(D::Num(6.0)),
            opt("frequency", T::Num).with_default(D::Num(9.0)),
            opt("v0", T::Num).with_default(D::Num(0.0)),
        ],
        T::Easing,
    ));
    out.push(descriptor(
        "cubic_bezier",
        ["x1", "y1", "x2", "y2"].map(|n| param(n, T::Num)).to_vec(),
        T::Easing,
    ));
    out
}

static CALLABLES: LazyLock<Vec<CallableDescriptor>> = LazyLock::new(|| {
    let mut out = timeline_catalog();
    out.extend(numeric_catalog());
    out.extend(utility_catalog());
    out
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_names_are_unique_and_canonical() {
        let mut names = std::collections::HashSet::new();
        for entry in callables() {
            entry
                .validate()
                .unwrap_or_else(|error| panic!("{}: {error}", entry.qualified_name()));
            assert!(names.insert(entry.qualified_name()));
            assert!(!entry.name.contains('-'));
            let mut params = std::collections::HashSet::new();
            assert!(entry.params.iter().all(|param| params.insert(&param.name)));
        }
        for kind in TimelineBuiltin::ALL {
            assert_eq!(kind.descriptor().return_type, T::Timeline);
            assert_eq!(TimelineBuiltin::from_name(&kind.descriptor().name), Some(kind));
        }
        assert!(TimelineBuiltin::from_name("fade-in").is_none());
    }
}
