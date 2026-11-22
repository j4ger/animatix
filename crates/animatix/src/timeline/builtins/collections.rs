use super::{Arc, Environment, EvalError, Value, expect_arg_count, expect_num};
use crate::timeline::plan_value::{PlanDuration, PlanOp, TimelinePlan};
use crate::timeline::utils::{evaluate_callable_value, invoke_closure_value};

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
    for (name, function) in [
        ("tabulate", tabulate as fn(&[Value], &Environment) -> Result<Value, EvalError>),
        ("list_map", list_map),
        ("zip", zip),
        ("scan", scan),
        ("stagger_each", stagger_each),
    ] {
        env.set(name, Value::NativeFn(Arc::new(function)));
    }
}

fn require_callback(name: &str, function: &Value) -> Result<(), EvalError> {
    match function {
        Value::Closure(..)
        | Value::UserFn { .. }
        | Value::NativeFn(_)
        | Value::RegisteredFn { .. } => Ok(()),
        _ => Err(EvalError::TypeMismatch(format!("{name} expects a function"))),
    }
}

fn invoke(
    function: &Value,
    args: impl IntoIterator<Item = Value>,
    name: &str,
    env: &Environment,
) -> Result<Value, EvalError> {
    if let Value::Closure(params, body, captures) = function {
        return invoke_closure_value(
            params,
            body,
            captures,
            args.into_iter().collect(),
            name,
            env,
            None,
        );
    }
    evaluate_callable_value(
        function.clone(),
        args.into_iter().map(|value| (None, value)).collect(),
        name,
        env,
    )
}

fn tabulate(args: &[Value], env: &Environment) -> Result<Value, EvalError> {
    let [count, function] = args else {
        return Err(EvalError::TypeMismatch("tabulate expects a count and a function".into()));
    };
    require_callback("tabulate", function)?;
    let count = list_count("tabulate", count)?;
    (0..count)
        .map(|index| invoke(function, [Value::Num(index as f64)], "tabulate callback", env))
        .collect::<Result<Arc<[_]>, _>>()
        .map(Value::List)
}

fn list_map(args: &[Value], env: &Environment) -> Result<Value, EvalError> {
    let [Value::List(items), function] = args else {
        return Err(EvalError::TypeMismatch("list_map expects a list and a function".into()));
    };
    require_callback("list_map", function)?;
    items
        .iter()
        .map(|item| invoke(function, [item.clone()], "list_map callback", env))
        .collect::<Result<Arc<[_]>, _>>()
        .map(Value::List)
}

fn zip(args: &[Value], _env: &Environment) -> Result<Value, EvalError> {
    let [Value::List(left), Value::List(right)] = args else {
        return Err(EvalError::TypeMismatch("zip expects two lists".into()));
    };
    if left.len() != right.len() {
        return Err(EvalError::TypeMismatch(format!(
            "zip requires lists of equal length, got {} and {}",
            left.len(),
            right.len()
        )));
    }
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| {
            crate::timeline::modifier_runtime::ir::make_vec_value(vec![a.clone(), b.clone()])
        })
        .collect::<Result<Arc<[_]>, _>>()
        .map(Value::List)
}

fn scan(args: &[Value], env: &Environment) -> Result<Value, EvalError> {
    let [initial, Value::List(items), function] = args else {
        return Err(EvalError::TypeMismatch(
            "scan expects an initial value, a list and a function".into(),
        ));
    };
    require_callback("scan", function)?;
    let mut state = initial.clone();
    let mut states = Vec::with_capacity(items.len());
    for item in items.iter() {
        state = invoke(function, [state, item.clone()], "scan callback", env)?;
        states.push(state.clone());
    }
    Ok(Value::List(states.into()))
}

fn stagger_each(args: &[Value], env: &Environment) -> Result<Value, EvalError> {
    let [Value::List(items), Value::Duration(seconds), function] = args else {
        return Err(EvalError::TypeMismatch(
            "stagger_each expects a list, a duration and a function".into(),
        ));
    };
    require_callback("stagger_each", function)?;
    let interval = PlanDuration::from_seconds(*seconds)
        .map_err(|error| EvalError::TypeMismatch(error.to_string()))?;
    let mut children = Vec::with_capacity(items.len());
    for item in items.iter() {
        let Value::TimelinePlan(plan) =
            invoke(function, [item.clone()], "stagger_each callback", env)?
        else {
            return Err(EvalError::TypeMismatch(
                "stagger_each callback must return a timeline".into(),
            ));
        };
        children.push(plan);
    }
    let plan = TimelinePlan::from_ops(vec![PlanOp::Stagger {
        interval,
        children: children.into(),
    }]);
    Ok(Value::TimelinePlan(Arc::new(plan)))
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
