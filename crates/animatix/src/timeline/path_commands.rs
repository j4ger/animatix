//! Path commands are ordinary evaluated values, so named arguments, variables,
//! and user functions work in `commands` and `along` exactly as elsewhere.

use std::sync::Arc;

use super::{Environment, EvalError, Value};

pub(crate) fn register(env: &mut Environment) {
    super::value_parser::register_gradient_functions(env);
    for (name, arity) in [
        ("move_to", 2),
        ("line_to", 2),
        ("quad_to", 4),
        ("curve_to", 6),
        ("close", 0),
    ] {
        env.set(
            name,
            Value::NativeFn(Arc::new(move |args, _| {
                if args.len() != arity
                    || !args.iter().all(|value| matches!(value, Value::Num(n) if n.is_finite()))
                {
                    return Err(EvalError::TypeMismatch(format!(
                        "{name} expects {arity} finite numeric coordinates"
                    )));
                }
                Ok(Value::Object(
                    "PathCommand".into(),
                    std::collections::HashMap::from([
                        ("operation".into(), Value::Str(name.into())),
                        ("coordinates".into(), Value::List(args.to_vec().into())),
                    ]),
                ))
            })),
        );
    }
}

pub(crate) fn append(path: &mut kurbo::BezPath, value: &Value) -> Option<()> {
    let Value::Object(kind, fields) = value else {
        return None;
    };
    if kind != "PathCommand" {
        return None;
    }
    let Value::Str(operation) = fields.get("operation")? else {
        return None;
    };
    let Value::List(coordinates) = fields.get("coordinates")? else {
        return None;
    };
    let numbers = coordinates
        .iter()
        .map(|value| match value {
            Value::Num(n) if n.is_finite() => Some(*n),
            _ => None, // Non-numeric command payloads cannot form valid geometry.
        })
        .collect::<Option<Vec<_>>>()?;
    match (operation.as_str(), numbers.as_slice()) {
        ("move_to", [x, y]) => path.move_to((*x, *y)),
        ("line_to", [x, y]) => path.line_to((*x, *y)),
        ("quad_to", [x1, y1, x, y]) => path.quad_to((*x1, *y1), (*x, *y)),
        ("curve_to", [x1, y1, x2, y2, x, y]) => path.curve_to((*x1, *y1), (*x2, *y2), (*x, *y)),
        ("close", []) => path.close_path(),
        _ => return None, // Reject unknown commands and wrong arities atomically.
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_accept_named_calls_and_lists_returned_by_functions() {
        let (ast, errors) = animatix_syntax::parser::parse_source(
            r#"
fn segment(x: Num) { return {move_to(y: 2, x: x), line_to(x: x + 10, y: 5)} }
let commands = segment(3)
#0s
p: Path, commands: commands
"#,
        );
        assert!(errors.is_empty(), "{errors:?}");
        let report =
            super::super::Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default());
        assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
        let Value::List(items) = report.output.env.get("commands").unwrap() else {
            panic!("command list")
        };
        let mut path = kurbo::BezPath::new();
        for item in items.iter() {
            append(&mut path, item).unwrap();
        }
        assert_eq!(
            path.elements(),
            &[
                kurbo::PathEl::MoveTo((3.0, 2.0).into()),
                kurbo::PathEl::LineTo((13.0, 5.0).into())
            ]
        );
    }
}
