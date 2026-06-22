use super::*;

#[test]
fn test_for_iter_values_supports_tuple_literals() {
    let env = Environment::new();
    let values =
        for_iter_values(&Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]), &env);

    assert_eq!(values, vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)]);
}
