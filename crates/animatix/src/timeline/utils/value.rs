//! Value formatting, color parsing/coercion, and time conversion.

use crate::ast::{Expr, Time};
use crate::timeline::env::{Environment, EvalError, Value};

use super::expr::evaluate_expr;

/// Format a single Value into its display string.
pub(super) fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::PropRef { label, prop } => {
            format!("&{label}.{prop} (not a value; read {label}.{prop})")
        },
        Value::Num(n) => {
            if *n == n.floor() {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        },
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Vec2(t) => format!("({}, {})", t[0], t[1]),
        Value::Vec3(t) => format!("({}, {}, {})", t[0], t[1], t[2]),
        Value::Vec4(t) => format!("({}, {}, {}, {})", t[0], t[1], t[2], t[3]),
        Value::Color(c) => format!("rgba({}, {}, {}, {})", c[0], c[1], c[2], c[3]),
        Value::List(items) => format!("{:?}", items),
        Value::Object(name, fields) => format!("{}({:?})", name, fields),
        Value::NativeFn(_) => "<NativeFn>".to_string(),
        Value::RegisteredFn { name, .. } => format!("<Callable({name})>"),
        Value::Closure(args, _, _) => format!("<Closure({:?})>", args),
        Value::UserFn { name, .. } => format!("<UserFn({})>", name),
        Value::Duration(seconds) => format!("{}s", seconds),
        Value::Unit => "()".to_string(),
        Value::Easing(easing) => format!("{easing:?}"),
        Value::ActorRef(actor) => format!("<ActorRef({})>", actor.label()),
        Value::TimelinePlan(plan) => format!("<TimelinePlan({})>", plan.id()),
    }
}

/// Parse a color expression into an `[r, g, b, a]` array.
pub fn parse_color(expr: &Expr) -> [f32; 4] {
    parse_color_in_env(expr, &Environment::new())
}

fn named_color(name: &str) -> Option<[f32; 4]> {
    animatix_syntax::typing::named_color_rgba(name).map(|c| c.map(|v| v as f32))
}

fn color_from_value(value: Value) -> Option<[f32; 4]> {
    // All-numeric brace lists of length 3/4 are accepted alongside
    // Vec3/Vec4/Color so `color = {1, 0, 0, 1}` works like the paren form;
    // heterogeneous or wrong-arity lists stay None (fallback color) — the
    // position/size paths warn on those instead.
    if let Some(c) = value.to_f32_vec4() {
        return Some(c);
    }
    match &value {
        Value::Vec3([r, g, b]) => Some([*r as f32, *g as f32, *b as f32, 1.0]),
        Value::List(items) if items.len() == 4 => {
            value.to_f64_vec().map(|n| [n[0] as f32, n[1] as f32, n[2] as f32, n[3] as f32])
        },
        Value::List(items) if items.len() == 3 => {
            value.to_f64_vec().map(|n| [n[0] as f32, n[1] as f32, n[2] as f32, 1.0])
        },
        _ => None,
    }
}

/// Resolve a color expression in the given environment, returning `None` if not a color.
pub fn resolve_color_in_env(expr: &Expr, env: &Environment) -> Result<Option<[f32; 4]>, EvalError> {
    if let Expr::Ident(name) = expr
        && let Some(color) = named_color(name)
    {
        return Ok(Some(color));
    }

    let value = evaluate_expr(expr, env)?;
    if matches!(value, Value::Str(_)) {
        return Err(EvalError::StringInColor);
    }
    Ok(color_from_value(value))
}

/// Parse a color expression in the given environment, falling back to a default gray.
pub fn parse_color_in_env(expr: &Expr, env: &Environment) -> [f32; 4] {
    resolve_color_in_env(expr, env).ok().flatten().unwrap_or([0.8, 0.8, 0.8, 1.0])
}

/// Convert a `Time` value to milliseconds.
///
/// A beat stamp resolves at the default tempo; a build that knows the scene's
/// `config { bpm: … }` uses [`time_to_ms_at_beat`] instead.
pub fn time_to_ms(time: &Time) -> Result<f64, crate::timeline::EvalError> {
    time_to_ms_at_beat(time, crate::ast::beat_seconds(crate::ast::DEFAULT_BPM) * 1000.0)
}

/// Convert a `Time` to milliseconds with `beat_ms` as the length of one beat.
pub fn time_to_ms_at_beat(time: &Time, beat_ms: f64) -> Result<f64, crate::timeline::EvalError> {
    match time {
        Time::Seconds(s) => Ok(*s * 1000.0),
        Time::Milliseconds(ms) => Ok(*ms as f64),
        Time::Beats(b) => Ok(*b * beat_ms),
        Time::SpeechMarker { offset_ms, .. } | Time::SpeechWord { offset_ms, .. } => {
            Ok(*offset_ms as f64)
        },
        Time::Expression(_) => Err(crate::timeline::EvalError::UnsupportedConstruct(
            "expression times need a build environment".into(),
        )),
    }
}
