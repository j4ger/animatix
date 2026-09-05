//! Lookup-path-aware evaluation and parsing helpers.
//!
//! These utilities evaluate or parse values that may reference dotted lookup
//! paths (for example `color: text.primary`), emitting [`DiagnosticCode::UnknownLookupPath`]
//! diagnostics with "did you mean" suggestions when resolution fails. They also
//! provide a few small helpers for actor-reference parsing, target keys,
//! iteration, and environment injection.

use super::{Environment, EvalError, Value, evaluate_expr, resolve_color_in_env};
use crate::ast::{Expr, TargetSegment, array_actor_label};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};

pub(crate) fn parse_numeric_vec2(expr: &Expr, env: &Environment) -> Option<[f32; 2]> {
    match evaluate_expr(expr, env).ok()? {
        Value::Vec2([x, y]) => Some([x as f32, y as f32]),
        _ => None,
    }
}

/// Parse a source-level actor reference into its internal track label.
/// `box` -> `"box"`, `group.box` -> `"group.box"`, `bar[2]` -> `"bar__2"`,
/// `group.bar[2]` -> `"group.bar__2"`.
pub(crate) fn parse_actor_ref_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Str(s) => Some(s.clone()),
        Expr::Ident(s) => Some(s.clone()),
        Expr::Path(parts) => Some(parts.join(".")),
        Expr::Index(base, index) => {
            let base_name = match base.as_ref() {
                Expr::Ident(name) => name.clone(),
                Expr::Path(parts) if !parts.is_empty() => parts.join("."),
                _ => return None,
            };
            let n = match index.as_ref() {
                Expr::Num(n) if n.trunc() == *n && *n >= 0.0 => Some(*n as usize),
                _ => None,
            };
            n.map(|n| array_actor_label(&base_name, n))
        },
        _ => None,
    }
}

/// Build a joined dot-separated key from target segments, using the base label
/// for Indexed segments (e.g. `bars[i].color` → `"bars.color"`).
///
/// Frame-time indexed targets are handled by the modifier IR's
/// `AssignIndexed` statement instead of this build-time key helper.
pub(crate) fn assignment_target_key(target: &[TargetSegment]) -> String {
    target.iter().map(|s| s.label_str()).collect::<Vec<&str>>().join(".")
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
            Some((_, best_score)) if score <= best_score => {},
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
        },
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
        },
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
    // Try to evaluate an expression, falling back to named-color resolution
    // for identifiers that aren't in the environment (e.g. `red`, `blue`).
    let try_eval = |item: &Expr| -> Option<Value> {
        match evaluate_expr(item, env) {
            Ok(val) => Some(val),
            Err(e) => {
                // Fall back to named-color resolution for bare identifiers
                if let Expr::Ident(_) = item {
                    if let Ok(Some(color)) = resolve_color_in_env(item, env) {
                        let [r, g, b, a] = color;
                        return Some(Value::Color([r as f64, g as f64, b as f64, a as f64]));
                    }
                }
                tracing::warn!("for_iter_values: failed to evaluate item {:?}: {}", item, e);
                None
            },
        }
    };

    match iterable {
        Expr::Tuple(items) => items.iter().filter_map(try_eval).collect(),
        Expr::List(items) => items.iter().filter_map(try_eval).collect(),
        _ => match evaluate_expr(iterable, env) {
            Ok(Value::List(items)) => items.to_vec(),
            Ok(Value::Vec2([start, end])) => {
                let start = start as i64;
                let end = end as i64;
                (start..end).map(|i| Value::Num(i as f64)).collect()
            },
            Ok(Value::Vec3(values)) => values.into_iter().map(Value::Num).collect(),
            Ok(Value::Vec4(values)) => values.into_iter().map(Value::Num).collect(),
            Ok(value) => vec![value],
            Err(e) => {
                tracing::warn!(
                    "for_iter_values: failed to evaluate iterable {:?}: {}",
                    iterable,
                    e
                );
                Vec::new()
            },
        },
    }
}

/// [`set_lookup_vec2`] over a reusable key buffer (PF-6): appends `.x`/`.y`
/// in place instead of `format!`-building two keys per call. `key` is left
/// holding the base key on return.
pub(crate) fn set_lookup_vec2_into(env: &mut Environment, key: &mut String, value: [f64; 2]) {
    env.set(key, Value::Vec2(value));
    let base_len = key.len();
    key.push('x');
    env.set(key, Value::Num(value[0]));
    key.truncate(base_len);
    key.push('y');
    env.set(key, Value::Num(value[1]));
    key.truncate(base_len);
}

/// [`set_lookup_color`] over a reusable key buffer (PF-6) — same shape as
/// [`set_lookup_vec2_into`]. `key` is left holding the base key on return.
pub(crate) fn set_lookup_color_into(env: &mut Environment, key: &mut String, value: [f64; 4]) {
    env.set(key, Value::Color(value));
    let base_len = key.len();
    for (suffix, channel) in [
        (".r", value[0]),
        (".g", value[1]),
        (".b", value[2]),
        (".a", value[3]),
    ] {
        key.push_str(suffix);
        env.set(key, Value::Num(channel));
        key.truncate(base_len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, TargetSegment};

    #[test]
    fn test_assignment_target_key_static() {
        // All-static path must produce the same key as before
        let target = vec![
            TargetSegment::Static("bars__0".to_string()),
            TargetSegment::Static("color".to_string()),
        ];
        assert_eq!(assignment_target_key(&target), "bars__0.color");
    }

    #[test]
    fn test_assignment_target_key_indexed() {
        // Indexed segments must NOT panic; they return the base label
        let target = vec![
            TargetSegment::Indexed {
                base: "bars".to_string(),
                index: Box::new(Expr::Ident("selected".to_string())),
            },
            TargetSegment::Static("color".to_string()),
        ];
        assert_eq!(assignment_target_key(&target), "bars.color");
    }

    #[test]
    fn test_array_actor_label_shared() {
        // Verify that the shared function is used consistently
        assert_eq!(crate::ast::array_actor_label("bars", 0), "bars__0");
        assert_eq!(crate::ast::array_actor_label("dots", 42), "dots__42");
    }
}
