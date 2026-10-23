//! Value formatting, color parsing/coercion, and time conversion.

use crate::ast::{Expr, Time};
use crate::timeline::env::{Environment, EvalError, Value};

use super::expr::evaluate_expr;

/// Format a single Value into its display string.
pub(super) fn format_value(value: &Value) -> String {
    match value {
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
        Value::Closure(args, _, _) => format!("<Closure({:?})>", args),
        Value::UserFn { name, .. } => format!("<UserFn({})>", name),
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
    fn all_nums(items: &[Value]) -> Option<Vec<f64>> {
        items
            .iter()
            .map(|item| match item {
                Value::Num(n) => Some(*n),
                _ => None,
            })
            .collect()
    }
    match value {
        Value::Color([r, g, b, a]) => Some([r as f32, g as f32, b as f32, a as f32]),
        Value::Vec4([r, g, b, a]) => Some([r as f32, g as f32, b as f32, a as f32]),
        Value::Vec3([r, g, b]) => Some([r as f32, g as f32, b as f32, 1.0]),
        Value::List(items) if items.len() == 4 => {
            all_nums(&items).map(|n| [n[0] as f32, n[1] as f32, n[2] as f32, n[3] as f32])
        },
        Value::List(items) if items.len() == 3 => {
            all_nums(&items).map(|n| [n[0] as f32, n[1] as f32, n[2] as f32, 1.0])
        },
        Value::Str(text) => color_from_text(&text),
        _ => None,
    }
}

/// Resolve a color written as text: a CSS-style hex literal (`#rgb`, `#rgba`,
/// `#rrggbb`, `#rrggbbaa`) or one of the built-in color names.
///
/// Both forms are accepted anywhere a color is, so `color: "#ff2d55"` and the
/// stops of `fill_gradient: linear(90, {"#ff2d55", "#5e5ce6"})` read the same
/// way they do on the web.
pub fn color_from_text(text: &str) -> Option<[f32; 4]> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let digits: Vec<u32> = hex.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
        let bytes: Vec<u32> = match digits.len() {
            3 | 4 => digits.iter().map(|n| n * 17).collect(),
            6 | 8 => digits.chunks(2).map(|c| c[0] * 16 + c[1]).collect(),
            _ => return None,
        };
        let scale = |v: u32| v as f32 / 255.0;
        return Some([
            scale(bytes[0]),
            scale(bytes[1]),
            scale(bytes[2]),
            bytes.get(3).map_or(1.0, |&a| scale(a)),
        ]);
    }
    named_color(text)
}

/// Resolve a color expression in the given environment, returning `None` if not a color.
pub fn resolve_color_in_env(expr: &Expr, env: &Environment) -> Result<Option<[f32; 4]>, EvalError> {
    if let Expr::Ident(name) = expr
        && let Some(color) = named_color(name)
    {
        return Ok(Some(color));
    }

    evaluate_expr(expr, env).map(color_from_value)
}

/// Parse a color expression in the given environment, falling back to a default gray.
pub fn parse_color_in_env(expr: &Expr, env: &Environment) -> [f32; 4] {
    resolve_color_in_env(expr, env).ok().flatten().unwrap_or([0.8, 0.8, 0.8, 1.0])
}

/// Convert a `Time` value to milliseconds.
///
/// A beat stamp resolves at the default tempo; a build that knows the scene's
/// `config { bpm: … }` uses [`time_to_ms_at_beat`] instead.
pub fn time_to_ms(time: &Time) -> f64 {
    time_to_ms_at_beat(time, crate::ast::beat_seconds(crate::ast::DEFAULT_BPM) * 1000.0)
}

/// Convert a `Time` to milliseconds with `beat_ms` as the length of one beat.
pub fn time_to_ms_at_beat(time: &Time, beat_ms: f64) -> f64 {
    match time {
        Time::Seconds(s) => *s * 1000.0,
        Time::Milliseconds(ms) => *ms as f64,
        Time::Beats(b) => *b * beat_ms,
        Time::SpeechMarker { offset_ms, .. } | Time::SpeechWord { offset_ms, .. } => {
            *offset_ms as f64
        },
    }
}
