use super::morph::{MorphOptions, MorphStrategy};
use super::plot::FuncBlendMode;
use crate::ast::{Expr, Modifier, Stmt};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::*;

pub(crate) fn sequence_stmt_kind(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Action(..) => "action",
        Stmt::Assignment { .. } => "assignment",
        Stmt::Sequence { .. } => "sequence",
        Stmt::Stagger { .. } => "stagger",
        Stmt::LetDecl { .. } => "let declaration",
        Stmt::TypeAlias { .. } => "type alias",
        Stmt::ActorDecl { .. } => "actor declaration",
        Stmt::Import { .. } => "import",
        Stmt::Keyframe { .. } => "keyframe",
        Stmt::RelativeKeyframe { .. } => "relative keyframe",
        Stmt::Always { .. } => "always block",
        Stmt::ReactiveBinding { .. } => "reactive binding",
        Stmt::Conditional { .. } => "conditional",
        Stmt::Match { .. } => "match",
        Stmt::ForLoop { .. } => "for loop",
        Stmt::ComponentDef(..) => "component definition",
        Stmt::FnDecl { .. } => "function",
        Stmt::Block { .. } => "block",
        Stmt::Return { .. } => "return",
        Stmt::Expr(..) => "expression",
        Stmt::Config { .. } => "config block",
        Stmt::Comment(..) => "comment",
        Stmt::Scene { .. } => "scene declaration",
        Stmt::Play { .. } => "play statement",
    }
}

pub(crate) fn push_unsupported_stagger_statement_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    kind: &str,
) {
    let message = if kind == "actor declaration" {
        "Stagger blocks do not support actor declarations. Declare actors before the composition block, then reference them inside.".to_string()
    } else {
        format!("Stagger blocks support only actions and assignments; '{kind}' is not supported.")
    };
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::UnsupportedStaggerStatement,
            DiagnosticPhase::Build,
            message,
        )
        .with_subject("stagger"),
    );
}

pub(crate) fn config_string_value(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Str(value) | Expr::Ident(value) => Some(value.clone()),
        _ => None,
    }
}

pub(crate) fn push_unknown_target_path_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    target_key: &str,
    suggestion: Option<&str>,
) {
    let hint = suggestion
        .map(|candidate| format!(" Did you mean '{candidate}'?"))
        .unwrap_or_default();
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::UnknownTargetPath,
            DiagnosticPhase::Build,
            format!(
                "Assignment target '{target_key}' does not resolve to a declared actor or nested label; ignoring this assignment.{hint}"
            ),
        )
        .with_subject(subject),
    );
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ParsedTimingModifiers {
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub easing: Easing,
    /// True when the author passed `ease:` explicitly. Preset entrances carry
    /// their own arrival curve and only fall back to it when this is false.
    pub ease_authored: bool,
    /// Length of the counter-move a motion verb should insert before its
    /// action (`[anticipate: 80ms]`); 0 = no anticipation.
    pub anticipate_ms: f64,
    pub morph_options: MorphOptions,
    pub func_blend_mode: FuncBlendMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModifierHost {
    Action,
    Assignment,
    Text,
    Typst,
    Code,
    ActorDeclaration,
}

impl ModifierHost {
    fn display_name(self) -> &'static str {
        match self {
            ModifierHost::Action => "action",
            ModifierHost::Assignment => "assignment",
            ModifierHost::Text => "text declaration",
            ModifierHost::Typst => "typst declaration",
            ModifierHost::Code => "code declaration",
            ModifierHost::ActorDeclaration => "actor declaration",
        }
    }

    fn supports_morph_modifiers(self) -> bool {
        matches!(
            self,
            ModifierHost::Text
                | ModifierHost::Typst
                | ModifierHost::Code
                | ModifierHost::ActorDeclaration
        )
    }
}

/// Easing names have one source of truth: `animatix_syntax::easing`, which
/// the engine re-exports as `crate::easing`. This used to carry its own copy
/// of the table, and the two had already drifted (the engine's was missing
/// `custom`). Re-export instead of duplicating.
pub use crate::easing::parse_easing_name;

/// Whether the action named by `subject` declares this modifier key.
///
/// The timing parser used to keep its own list of effect keys
/// (`intensity`, `frequency`, `color`, …) and went stale the first time an
/// action grew a parameter — `bounce [restitution: …]` warned despite being
/// legal. The signatures are the single source, and `validate_action_modifiers`
/// is what reports a real typo.
fn action_declares_modifier(subject: Option<&str>, name: &str) -> bool {
    let Some(verb) = subject else {
        return false;
    };
    super::actions::get_action_signatures()
        .into_iter()
        .find(|sig| sig.name == verb)
        .is_some_and(|sig| sig.modifiers.iter().any(|p| p.name == name))
}

/// Explain an `ease:` value the build layer could not resolve.
///
/// The supported-name list is read from the registry rather than written out
/// here, so adding a curve cannot leave this message behind — the previous
/// version of this string had already gone stale.
fn ease_value_diagnostic(value: &Expr, host: ModifierHost) -> String {
    let supported = EASING_REGISTRY.iter().map(|(id, _)| *id).collect::<Vec<_>>().join(", ");
    match value {
        Expr::Call(name, _) => match easing_expects_args(name) {
            Some(form) => {
                format!("Ease '{name}' on {} expects the form {form}.", host.display_name())
            },
            None => format!(
                "Unknown easing '{name}' on {}; supported values are {supported}, or cubic-bezier(x1, y1, x2, y2).",
                host.display_name()
            ),
        },
        Expr::Ident(raw) => format!(
            "Unsupported ease value '{raw}' on {}; supported values are {supported}, or a cubic-bezier(…) / spring(…) call.",
            host.display_name()
        ),
        other => format!(
            "Unsupported ease modifier value {other:?} on {}; expected an easing name or a cubic-bezier(…) / spring(…) call.",
            host.display_name()
        ),
    }
}

pub(crate) fn parse_duration_literal(raw: &str) -> Option<f64> {
    if let Some(ms) = raw.strip_suffix("ms") {
        ms.parse::<f64>().ok()
    } else if let Some(seconds) = raw.strip_suffix('s') {
        seconds.parse::<f64>().ok().map(|seconds| seconds * 1000.0)
    } else {
        None
    }
}

pub(crate) fn push_conflicting_modifier_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    logical_name: &str,
    host: ModifierHost,
    subject: Option<&str>,
) {
    push_modifier_diagnostic(
        diagnostics,
        DiagnosticCode::ConflictingModifierKey,
        format!(
            "Conflicting '{logical_name}' modifiers on {}; using the last value provided.",
            host.display_name()
        ),
        subject,
    );
}

pub(crate) fn has_non_default_morph_options(options: MorphOptions) -> bool {
    options != MorphOptions::default()
}

pub(crate) fn push_modifier_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    code: DiagnosticCode,
    message: String,
    subject: Option<&str>,
) {
    let diagnostic = Diagnostic::warning(code, DiagnosticPhase::Build, message);
    diagnostics.push(match subject {
        Some(subject) => diagnostic.with_subject(subject),
        None => diagnostic,
    });
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StaggerOrigin {
    Center,
    TopLeft,
    Point([f64; 2]),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StaggerMetric {
    #[default]
    Euclidean,
    Manhattan,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedStaggerModifiers {
    pub interval_ms: f64,
    pub from: Option<StaggerOrigin>,
    pub metric: StaggerMetric,
}

pub(crate) fn parse_stagger_modifiers(
    modifiers: &[Modifier],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ParsedStaggerModifiers> {
    let mut interval_ms = None;
    let mut saw_interval = false;
    let mut from_origin = None;
    let mut metric = StaggerMetric::default();

    for modifier in modifiers {
        match modifier.name.as_deref() {
            None => {
                if let Expr::Ident(raw) = &modifier.value {
                    if let Some(parsed_ms) = parse_duration_literal(raw) {
                        if saw_interval {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "duration-shorthand",
                                ModifierHost::Action,
                                Some("stagger"),
                            );
                        }
                        interval_ms = Some(parsed_ms);
                        saw_interval = true;
                    } else {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported stagger interval '{raw}'; expected a time literal such as 120ms or 1s."
                            ),
                            Some("stagger"),
                        );
                    }
                } else {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported stagger interval modifier {:?}; expected a time literal such as 120ms or 1s.",
                            modifier.value
                        ),
                        Some("stagger"),
                    );
                }
            },
            Some("each") => {
                if let Expr::Ident(raw) = &modifier.value {
                    if let Some(parsed_ms) = parse_duration_literal(raw) {
                        if saw_interval {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "each",
                                ModifierHost::Action,
                                Some("stagger"),
                            );
                        }
                        interval_ms = Some(parsed_ms);
                        saw_interval = true;
                    } else {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported stagger each value '{raw}'; expected a time literal such as 120ms or 1s."
                            ),
                            Some("stagger"),
                        );
                    }
                } else {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported stagger each modifier {:?}; expected a time literal such as 120ms or 1s.",
                            modifier.value
                        ),
                        Some("stagger"),
                    );
                }
            },
            Some("from") => {
                match &modifier.value {
                    Expr::Ident(name) | Expr::Str(name) if name == "center" => {
                        from_origin = Some(StaggerOrigin::Center);
                    },
                    Expr::Ident(name) | Expr::Str(name) if name == "top-left" || name == "top_left" => {
                        from_origin = Some(StaggerOrigin::TopLeft);
                    },
                    Expr::Tuple(items) if items.len() == 2 => {
                        match (&items[0], &items[1]) {
                            (Expr::Num(x), Expr::Num(y)) => {
                                from_origin = Some(StaggerOrigin::Point([*x, *y]));
                            },
                            _ => {
                                push_modifier_diagnostic(
                                    diagnostics,
                                    DiagnosticCode::InvalidModifierValue,
                                    "Stagger from coordinate tuple expects (x, y) numbers.".to_string(),
                                    Some("stagger"),
                                );
                            }
                        }
                    },
                    _ => {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported stagger from value '{:?}'; expected 'center', 'top-left', or (x, y).",
                                modifier.value
                            ),
                            Some("stagger"),
                        );
                    }
                }
            },
            Some("metric") => {
                match &modifier.value {
                    Expr::Ident(name) | Expr::Str(name) if name == "euclidean" => {
                        metric = StaggerMetric::Euclidean;
                    },
                    Expr::Ident(name) | Expr::Str(name) if name == "manhattan" => {
                        metric = StaggerMetric::Manhattan;
                    },
                    _ => {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported stagger metric '{:?}'; expected 'euclidean' or 'manhattan'.",
                                modifier.value
                            ),
                            Some("stagger"),
                        );
                    }
                }
            },
            Some(other) => push_modifier_diagnostic(
                diagnostics,
                DiagnosticCode::UnsupportedModifierKey,
                format!(
                    "Unsupported modifier key '{other}' on stagger; only duration shorthand, 'each', 'from', or 'metric' are supported."
                ),
                Some("stagger"),
            ),
        }
    }

    if interval_ms.is_none() {
        push_modifier_diagnostic(
            diagnostics,
            DiagnosticCode::InvalidModifierValue,
            "Stagger blocks require an interval such as [150ms] or [each: 150ms].".to_string(),
            Some("stagger"),
        );
    }

    interval_ms.map(|interval| ParsedStaggerModifiers {
        interval_ms: interval,
        from: from_origin,
        metric,
    })
}

/// The curve an uneased timed statement gets, chosen by what it is doing.
///
/// 72% of the shipped corpus's timed statements carry no ease and so animate
/// linearly, and linear is the one curve the motion-design craft tables rule out
/// for anything but a continuous loop ("the eye forgives a slow start far less
/// than a slow end"). So the default is now chosen by role: arrivals
/// decelerate, departures accelerate, and a move between two on-screen
/// positions does both. An explicit `ease:` always wins — including
/// `ease: linear`, which still means linear.
///
/// Assignments deliberately keep the old linear default: the verb is not
/// available there and cannot be, because an action that expands into
/// assignments hands them its own timing modifiers. Widening the default to
/// assignments needs that expansion to carry the resolved easing, which is a
/// separate change.
fn default_easing_for(host: ModifierHost, subject: Option<&str>) -> Easing {
    if host != ModifierHost::Action {
        // Assignments keep the linear default on purpose: an action that expands
        // into them hands them its own timing modifiers, so easing the
        // assignment path would double-ease effects that already bake their
        // curve into keyframes (`bounce`). Write `ease:` to opt in.
        return Easing::Linear;
    }
    match subject.unwrap_or_default() {
        "fade-in" | "wipe-in" | "reveal-in" | "draw-in" | "settle-in" | "pop-in" => Easing::ExpoOut,
        "fade-out" | "wipe-out" | "reveal-out" | "draw-out" | "remove" => Easing::EaseIn,
        // Oscillating effects are continuous loops: their shape is the effect,
        // so easing them would soften it. Everything else travels, and a
        // travel that starts and ends at rest reads as intentional.
        "shake" | "pulse" | "bounce" | "highlight" | "unhighlight" => Easing::Linear,
        _ => Easing::EaseInOut,
    }
}

pub(crate) fn parse_timing_modifiers(
    modifiers: &[Modifier],
    host: ModifierHost,
    subject: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> ParsedTimingModifiers {
    let mut parsed = ParsedTimingModifiers {
        duration_ms: 0.0,
        delay_ms: 0.0,
        easing: Easing::Linear,
        ease_authored: false,
        anticipate_ms: 0.0,
        morph_options: MorphOptions::default(),
        func_blend_mode: FuncBlendMode::Output,
    };
    let mut saw_duration = false;
    let mut saw_delay = false;
    let mut saw_ease = false;
    let mut saw_strategy = false;
    let mut saw_path_arc = false;
    let mut saw_stretch = false;
    let mut saw_func_blend = false;

    for modifier in modifiers {
        match modifier.name.as_deref() {
            Some("delay") => match &modifier.value {
                Expr::Ident(raw) => {
                    if let Some(delay_ms) = parse_duration_literal(raw) {
                        if saw_delay {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "delay",
                                host,
                                subject,
                            );
                        }
                        parsed.delay_ms = delay_ms;
                        saw_delay = true;
                    } else {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported delay value '{raw}' on {}; expected a time literal such as 120ms or 1s.",
                                host.display_name()
                            ),
                            subject,
                        );
                    }
                },
                other => push_modifier_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    format!(
                        "Unsupported delay modifier value {:?} on {}; expected a time literal such as 120ms or 1s.",
                        other,
                        host.display_name()
                    ),
                    subject,
                ),
            },
            Some("anticipate") => match &modifier.value {
                Expr::Ident(raw) => match parse_duration_literal(raw) {
                    Some(ms) => parsed.anticipate_ms = ms,
                    None => push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported anticipate value '{raw}' on {}; expected a time literal such as 80ms or 0.1s.",
                            host.display_name()
                        ),
                        subject,
                    ),
                },
                other => push_modifier_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    format!(
                        "Unsupported anticipate modifier value {other:?} on {}; expected a time literal such as 80ms or 0.1s.",
                        host.display_name()
                    ),
                    subject,
                ),
            },
            Some("ease") => {
                if let Some(easing) = parse_easing_expr(&modifier.value) {
                    if saw_ease {
                        push_conflicting_modifier_diagnostic(diagnostics, "ease", host, subject);
                    }
                    parsed.easing = easing;
                    parsed.ease_authored = true;
                    saw_ease = true;
                } else {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        ease_value_diagnostic(&modifier.value, host),
                        subject,
                    );
                }
            },
            Some("strategy") => {
                if !host.supports_morph_modifiers() {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::UnsupportedModifierKey,
                        format!(
                            "Unsupported modifier key 'strategy' on {}; morph-only keys are limited to path-morphing declarations.",
                            host.display_name()
                        ),
                        subject,
                    );
                    continue;
                }

                match &modifier.value {
                    Expr::Ident(raw) => {
                        if saw_strategy {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "strategy",
                                host,
                                subject,
                            );
                        }
                        match raw.as_str() {
                            "auto" => parsed.morph_options.strategy = MorphStrategy::Auto,
                            "match" => parsed.morph_options.strategy = MorphStrategy::Match,
                            "fade" => parsed.morph_options.strategy = MorphStrategy::Fade,
                            "nearest" => parsed.morph_options.strategy = MorphStrategy::Nearest,
                            other => push_modifier_diagnostic(
                                diagnostics,
                                DiagnosticCode::InvalidModifierValue,
                                format!(
                                    "Unsupported strategy value '{other}' on {}; supported values are auto, match, fade, and nearest.",
                                    host.display_name()
                                ),
                                subject,
                            ),
                        }
                        saw_strategy = true;
                    },
                    other => push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported strategy modifier value {:?} on {}; expected an identifier such as auto, match, fade, or nearest.",
                            other,
                            host.display_name()
                        ),
                        subject,
                    ),
                }
            },
            Some("path_arc") => {
                if !host.supports_morph_modifiers() {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::UnsupportedModifierKey,
                        format!(
                            "Unsupported modifier key 'path_arc' on {}; morph-only keys are limited to path-morphing declarations.",
                            host.display_name()
                        ),
                        subject,
                    );
                    continue;
                }

                let parsed_arc = match &modifier.value {
                    Expr::Num(value) => Some(*value),
                    Expr::Ident(raw) => raw.parse::<f64>().ok(),
                    _ => None,
                };

                if let Some(path_arc) = parsed_arc {
                    if saw_path_arc {
                        push_conflicting_modifier_diagnostic(
                            diagnostics,
                            "path_arc",
                            host,
                            subject,
                        );
                    }
                    parsed.morph_options.path_arc = path_arc;
                    saw_path_arc = true;
                } else {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported path_arc value on {}; expected a numeric radians hint.",
                            host.display_name()
                        ),
                        subject,
                    );
                }
            },
            Some("stretch") => {
                if !host.supports_morph_modifiers() {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::UnsupportedModifierKey,
                        format!(
                            "Unsupported modifier key 'stretch' on {}; morph-only keys are limited to path-morphing declarations.",
                            host.display_name()
                        ),
                        subject,
                    );
                    continue;
                }

                match &modifier.value {
                    Expr::Bool(value) => {
                        if saw_stretch {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "stretch",
                                host,
                                subject,
                            );
                        }
                        parsed.morph_options.stretch = *value;
                        saw_stretch = true;
                    },
                    Expr::Ident(raw) if raw == "true" || raw == "false" => {
                        if saw_stretch {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "stretch",
                                host,
                                subject,
                            );
                        }
                        parsed.morph_options.stretch = raw == "true";
                        saw_stretch = true;
                    },
                    other => push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported stretch modifier value {:?} on {}; expected true or false.",
                            other,
                            host.display_name()
                        ),
                        subject,
                    ),
                }
            },
            Some("blend") if host == ModifierHost::Assignment => match &modifier.value {
                Expr::Ident(raw) => {
                    if saw_func_blend {
                        push_conflicting_modifier_diagnostic(diagnostics, "blend", host, subject);
                    }
                    match raw.as_str() {
                        "output" => parsed.func_blend_mode = FuncBlendMode::Output,
                        "opacity" => parsed.func_blend_mode = FuncBlendMode::Opacity,
                        other => push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported blend value '{other}' on {}; supported values are output and opacity.",
                                host.display_name()
                            ),
                            subject,
                        ),
                    }
                    saw_func_blend = true;
                },
                other => push_modifier_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    format!(
                        "Unsupported blend modifier value {:?} on {}; expected output or opacity.",
                        other,
                        host.display_name()
                    ),
                    subject,
                ),
            },
            // Action-specific effect modifiers are declared in
            // ActionSignature.modifiers and consumed by the action itself
            // (after this parser runs, validate_action_modifiers does the
            // signature check). The parser stays silent about them ONLY on
            // the Action host — on assignments/declarations/text these keys
            // have no vocabulary and would be silent no-ops, so they warn.
            Some(name)
                if host == ModifierHost::Action && action_declares_modifier(subject, name) => {},
            Some(name) => push_modifier_diagnostic(
                diagnostics,
                DiagnosticCode::UnsupportedModifierKey,
                format!(
                    "Unsupported modifier key '{name}' on {}; supported modifiers are duration shorthand, delay, and ease.",
                    host.display_name()
                ),
                subject,
            ),
            None => match &modifier.value {
                Expr::Ident(raw) => {
                    if let Some(duration_ms) = parse_duration_literal(raw) {
                        if saw_duration {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "duration",
                                host,
                                subject,
                            );
                        }
                        parsed.duration_ms = duration_ms;
                        saw_duration = true;
                    } else if parse_easing_name(raw).is_some() {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Use named syntax like [ease: {raw}] on {}; bare modifiers are reserved for duration values such as 2s or 500ms.",
                                host.display_name()
                            ),
                            subject,
                        );
                    } else {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported duration shorthand '{raw}' on {}; expected a bare time literal such as 2s or 500ms.",
                                host.display_name()
                            ),
                            subject,
                        );
                    }
                },
                other => push_modifier_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    format!(
                        "Unsupported positional modifier value {:?} on {}; expected a bare duration like 2s or 500ms.",
                        other,
                        host.display_name()
                    ),
                    subject,
                ),
            },
        }
    }

    // Role-appropriate default easing. See `default_easing_for`.
    if !saw_ease && parsed.duration_ms > 0.0 {
        parsed.easing = default_easing_for(host, subject);
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_easing_follows_the_statements_role() {
        // Arrivals decelerate, departures accelerate, repositioning does both.
        assert_eq!(default_easing_for(ModifierHost::Action, Some("fade-in")), Easing::ExpoOut);
        assert_eq!(default_easing_for(ModifierHost::Action, Some("settle-in")), Easing::ExpoOut);
        assert_eq!(default_easing_for(ModifierHost::Action, Some("pop-in")), Easing::ExpoOut);
        assert_eq!(default_easing_for(ModifierHost::Action, Some("fade-out")), Easing::EaseIn);
        assert_eq!(default_easing_for(ModifierHost::Action, Some("remove")), Easing::EaseIn);
        assert_eq!(default_easing_for(ModifierHost::Action, Some("move")), Easing::EaseInOut);
        assert_eq!(
            default_easing_for(ModifierHost::Assignment, Some("x.y")),
            Easing::Linear,
            "assignments keep the old default: an action that expands into them \
             already carries its own timing"
        );
        // Oscillating effects are continuous loops; easing them softens the
        // effect itself.
        assert_eq!(default_easing_for(ModifierHost::Action, Some("shake")), Easing::Linear);
        assert_eq!(default_easing_for(ModifierHost::Action, Some("bounce")), Easing::Linear);
    }

    #[test]
    fn an_explicit_ease_always_wins_over_the_role_default() {
        let ease = |mods: &[Modifier], host, subject| {
            let mut diagnostics = Vec::new();
            let parsed = parse_timing_modifiers(mods, host, subject, &mut diagnostics);
            assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
            parsed
        };
        // A duration is what makes a statement timed; an uneased instant write
        // has no segment to ease.
        let timed = |name: Option<&str>, value: Expr| Modifier {
            name: name.map(str::to_string),
            value,
        };
        let parsed = ease(
            &[timed(None, Expr::Ident("500ms".to_string()))],
            ModifierHost::Action,
            Some("fade-in"),
        );
        assert_eq!(parsed.easing, Easing::ExpoOut, "uneased entrance takes the default");
        assert!(!parsed.ease_authored);
        assert_eq!(
            ease(&[], ModifierHost::Action, Some("fade-in")).easing,
            Easing::Linear,
            "an undated write has no segment to ease"
        );

        let linear = ease(
            &[
                timed(None, Expr::Ident("500ms".to_string())),
                timed(Some("ease"), Expr::Ident("linear".to_string())),
            ],
            ModifierHost::Action,
            Some("fade-in"),
        );
        assert_eq!(linear.easing, Easing::Linear, "`ease: linear` must still mean linear");
        assert!(linear.ease_authored, "the author's intent must be visible to presets");

        let expo = ease(
            &[
                timed(None, Expr::Ident("500ms".to_string())),
                timed(Some("ease"), Expr::Ident("expo-in-out".to_string())),
            ],
            ModifierHost::Assignment,
            Some("box.at"),
        );
        assert_eq!(expo.easing, Easing::ExpoInOut, "an authored curve always wins");
    }

    #[test]
    fn an_uneased_entrance_lands_the_eased_curve_on_its_keyframe() {
        // The default is only real if it reaches the track, not just the parser.
        let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

box: Rect, size: (100, 100), at: (200, 180), color: accent.primary
#0s
fade-in box [500ms]
"#;
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "Parse errors: {errors:?}");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            ast.as_ref().unwrap(),
            &std::collections::HashMap::new(),
        );
        let timeline = report.output;
        let track = timeline.tracks.get("box").expect("box track");
        let easing = track
            .field_ref(crate::timeline::ActorField::Opacity)
            .and_then(|f| f.keyframe_easing(500));
        assert_eq!(easing, Some(Easing::ExpoOut), "the entrance keyframe must carry the default");
    }
}
