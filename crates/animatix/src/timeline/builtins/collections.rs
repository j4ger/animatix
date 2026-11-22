use super::{Arc, Environment, EvalError, Value, expect_arg_count, expect_num};

const MAX_LIST_ITEMS: usize = 100_000;

pub(super) fn register(env: &mut Environment) {
    let functions = [
        ("range", integer_range as fn(&[Value]) -> Result<Value, EvalError>),
        ("linspace", linspace),
        ("repeat", repeat),
        ("list_slice", list_slice),
        ("list_concat", list_concat),
        ("list_reverse", list_reverse),
    ];
    for (name, function) in functions {
        env.set(name, Value::NativeFn(Arc::new(move |args, _env| function(args))));
    }
}

fn integer_range(args: &[Value]) -> Result<Value, EvalError> {
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

    expect_arg_count("range", args, 3)?;
    let start = expect_num("range", &args[0])?;
    let end = expect_num("range", &args[1])?;
    let step = expect_num("range", &args[2])?;
    for (name, value) in [("start", start), ("end", end), ("step", step)] {
        if !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value) || value.fract() != 0.0 {
            return Err(EvalError::TypeMismatch(format!(
                "range {name} must be an integer between -{MAX_SAFE_INTEGER} and {MAX_SAFE_INTEGER}, got {value}"
            )));
        }
    }
    let (start, end, step) = (start as i64, end as i64, step as i64);
    if step == 0 {
        return Err(EvalError::TypeMismatch("range step must not be zero".into()));
    }

    // Safe-integer bounds leave room in i64 for the full distance and offsets.
    let distance = if step > 0 { end - start } else { start - end };
    if distance <= 0 {
        return Ok(Value::List(Arc::from([])));
    }
    let count = 1 + (distance - 1) / step.abs();
    if count > MAX_LIST_ITEMS as i64 {
        return Err(list_limit("range"));
    }
    Ok(Value::List((0..count).map(|i| Value::Num((start + i * step) as f64)).collect()))
}

fn linspace(args: &[Value]) -> Result<Value, EvalError> {
    let [
        Value::Num(start),
        Value::Num(end),
        count,
        Value::Bool(endpoint),
    ] = args
    else {
        return Err(EvalError::TypeMismatch(
            "linspace expects numeric bounds and count, and a boolean endpoint".into(),
        ));
    };
    if !start.is_finite() || !end.is_finite() {
        return Err(EvalError::TypeMismatch("linspace requires finite bounds".into()));
    }
    let count = list_count("linspace", count)?;
    if count < 2 {
        return Ok(Value::List(vec![Value::Num(*start); count].into()));
    }
    let segments = count - usize::from(*endpoint);
    let span = end - start;
    let mut samples = Vec::with_capacity(count);
    samples.push(Value::Num(*start));
    for i in 1..segments {
        let fraction = i as f64 / segments as f64;
        let value = if span.is_finite() {
            start + span * fraction
        } else {
            // Interpolate opposite-sign extreme bounds without overflowing their difference.
            start * (1.0 - fraction) + end * fraction
        };
        samples.push(Value::Num(value));
    }
    if *endpoint {
        samples.push(Value::Num(*end));
    }
    Ok(Value::List(samples.into()))
}

fn repeat(args: &[Value]) -> Result<Value, EvalError> {
    expect_arg_count("repeat", args, 2)?;
    let count = list_count("repeat", &args[1])?;
    Ok(Value::List(vec![args[0].clone(); count].into()))
}

fn list_slice(args: &[Value]) -> Result<Value, EvalError> {
    let [Value::List(items), Value::Num(start), end] = args else {
        return Err(EvalError::TypeMismatch("list_slice expects a list and numeric bounds".into()));
    };
    let end = match end {
        Value::Null => items.len() as f64,
        other => expect_num("list_slice", other)?,
    };
    if !((0.0..=items.len() as f64).contains(start) && (*start..=items.len() as f64).contains(&end))
        || start.fract() != 0.0
        || end.fract() != 0.0
    {
        return Err(EvalError::TypeMismatch(format!(
            "list_slice requires integer bounds with 0 <= start <= end <= {}, got {start} and {end}",
            items.len()
        )));
    }
    Ok(Value::List(Arc::from(&items[*start as usize..end as usize])))
}

fn list_concat(args: &[Value]) -> Result<Value, EvalError> {
    let [Value::List(left), Value::List(right)] = args else {
        return Err(EvalError::TypeMismatch("list_concat expects two lists".into()));
    };
    if left.len().saturating_add(right.len()) > MAX_LIST_ITEMS {
        return Err(list_limit("list_concat"));
    }
    Ok(Value::List(left.iter().chain(right.iter()).cloned().collect()))
}

fn list_reverse(args: &[Value]) -> Result<Value, EvalError> {
    let [Value::List(items)] = args else {
        return Err(EvalError::TypeMismatch("list_reverse expects a list".into()));
    };
    Ok(Value::List(items.iter().rev().cloned().collect()))
}

fn list_count(name: &str, count: &Value) -> Result<usize, EvalError> {
    let count = expect_num(name, count)?;
    if !count.is_finite() || count < 0.0 || count.fract() != 0.0 {
        return Err(EvalError::TypeMismatch(format!(
            "{name} count must be a non-negative integer, got {count}"
        )));
    }
    if count > MAX_LIST_ITEMS as f64 {
        return Err(list_limit(name));
    }
    Ok(count as usize)
}

fn list_limit(name: &str) -> EvalError {
    EvalError::TypeMismatch(format!("{name} cannot generate more than 100,000 items"))
}
