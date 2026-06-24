use super::{Environment, EvalError, Value, evaluate_expr, resolve_color_in_env};
use crate::ast::Expr;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};

pub(crate) fn parse_numeric_vec2(expr: &Expr, env: &Environment) -> Option<[f32; 2]> {
    match evaluate_expr(expr, env).ok()? {
        Value::Vec2([x, y]) => Some([x as f32, y as f32]),
        _ => None,
    }
}

pub(crate) fn assignment_target_key(target: &[String]) -> String {
    target.join(".")
}

pub(crate) fn push_unknown_lookup_path_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    lookup_key: &str,
    suggestion: Option<&str>,
) {
    let hint = suggestion
        .map(|candidate| format!(" Did you mean '{candidate}'?"))
        .unwrap_or_default();
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::UnknownLookupPath,
            DiagnosticPhase::Build,
            format!(
                "Lookup path '{lookup_key}' does not resolve to a known property; using the default value instead.{hint}"
            ),
        )
        .with_subject(subject),
    );
}

pub(crate) fn path_similarity_score(query: &str, candidate: &str) -> isize {
    let query_segments: Vec<&str> = query.split('.').collect();
    let candidate_segments: Vec<&str> = candidate.split('.').collect();

    let shared_prefix = query_segments
        .iter()
        .zip(candidate_segments.iter())
        .take_while(|(left, right)| left == right)
        .count();

    let same_last_segment = query_segments
        .last()
        .zip(candidate_segments.last())
        .is_some_and(|(left, right)| left == right);

    let length_penalty = query.len().abs_diff(candidate.len()) as isize;
    let segment_penalty = (query_segments.len().abs_diff(candidate_segments.len()) * 3) as isize;
    let prefix_bonus = (shared_prefix * 10) as isize;
    let suffix_bonus = if same_last_segment { 12 } else { 0 };

    prefix_bonus + suffix_bonus - length_penalty.min(8) - segment_penalty.min(12)
}

pub(crate) fn best_path_suggestion<'a>(
    query: &str,
    candidates: impl Iterator<Item = &'a str>,
) -> Option<&'a str> {
    let mut best: Option<(&'a str, isize)> = None;

    for candidate in candidates {
        let score = path_similarity_score(query, candidate);
        if score < 4 {
            continue;
        }

        match best {
            Some((_, best_score)) if score <= best_score => {}
            _ => best = Some((candidate, score)),
        }
    }

    best.map(|(candidate, _)| candidate)
}

pub(crate) fn evaluate_expr_with_lookup_diagnostic(
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<Value> {
    match evaluate_expr(expr, env) {
        Ok(value) => Some(value),
        Err(EvalError::UndefinedVariable(lookup_key)) if lookup_key.contains('.') => {
            let candidate_keys = env.all_keys();
            let suggestion =
                best_path_suggestion(&lookup_key, candidate_keys.iter().map(String::as_str));
            push_unknown_lookup_path_diagnostic(diagnostics, subject, &lookup_key, suggestion);
            None
        }
        Err(_) => None,
    }
}

pub(crate) fn parse_color_in_env_with_lookup_diagnostic(
    label: &str,
    property_name: &str,
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<[f32; 4]> {
    let fallback = [0.8, 0.8, 0.8, 1.0];
    match resolve_color_in_env(expr, env) {
        Ok(Some(color)) => Some(color),
        Ok(None) => Some(fallback),
        Err(EvalError::UndefinedVariable(lookup_key)) => {
            let candidate_keys = env.all_keys();
            let suggestion =
                best_path_suggestion(&lookup_key, candidate_keys.iter().map(String::as_str));
            if matches!(expr, Expr::Path(parts) if parts.len() > 2) {
                push_unknown_lookup_path_diagnostic(diagnostics, subject, &lookup_key, suggestion);
                return Some(fallback);
            }
            let hint = suggestion
                .map(|candidate| format!(" Did you mean '{candidate}'?"))
                .unwrap_or_default();
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownColorReference,
                    DiagnosticPhase::Build,
                    format!(
                        "Color value '{lookup_key}' on '{}.{}' does not resolve to a known color; using the default color instead.{hint}",
                        label, property_name
                    ),
                )
                .with_subject(subject),
            );
            Some(fallback)
        }
        Err(_) => Some(fallback),
    }
}

pub(crate) fn parse_numeric_vec2_with_lookup_diagnostic(
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<[f32; 2]> {
    match evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)? {
        Value::Vec2([x, y]) => Some([x as f32, y as f32]),
        _ => None,
    }
}

pub(crate) fn for_iter_values(iterable: &Expr, env: &Environment) -> Vec<Value> {
    match iterable {
        Expr::Tuple(items) => items
            .iter()
            .filter_map(|item| evaluate_expr(item, env).ok())
            .collect(),
        Expr::List(items) => items
            .iter()
            .map(|item| evaluate_expr(item, env))
            .collect::<Result<Vec<Value>, _>>()
            .unwrap_or_default(),
        _ => match evaluate_expr(iterable, env) {
            Ok(Value::List(items)) => items,
            Ok(Value::Vec2([start, end])) => {
                let start = start as i64;
                let end = end as i64;
                (start..end).map(|i| Value::Num(i as f64)).collect()
            }
            Ok(Value::Vec3(values)) => values.into_iter().map(Value::Num).collect(),
            Ok(Value::Vec4(values)) => values.into_iter().map(Value::Num).collect(),
            Ok(value) => vec![value],
            Err(_) => Vec::new(),
        },
    }
}

pub(crate) fn set_lookup_vec2(env: &mut Environment, key: &str, value: [f64; 2]) {
    env.set(key, Value::Vec2(value));
    env.set(&format!("{}.x", key), Value::Num(value[0]));
    env.set(&format!("{}.y", key), Value::Num(value[1]));
}

pub(crate) fn set_lookup_color(env: &mut Environment, key: &str, value: [f64; 4]) {
    env.set(key, Value::Color(value));
    env.set(&format!("{}.r", key), Value::Num(value[0]));
    env.set(&format!("{}.g", key), Value::Num(value[1]));
    env.set(&format!("{}.b", key), Value::Num(value[2]));
    env.set(&format!("{}.a", key), Value::Num(value[3]));
}
