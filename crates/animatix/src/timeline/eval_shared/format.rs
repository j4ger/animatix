//! Value formatting and `format` placeholders.

use super::*;

pub(super) fn eval_unary_math(
    args: &[Value],
    name: &str,
    f: fn(f64) -> f64,
) -> Result<Value, EvalError> {
    let x = single_num_arg(name, args)?;
    Ok(Value::Num(f(x)))
}

// ─── Shared format implementation ─────────────────────────────────────────────

/// Format one argument value for `format()` substitution.
pub(super) fn format_value(arg: &Value, precision: Option<usize>) -> String {
    match arg {
        Value::PropRef { label, prop } => {
            format!("&{label}.{prop} (not a value; read {label}.{prop})")
        },
        Value::Num(n) => match precision {
            Some(digits) => format!("{:.*}", digits, n),
            None => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            },
        },
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Vec2(v) => format!("({}, {})", v[0], v[1]),
        Value::Vec3(v) => format!("({}, {}, {})", v[0], v[1], v[2]),
        Value::Vec4(v) | Value::Color(v) => {
            format!("({}, {}, {}, {})", v[0], v[1], v[2], v[3])
        },
        Value::List(items) => format!("{:?}", items),
        Value::Object(name, fields) => format!("{}({:?})", name, fields),
        Value::NativeFn(_) => "<NativeFn>".to_string(),
        Value::Closure(_, _, _) => "<Closure>".to_string(),
        Value::UserFn { name, .. } => format!("<UserFn({})>", name),
    }
}

/// What a `{...}` sequence starting at `open` (the index of `{`) means.
pub(super) enum Placeholder {
    /// Recognized placeholder: braces end at `close`, value formatted with
    /// the given precision (`{}` → `None`).
    Subst {
        close: usize,
        precision: Option<usize>,
    },
    /// Braces with an unsupported spec (`{:d}`) — keep them literal but
    /// continue scanning after them so later placeholders still substitute.
    Literal { close: usize },
    /// No closing brace — emit the rest of the template verbatim.
    Unterminated,
}

pub(super) fn parse_placeholder(template: &str, open: usize) -> Placeholder {
    let Some(close) = template[open..].find('}') else {
        return Placeholder::Unterminated;
    };
    let close = open + close;
    let spec = &template[open + 1..close];
    // Only `{:.N}` precision specs are supported; `{}` means "next value,
    // default formatting".
    if spec.is_empty() {
        return Placeholder::Subst {
            close,
            precision: None,
        };
    }
    if let Some(rest) = spec.strip_prefix(':') {
        if let Some(digits) = rest.strip_prefix('.') {
            if let Ok(n) = digits.parse::<usize>() {
                return Placeholder::Subst {
                    close,
                    precision: Some(n),
                };
            }
        }
    }
    Placeholder::Literal { close }
}

pub(super) fn eval_format(args: &[Value]) -> Result<Value, EvalError> {
    let Some((template, rest)) = args.split_first() else {
        return Ok(Value::Str(String::new()));
    };
    let Value::Str(template) = template else {
        return Ok(Value::Str(String::new()));
    };
    let template: &str = template;
    let mut output = String::with_capacity(template.len() + 16);
    let mut byte_pos = 0;
    let mut arg_index = 0;
    while let Some(open) = template[byte_pos..].find('{') {
        let open = byte_pos + open;
        output.push_str(&template[byte_pos..open]);
        match parse_placeholder(template, open) {
            Placeholder::Subst { close, precision } if arg_index < rest.len() => {
                output.push_str(&format_value(&rest[arg_index], precision));
                arg_index += 1;
                byte_pos = close + 1;
            },
            Placeholder::Subst { close, .. } => {
                // No value left for this placeholder — keep it literal so
                // over-`{}`-ing a template is visible rather than silent.
                output.push_str(&template[open..=close]);
                byte_pos = close + 1;
            },
            Placeholder::Literal { close } => {
                output.push_str(&template[open..=close]);
                byte_pos = close + 1;
            },
            Placeholder::Unterminated => {
                // Unterminated brace: emit the rest of the template literally.
                output.push_str(&template[open..]);
                byte_pos = template.len();
            },
        }
    }
    output.push_str(&template[byte_pos..]);
    Ok(Value::Str(output))
}

// ─── Argument extraction ──────────────────────────────────────────────────────

/// Extract a single numeric argument, returning a type error otherwise.
pub(super) fn single_num_arg(name: &str, args: &[Value]) -> Result<f64, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(format!("{} expects 1 argument", name)));
    }
    match &args[0] {
        Value::Num(n) => Ok(*n),
        other => {
            Err(EvalError::TypeMismatch(format!("{} expects a number, got {:?}", name, other)))
        },
    }
}

/// Extract two numeric arguments, returning a type error otherwise.
pub(super) fn two_num_args(name: &str, args: &[Value]) -> Result<(f64, f64), EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(format!("{} expects 2 arguments", name)));
    }
    match (&args[0], &args[1]) {
        (Value::Num(a), Value::Num(b)) => Ok((*a, *b)),
        _ => Err(EvalError::TypeMismatch(format!(
            "{} expects numbers, got {:?} and {:?}",
            name, args[0], args[1]
        ))),
    }
}

/// Extract three numeric arguments, returning a type error otherwise.
pub(super) fn three_num_args(name: &str, args: &[Value]) -> Result<(f64, f64, f64), EvalError> {
    if args.len() != 3 {
        return Err(EvalError::TypeMismatch(format!("{} expects 3 arguments", name)));
    }
    match (&args[0], &args[1], &args[2]) {
        (Value::Num(a), Value::Num(b), Value::Num(c)) => Ok((*a, *b, *c)),
        _ => Err(EvalError::TypeMismatch(format!(
            "{} expects numbers, got {:?}, {:?}, {:?}",
            name, args[0], args[1], args[2]
        ))),
    }
}
