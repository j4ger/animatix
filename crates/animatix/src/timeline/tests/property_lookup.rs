use super::*;

#[test]
fn test_for_iter_values_supports_tuple_literals() {
    let env = Environment::new();
    let values =
        for_iter_values(&Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]), &env);

    assert_eq!(values, vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)]);
}

#[test]
fn test_for_iter_values_spreads_list_variable() {
    // When iterating over a variable holding a list, the list should be spread
    // (not wrapped as a single element). This is bug #17 fix.
    let mut env = Environment::new();
    env.set(
        "items",
        Value::List(vec![
            Value::Num(10.0),
            Value::Num(20.0),
            Value::Num(30.0),
        ]),
    );
    let values = for_iter_values(&Expr::Ident("items".to_string()), &env);
    assert_eq!(
        values,
        vec![Value::Num(10.0), Value::Num(20.0), Value::Num(30.0)],
        "for_iter_values should spread a Value::List from a variable, not wrap it"
    );
}

#[test]
fn test_for_iter_values_wraps_non_list_variable() {
    // Non-list values from a variable should still be wrapped as a single element
    let mut env = Environment::new();
    env.set("scalar", Value::Num(42.0));
    let values = for_iter_values(&Expr::Ident("scalar".to_string()), &env);
    assert_eq!(
        values,
        vec![Value::Num(42.0)],
        "for_iter_values should wrap a non-List value as a single element"
    );
}
