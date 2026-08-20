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

/// Parse an easing name into the corresponding `Easing` variant.
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

pub(crate) fn parse_stagger_interval_ms(
    modifiers: &[Modifier],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f64> {
    let mut interval_ms = None;
    let mut saw_interval = false;

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
            Some(other) => push_modifier_diagnostic(
                diagnostics,
                DiagnosticCode::UnsupportedModifierKey,
                format!(
                    "Unsupported modifier key '{other}' on stagger; only duration shorthand or 'each' are supported."
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

    interval_ms
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
            Some("ease") => match &modifier.value {
                Expr::Ident(raw) => {
                    if let Some(easing) = parse_easing_name(raw) {
                        if saw_ease {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "ease",
                                host,
                                subject,
                            );
                        }
                        parsed.easing = easing;
                        saw_ease = true;
                    } else {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported ease value '{raw}' on {}; supported values are linear, ease-in, ease-out, ease-in-out, bounce, elastic, back, and expo.",
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
                        "Unsupported ease modifier value {:?} on {}; expected an easing identifier.",
                        other,
                        host.display_name()
                    ),
                    subject,
                ),
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
            // Action-specific effect modifiers handled directly by action execute functions.
            // These are declared in ActionSignature.modifiers and consumed by the action itself;
            // the timing parser must not warn on them. Extend this list if a new action
            // declares custom effect keys (alternative: thread ActionSignature into this fn).
            Some("intensity" | "frequency" | "color" | "blend" | "padding" | "radius") => {
                // Valid action-effect modifiers — not timing modifiers, skip diagnostic.
            },
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

    parsed
}
