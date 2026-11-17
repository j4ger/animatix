use super::*;

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
                if let Some(parsed_ms) = duration_expr_to_ms(&modifier.value) {
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
                            "Unsupported stagger interval {:?}; expected a time literal such as 120ms or 1s.",
                            modifier.value
                        ),
                        Some("stagger"),
                    );
                }
            },
            Some("each") => {
                if let Some(parsed_ms) = duration_expr_to_ms(&modifier.value) {
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
                            "Unsupported stagger each value {:?}; expected a time literal such as 120ms or 1s.",
                            modifier.value
                        ),
                        Some("stagger"),
                    );
                }
            },
            Some("from") => match &modifier.value {
                Expr::Ident(name) if name == "center" => {
                    from_origin = Some(StaggerOrigin::Center);
                },
                Expr::Str(name) if name == "center" => {
                    from_origin = Some(StaggerOrigin::Center);
                },
                Expr::Ident(name) if name == "top_left" => {
                    from_origin = Some(StaggerOrigin::TopLeft);
                },
                Expr::Str(name) if name == "top-left" || name == "top_left" => {
                    from_origin = Some(StaggerOrigin::TopLeft);
                },
                Expr::Tuple(items) if items.len() == 2 => match (&items[0], &items[1]) {
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
                    },
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
                },
            },
            Some("metric") => match &modifier.value {
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
                },
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
pub(super) fn default_easing_for(host: ModifierHost, subject: Option<&str>) -> Easing {
    if host != ModifierHost::Action {
        // Assignments keep the linear default on purpose: an action that expands
        // into them hands them its own timing modifiers, so easing the
        // assignment path would double-ease effects that already bake their
        // curve into keyframes (`bounce`). Write `ease:` to opt in.
        return Easing::Linear;
    }
    match subject.unwrap_or_default() {
        "fade_in" | "wipe_in" | "reveal_in" | "draw_in" | "settle_in" | "pop_in" | "grow_in" => {
            Easing::ExpoOut
        },
        "fade_out" | "wipe_out" | "reveal_out" | "draw_out" | "remove" | "shrink_out" => {
            Easing::EaseIn
        },
        // Oscillating effects are continuous loops: their shape is the effect,
        // so easing them would soften it. Everything else travels, and a
        // travel that starts and ends at rest reads as intentional.
        "shake" | "pulse" | "bounce" | "highlight" | "unhighlight" => Easing::Linear,
        _ => Easing::EaseInOut,
    }
}
