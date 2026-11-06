use super::stagger::default_easing_for;
use super::*;

pub(crate) fn parse_timing_modifiers(
    modifiers: &[Modifier],
    host: ModifierHost,
    subject: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> ParsedTimingModifiers {
    let mut state = TimingModifierState::new();

    for modifier in modifiers {
        state.apply(modifier, host, subject, diagnostics);
    }

    // Role-appropriate default easing. See `default_easing_for`.
    if !state.saw_ease && state.parsed.duration_ms > 0.0 {
        state.parsed.easing = default_easing_for(host, subject);
    }
    state.parsed
}

/// Accumulated parse state for [`parse_timing_modifiers`]: the result being
/// built plus one saw-a-value flag per modifier family. The flags drive the
/// conflicting-modifier diagnostic. Holding them in a struct lets each
/// modifier family lower in its own helper.
struct TimingModifierState {
    parsed: ParsedTimingModifiers,
    saw_duration: bool,
    saw_delay: bool,
    saw_ease: bool,
    saw_strategy: bool,
    saw_path_arc: bool,
    saw_stretch: bool,
    saw_func_blend: bool,
}

impl TimingModifierState {
    fn new() -> Self {
        Self {
            parsed: ParsedTimingModifiers {
                duration_ms: 0.0,
                delay_ms: 0.0,
                easing: Easing::Linear,
                ease_authored: false,
                anticipate_ms: 0.0,
                morph_options: MorphOptions::default(),
                func_blend_mode: FuncBlendMode::Output,
            },
            saw_duration: false,
            saw_delay: false,
            saw_ease: false,
            saw_strategy: false,
            saw_path_arc: false,
            saw_stretch: false,
            saw_func_blend: false,
        }
    }

    /// Lower one modifier by key. Key dispatch order, values and diagnostics
    /// are unchanged from the pre-split loop body.
    fn apply(
        &mut self,
        modifier: &Modifier,
        host: ModifierHost,
        subject: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match modifier.name.as_deref() {
            Some("delay") => self.apply_delay(modifier, host, subject, diagnostics),
            Some("anticipate") => self.apply_anticipate(modifier, host, subject, diagnostics),
            Some("ease") => self.apply_ease(modifier, host, subject, diagnostics),
            Some("strategy") => self.apply_strategy(modifier, host, subject, diagnostics),
            Some("path_arc") => self.apply_path_arc(modifier, host, subject, diagnostics),
            Some("stretch") => self.apply_stretch(modifier, host, subject, diagnostics),
            Some("blend") if host == ModifierHost::Assignment => {
                self.apply_blend(modifier, host, subject, diagnostics)
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
            None => self.apply_duration_shorthand(modifier, host, subject, diagnostics),
        }
    }

    /// `[delay: 120ms]`
    fn apply_delay(
        &mut self,
        modifier: &Modifier,
        host: ModifierHost,
        subject: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match &modifier.value {
            Expr::Ident(raw) => {
                if let Some(delay_ms) = parse_duration_literal(raw) {
                    if self.saw_delay {
                        push_conflicting_modifier_diagnostic(diagnostics, "delay", host, subject);
                    }
                    self.parsed.delay_ms = delay_ms;
                    self.saw_delay = true;
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
        }
    }

    /// `[anticipate: 80ms]`
    fn apply_anticipate(
        &mut self,
        modifier: &Modifier,
        host: ModifierHost,
        subject: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match &modifier.value {
            Expr::Ident(raw) => match parse_duration_literal(raw) {
                Some(ms) => self.parsed.anticipate_ms = ms,
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
        }
    }

    /// `[ease: ease-out]`
    fn apply_ease(
        &mut self,
        modifier: &Modifier,
        host: ModifierHost,
        subject: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(easing) = parse_easing_expr(&modifier.value) {
            if self.saw_ease {
                push_conflicting_modifier_diagnostic(diagnostics, "ease", host, subject);
            }
            self.parsed.easing = easing;
            self.parsed.ease_authored = true;
            self.saw_ease = true;
        } else {
            push_modifier_diagnostic(
                diagnostics,
                DiagnosticCode::InvalidModifierValue,
                ease_value_diagnostic(&modifier.value, host),
                subject,
            );
        }
    }

    /// `[strategy: auto]` — morph-only.
    fn apply_strategy(
        &mut self,
        modifier: &Modifier,
        host: ModifierHost,
        subject: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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
            return;
        }

        match &modifier.value {
            Expr::Ident(raw) => {
                if self.saw_strategy {
                    push_conflicting_modifier_diagnostic(diagnostics, "strategy", host, subject);
                }
                match raw.as_str() {
                    "auto" => self.parsed.morph_options.strategy = MorphStrategy::Auto,
                    "match" => self.parsed.morph_options.strategy = MorphStrategy::Match,
                    "fade" => self.parsed.morph_options.strategy = MorphStrategy::Fade,
                    "nearest" => self.parsed.morph_options.strategy = MorphStrategy::Nearest,
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
                self.saw_strategy = true;
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
    }

    /// `[path_arc: 0.5]` — morph-only.
    fn apply_path_arc(
        &mut self,
        modifier: &Modifier,
        host: ModifierHost,
        subject: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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
            return;
        }

        let parsed_arc = match &modifier.value {
            Expr::Num(value) => Some(*value),
            Expr::Ident(raw) => raw.parse::<f64>().ok(),
            _ => None,
        };

        if let Some(path_arc) = parsed_arc {
            if self.saw_path_arc {
                push_conflicting_modifier_diagnostic(diagnostics, "path_arc", host, subject);
            }
            self.parsed.morph_options.path_arc = path_arc;
            self.saw_path_arc = true;
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
    }

    /// `[stretch: true]` — morph-only.
    fn apply_stretch(
        &mut self,
        modifier: &Modifier,
        host: ModifierHost,
        subject: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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
            return;
        }

        match &modifier.value {
            Expr::Bool(value) => {
                if self.saw_stretch {
                    push_conflicting_modifier_diagnostic(diagnostics, "stretch", host, subject);
                }
                self.parsed.morph_options.stretch = *value;
                self.saw_stretch = true;
            },
            Expr::Ident(raw) if raw == "true" || raw == "false" => {
                if self.saw_stretch {
                    push_conflicting_modifier_diagnostic(diagnostics, "stretch", host, subject);
                }
                self.parsed.morph_options.stretch = raw == "true";
                self.saw_stretch = true;
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
    }

    /// `[blend: output]` on an assignment.
    fn apply_blend(
        &mut self,
        modifier: &Modifier,
        host: ModifierHost,
        subject: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match &modifier.value {
            Expr::Ident(raw) => {
                if self.saw_func_blend {
                    push_conflicting_modifier_diagnostic(diagnostics, "blend", host, subject);
                }
                match raw.as_str() {
                    "output" => self.parsed.func_blend_mode = FuncBlendMode::Output,
                    "opacity" => self.parsed.func_blend_mode = FuncBlendMode::Opacity,
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
                self.saw_func_blend = true;
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
        }
    }

    /// A bare (unnamed) modifier: duration shorthand such as `[2s]`/`[500ms]`.
    fn apply_duration_shorthand(
        &mut self,
        modifier: &Modifier,
        host: ModifierHost,
        subject: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match &modifier.value {
            Expr::Ident(raw) => {
                if let Some(duration_ms) = parse_duration_literal(raw) {
                    if self.saw_duration {
                        push_conflicting_modifier_diagnostic(
                            diagnostics,
                            "duration",
                            host,
                            subject,
                        );
                    }
                    self.parsed.duration_ms = duration_ms;
                    self.saw_duration = true;
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
        }
    }
}
