use super::stagger::default_easing_for;
use super::*;

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
                if (host == ModifierHost::Action && action_declares_modifier(subject, name))
                    || (name == "padding"
                        && subject.is_some_and(|s| {
                            s.ends_with(".focus") || s.ends_with(".focus_on")
                        })) => {},
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
