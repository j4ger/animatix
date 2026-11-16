//! Invocation-local affine time mapping for expanded timeline functions.
//!
//! This is build-time lowering: the compiled timeline still stores ordinary
//! scene-time tracks and `always` programs. No per-frame scope scheduler or
//! mutable clip state is introduced.

use super::*;
use crate::ast::{Expr, Time, TimeScopeSpec};

#[derive(Clone, Copy, Debug)]
pub(super) struct TimeScope {
    pub origin_ms: f64,
    pub rate: f64,
    pub end_ms: Option<f64>,
}

impl TimeScope {
    pub fn at(self, local_ms: f64) -> f64 {
        self.origin_ms + local_ms * self.rate
    }
}

pub(super) fn contains_local_keyframe(body: &[Stmt]) -> bool {
    body.iter()
        .any(|stmt| matches!(stmt, Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. }))
}

pub(super) fn contains_nested_keyframe(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
            contains_any_keyframe(body)
        },
        Stmt::Block { .. } => false,
        _ => stmt_bodies(stmt).into_iter().any(contains_any_keyframe),
    })
}

fn contains_any_keyframe(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. } => true,
        Stmt::Block { .. } => false,
        _ => stmt_bodies(stmt).into_iter().any(contains_any_keyframe),
    })
}

fn stmt_bodies(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ForLoop { body, .. } => vec![body],
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            let mut bodies = vec![then_branch.as_slice()];
            if let Some(else_branch) = else_branch {
                bodies.push(else_branch);
            }
            bodies
        },
        Stmt::Match { arms, .. } => arms.iter().map(|(_, body)| body.as_slice()).collect(),
        _ => Vec::new(),
    }
}

pub(super) fn local_keyframe_time_ms(time: &Time, beat_ms: f64) -> Option<f64> {
    local_time_ms(time, beat_ms)
}

pub(super) fn statement_extent_ms(
    timeline: &Timeline,
    stmt: &Stmt,
    sequenced: bool,
) -> Option<f64> {
    statement_span_ms_in(timeline, stmt, sequenced)
}

pub(crate) fn sequence_block_span_ms(
    timeline: &Timeline,
    spec: Option<&TimeScopeSpec>,
    body: &[Stmt],
) -> Option<f64> {
    scoped_block_span_ms(timeline, spec, body, true)
}

fn scoped_block_span_ms(
    timeline: &Timeline,
    spec: Option<&TimeScopeSpec>,
    body: &[Stmt],
    sequenced: bool,
) -> Option<f64> {
    if spec.is_none() && !contains_local_keyframe(body) {
        let spans = body
            .iter()
            .map(|stmt| statement_span_ms_in(timeline, stmt, sequenced))
            .collect::<Option<Vec<_>>>()?;
        let span = if sequenced {
            spans.into_iter().sum()
        } else {
            spans.into_iter().fold(0.0, f64::max)
        };
        return span.is_finite().then_some(span);
    }
    if body_has_local_loop_iterable(body) {
        return None;
    }
    let source = body_extent_ms(timeline, body, sequenced, &mut Vec::new())?;
    let span = match spec {
        Some(spec) if spec.clip_duration.is_some() => {
            let clip = parse_clip_duration_ms(spec.clip_duration.as_ref()?)
                .filter(|duration| duration.is_finite() && *duration >= 0.0)?;
            if source > 0.0 || contains_always(body) {
                Some(clip)
            } else {
                None
            }
        },
        Some(spec) if spec.time_scale.is_some() => {
            let rate = match evaluate_expr(spec.time_scale.as_ref()?, &timeline.env).ok()? {
                Value::Num(rate) if rate.is_finite() && rate > 0.0 => rate,
                _ => return None,
            };
            if source > 0.0 {
                let span = source * rate;
                span.is_finite().then_some(span)
            } else if contains_always(body) {
                spec.end_limit_duration_ms
            } else {
                None
            }
        },
        _ => Some(source),
    }?;
    let span = spec
        .and_then(|scope| scope.end_limit_duration_ms)
        .map_or(span, |limit| span.min(limit));
    span.is_finite().then_some(span)
}

fn body_has_local_loop_iterable(body: &[Stmt]) -> bool {
    fn collect_names(body: &[Stmt], names: &mut std::collections::HashSet<String>) {
        for stmt in body {
            match stmt {
                Stmt::LetDecl { name, .. } => {
                    names.insert(name.clone());
                },
                Stmt::ForLoop { var, index_var, .. } => {
                    match var {
                        crate::ast::LoopPattern::Single(name) => {
                            names.insert(name.clone());
                        },
                        crate::ast::LoopPattern::Tuple(items) => {
                            names.extend(items.iter().cloned());
                        },
                    }
                    if let Some(index) = index_var {
                        names.insert(index.clone());
                    }
                },
                _ => {},
            }
            for nested in statement_bodies_including_markers(stmt) {
                collect_names(nested, names);
            }
        }
    }

    fn has_dependent_loop(body: &[Stmt], local_names: &std::collections::HashSet<String>) -> bool {
        body.iter().any(|stmt| {
            if let Stmt::ForLoop { iterable, .. } = stmt
                && local_names.iter().any(|name| iterable.references_ident(name))
            {
                return true;
            }
            statement_bodies_including_markers(stmt)
                .into_iter()
                .any(|nested| has_dependent_loop(nested, local_names))
        })
    }

    let mut local_names = std::collections::HashSet::new();
    collect_names(body, &mut local_names);
    has_dependent_loop(body, &local_names)
}

fn statement_bodies_including_markers(stmt: &Stmt) -> Vec<&[Stmt]> {
    match stmt {
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ForLoop { body, .. }
        | Stmt::Block { body, .. }
        | Stmt::FnDecl { body, .. } => vec![body],
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            let mut bodies = vec![then_branch.as_slice()];
            if let Some(else_branch) = else_branch {
                bodies.push(else_branch);
            }
            bodies
        },
        Stmt::Match { arms, .. } => arms.iter().map(|(_, body)| body.as_slice()).collect(),
        _ => Vec::new(),
    }
}

pub(super) fn resolve_scope(
    timeline: &Timeline,
    spec: Option<&TimeScopeSpec>,
    body: &[Stmt],
    origin_ms: f64,
    sequenced: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TimeScope> {
    validate_scope_inputs(timeline, spec, body, origin_ms, diagnostics)?;

    let source_extent = body_extent_ms(timeline, body, sequenced, diagnostics);
    let rate = match spec {
        Some(spec) if spec.time_scale.is_some() => {
            if source_extent.is_none() {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::InvalidModifierValue,
                    DiagnosticPhase::Build,
                    "Cannot verify a finite local extent for `time_scale`; use a fixed iterable or a body with statically measurable timing.",
                ));
                return None;
            }
            let value = evaluate_expr(spec.time_scale.as_ref().unwrap(), &timeline.env);
            match value {
                Ok(Value::Num(rate)) if rate.is_finite() && rate > 0.0 => rate,
                other => {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::InvalidModifierValue,
                        DiagnosticPhase::Build,
                        format!("`time_scale` must evaluate to a finite positive number, got {other:?}; skipping this function call."),
                    ));
                    return None;
                },
            }
        },
        Some(spec) if spec.clip_duration.is_some() => {
            let target_ms = parse_clip_duration_ms(spec.clip_duration.as_ref().unwrap());
            let Some(target_ms) =
                target_ms.filter(|duration| duration.is_finite() && *duration > 0.0)
            else {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::InvalidModifierValue,
                    DiagnosticPhase::Build,
                    "`clip_duration` expects a positive fixed time literal such as `900ms`; skipping this function call.",
                ));
                return None;
            };
            match source_extent {
                Some(source) if source > 0.0 => target_ms / source,
                Some(0.0) if contains_always(body) => 1.0,
                _ => {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::InvalidModifierValue,
                        DiagnosticPhase::Build,
                        "`clip_duration` needs a function body with a measurable time extent; add keyframes or timed actions.",
                    ));
                    return None;
                },
            }
        },
        _ => 1.0,
    };

    if !rate.is_finite() || rate <= 0.0 {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "The effective local time scale must be finite and positive; skipping this function call.",
        ));
        return None;
    }
    let source_extent = source_extent.unwrap_or(0.0);
    let mut end_ms = match spec {
        Some(spec) if spec.clip_duration.is_some() => {
            parse_clip_duration_ms(spec.clip_duration.as_ref().unwrap())
                .map(|duration| origin_ms + duration)
        },
        _ if source_extent > 0.0 => Some(origin_ms + source_extent * rate),
        _ => None,
    };
    if let Some(limit_ms) = spec.and_then(|scope| scope.end_limit_duration_ms) {
        if !limit_ms.is_finite() || limit_ms < 0.0 {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::InvalidModifierValue,
                DiagnosticPhase::Build,
                "A nested local time boundary must be finite and non-negative; skipping this function call.",
            ));
            return None;
        }
        let inherited_end = origin_ms + limit_ms;
        if !inherited_end.is_finite() {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::InvalidModifierValue,
                DiagnosticPhase::Build,
                "A nested local time boundary overflows scene time; skipping this function call.",
            ));
            return None;
        }
        end_ms = Some(end_ms.map_or(inherited_end, |end| end.min(inherited_end)));
    }
    if end_ms.is_some_and(|end| !end.is_finite()) {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "The local function time extent overflows scene time; reduce `time_scale` or `clip_duration`.",
        ));
        return None;
    }
    if body.iter().any(|stmt| {
        let local_ms = match stmt {
            Stmt::Keyframe { time, .. } => local_time_ms(time, timeline.beat_duration_ms()),
            Stmt::RelativeKeyframe { offset, .. } => {
                local_time_ms(offset, timeline.beat_duration_ms())
            },
            _ => None,
        };
        local_ms.is_some_and(|local_ms| !(origin_ms + local_ms * rate).is_finite())
    }) {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "A local keyframe time overflows scene time under this scope's scale; skipping the function call.",
        ));
        return None;
    }
    if contains_always(body) && end_ms.is_none() {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "A time-scoped `always` needs a finite end. Add `clip_duration:` or a later timed/keyframe boundary.",
        ));
        return None;
    }
    if body_has_plot_func_free_t(body) && end_ms.is_none() {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "A locally retimed plot function that reads scene time needs a finite local scope end; add `clip_duration:` or a later timed boundary.",
        ));
        return None;
    }
    Some(TimeScope {
        origin_ms,
        rate,
        end_ms,
    })
}

fn validate_scope_inputs(
    timeline: &Timeline,
    spec: Option<&TimeScopeSpec>,
    body: &[Stmt],
    origin_ms: f64,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<()> {
    if contains_nested_keyframe(body) {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidPropertyValue,
            DiagnosticPhase::Build,
            "Locally retimed keyframes must be direct statements in a timeline function body; nested keyframe markers are not supported.",
        ));
        return None;
    }
    if !origin_ms.is_finite() {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "A local function time origin must be finite; skipping this function call.",
        ));
        return None;
    }
    let local_keys = contains_local_keyframe(body);
    if spec.is_none() && !local_keys {
        return None;
    }
    if body_has_local_loop_iterable(body) {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "Cannot infer a local time extent from a loop iterable that references a function-local binding; use a fixed iterable independent of function parameters.",
        ));
        return None;
    }
    if let Some(limit) = spec.and_then(|scope| scope.end_limit_duration_ms)
        && (!limit.is_finite() || limit < 0.0)
    {
        return None;
    }
    if contains_always(body) && contains_local_t_binding(body) {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "A locally retimed `always` cannot share a function body with a function-local `t`; the local binding would shadow the scene clock used by its interval guard.",
        ));
        return None;
    }
    if body.iter().any(|stmt| match stmt {
        Stmt::Keyframe { time, .. } => local_time_ms(time, timeline.beat_duration_ms()).is_none(),
        Stmt::RelativeKeyframe { offset, .. } => {
            local_time_ms(offset, timeline.beat_duration_ms()).is_none()
        },
        _ => false,
    }) {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "Local keyframe markers require finite fixed times; speech markers and overflowed times cannot be mapped.",
        ));
        return None;
    }
    Some(())
}

fn parse_clip_duration_ms(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Ident(raw) | Expr::Str(raw) => crate::timeline::parse_duration_literal(raw),
        _ => None,
    }
}

fn contains_always(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::Always { .. } | Stmt::ReactiveBinding { .. } => true,
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. } => contains_always(body),
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => contains_always(then_branch) || else_branch.as_deref().is_some_and(contains_always),
        Stmt::ForLoop { body, .. } => contains_always(body),
        Stmt::Block { .. } => false,
        _ => false,
    })
}

fn contains_local_t_binding(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match stmt {
        Stmt::LetDecl { name, .. } => name == "t",
        Stmt::ForLoop {
            var, index_var, ..
        } => {
            matches!(var, crate::ast::LoopPattern::Single(name) if name == "t")
                || matches!(var, crate::ast::LoopPattern::Tuple(names) if names.iter().any(|name| name == "t"))
                || index_var.as_deref() == Some("t")
                || statement_bodies_including_markers(stmt)
                    .into_iter()
                    .any(contains_local_t_binding)
        },
        _ => statement_bodies_including_markers(stmt)
            .into_iter()
            .any(contains_local_t_binding),
    })
}

fn body_has_plot_func_free_t(body: &[Stmt]) -> bool {
    fn property_reads_t(props: &[crate::ast::Property]) -> bool {
        props
            .iter()
            .any(|prop| prop.name == "func" && rewrite::expr_has_free_t(&prop.value))
    }

    fn inline_reads_t(items: &[crate::ast::InlineItem]) -> bool {
        items.iter().any(|item| match item {
            crate::ast::InlineItem::Labeled {
                props, children, ..
            }
            | crate::ast::InlineItem::Anonymous {
                props, children, ..
            } => property_reads_t(props) || inline_reads_t(children),
            crate::ast::InlineItem::ForLoop { body, .. } => inline_reads_t(body),
            crate::ast::InlineItem::SlotFill { items, .. } => inline_reads_t(items),
            crate::ast::InlineItem::SlotMarker => false,
        })
    }

    body.iter().any(|stmt| {
        let own_plot = match stmt {
            Stmt::ActorDecl {
                props, children, ..
            } => property_reads_t(props) || inline_reads_t(children),
            Stmt::Assignment {
                property, value, ..
            } => property == "func" && rewrite::expr_has_free_t(value),
            _ => false,
        };
        own_plot
            || statement_bodies_including_markers(stmt)
                .into_iter()
                .any(body_has_plot_func_free_t)
    })
}

fn body_extent_ms(
    timeline: &Timeline,
    body: &[Stmt],
    sequenced: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f64> {
    let mut cursor = 0.0_f64;
    let mut end = 0.0_f64;
    for stmt in body {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                let Some(time_ms) = local_time_ms(time, timeline.beat_duration_ms()) else {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                "Local keyframe markers require finite fixed times; speech markers and overflowed times cannot be mapped.",
                    ));
                    return None;
                };
                cursor = time_ms;
                let span = body_span_ms(timeline, body)?;
                end = end.max(cursor + span);
                if sequenced {
                    cursor += span;
                }
            },
            Stmt::RelativeKeyframe { offset, body, .. } => {
                let Some(offset_ms) = local_time_ms(offset, timeline.beat_duration_ms()) else {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        "Local keyframe markers require finite fixed times; speech markers and overflowed times cannot be mapped.",
                    ));
                    return None;
                };
                cursor += offset_ms;
                let span = body_span_ms(timeline, body)?;
                end = end.max(cursor + span);
                if sequenced {
                    cursor += span;
                }
            },
            _ => {
                let span = match stmt {
                    Stmt::ForLoop { .. }
                    | Stmt::Conditional { .. }
                    | Stmt::Match { .. }
                    | Stmt::Sequence { .. }
                    | Stmt::Stagger { .. }
                    | Stmt::Block { .. } => statement_span_ms_in(timeline, stmt, sequenced)?,
                    _ => match statement_span_ms_in(timeline, stmt, sequenced) {
                        Some(span) => span,
                        None if timeline.sequence_statement_span_ms(stmt).is_some() => return None,
                        None => 0.0,
                    },
                };
                end = end.max(cursor + span);
                if sequenced {
                    cursor += span;
                }
            },
        }
    }
    (end.is_finite() && cursor.is_finite()).then_some(end)
}

fn local_time_ms(time: &Time, beat_ms: f64) -> Option<f64> {
    let ms = match time {
        Time::Seconds(_) | Time::Milliseconds(_) | Time::Beats(_) => {
            crate::timeline::time_to_ms_at_beat(time, beat_ms)
        },
        Time::SpeechMarker { .. } | Time::SpeechWord { .. } => return None,
    };
    ms.is_finite().then_some(ms)
}

fn body_span_ms(timeline: &Timeline, body: &[Stmt]) -> Option<f64> {
    let span = body
        .iter()
        .map(|stmt| statement_span_ms_in(timeline, stmt, false))
        .collect::<Option<Vec<_>>>()
        .map(|spans| spans.into_iter().fold(0.0, f64::max))?;
    span.is_finite().then_some(span)
}

fn statement_span_ms_in(timeline: &Timeline, stmt: &Stmt, sequenced: bool) -> Option<f64> {
    let span = match stmt {
        Stmt::Keyframe { time, body, .. } => {
            Some(local_time_ms(time, timeline.beat_duration_ms())? + body_span_ms(timeline, body)?)
        },
        Stmt::RelativeKeyframe { offset, body, .. } => Some(
            local_time_ms(offset, timeline.beat_duration_ms())? + body_span_ms(timeline, body)?,
        ),
        Stmt::ForLoop {
            iterable,
            modifiers,
            body,
            ..
        } => {
            let step = modifiers
                .iter()
                .find_map(|modifier| {
                    (modifier.name.as_deref() == Some("step"))
                        .then(|| match &modifier.value {
                            Expr::Ident(raw) | Expr::Str(raw) => {
                                crate::timeline::parse_duration_literal(raw)
                            },
                            _ => None,
                        })
                        .flatten()
                })
                .filter(|step| step.is_finite() && *step >= 0.0)
                .unwrap_or(0.0);
            let count = match evaluate_expr(iterable, &timeline.env) {
                Ok(Value::List(values)) => values.len(),
                Ok(Value::Vec2([start, end])) => (end as i64 - start as i64).max(0) as usize,
                Ok(Value::Vec3(_)) => 3,
                Ok(Value::Vec4(_)) => 4,
                Ok(_) => 1,
                Err(error) => {
                    tracing::debug!(
                        "cannot infer local clip extent from loop iterable {iterable:?}: {error}"
                    );
                    return None;
                },
            };
            if count == 0 {
                return Some(0.0);
            }
            Some(count.saturating_sub(1) as f64 * step + body_span_ms(timeline, body)?)
        },
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => Some(
            body_span_ms(timeline, then_branch)?
                .max(else_branch.as_deref().map_or(Some(0.0), |b| body_span_ms(timeline, b))?),
        ),
        Stmt::Match { arms, .. } => Some(
            arms.iter()
                .map(|(_, body)| body_span_ms(timeline, body))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .fold(0.0, f64::max),
        ),
        Stmt::Block {
            body, time_scope, ..
        } => scoped_block_span_ms(timeline, time_scope.as_ref(), body, sequenced),
        Stmt::Sequence { body, .. } => body
            .iter()
            .try_fold(0.0, |sum, stmt| Some(sum + statement_span_ms_in(timeline, stmt, true)?)),
        Stmt::Stagger { .. } => timeline.sequence_statement_span_ms(stmt),
        _ => Some(timeline.sequence_statement_span_ms(stmt).unwrap_or(0.0)),
    }?;
    span.is_finite().then_some(span)
}

mod rewrite;
pub(super) use rewrite::retime_statement;

#[cfg(test)]
mod tests;
