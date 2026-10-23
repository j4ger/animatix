use super::*;

/// Compute the common supertype for a list of inferred types.
pub fn common_type(types: &[Type]) -> Type {
    if types.is_empty() {
        return Type::Any;
    }
    if types.iter().all(|t| *t == types[0]) {
        return types[0].clone();
    }
    if types.iter().all(|t| matches!(t, Type::Color | Type::Vec4)) {
        return Type::Vec4;
    }
    if types.iter().all(|t| matches!(t, Type::Num)) {
        return Type::Num;
    }
    Type::Any
}

/// Check that an actual function parameter type is a supertype of the expected
/// parameter type (contravariance). `Any` is top on the actual side and cannot
/// be required by the expected side without also being provided by the actual
/// function.
fn is_param_supertype(actual: &Type, expected: &Type) -> bool {
    if actual == &Type::Any {
        return true;
    }
    if expected == &Type::Any {
        return false;
    }
    is_subtype(expected, actual)
}

/// Check whether `actual` is a subtype of `expected`.
pub fn is_subtype(actual: &Type, expected: &Type) -> bool {
    match (actual, expected) {
        (_, Type::Any) => true,
        (Type::Any, _) => true,
        (a, b) if a == b => true,
        (Type::Color, Type::Vec4) | (Type::Vec4, Type::Color) => true,
        (Type::List(a), Type::List(b)) => is_subtype(a, b),
        (Type::Enum(a), Type::Enum(b)) => a == b,
        (Type::Enum(_), Type::Str) | (Type::Str, Type::Enum(_)) => true,
        (Type::Tuple(a), Type::Tuple(b)) => {
            a.len() == b.len()
                && a.iter().zip(b).all(|(actual, expected)| is_subtype(actual, expected))
        },
        // Tuple literals with `auto`/computed dimensions satisfy fixed-size
        // vectors (e.g. `size: (640, auto)`).
        (Type::Tuple(types), Type::Vec2)
        | (Type::Tuple(types), Type::Vec3)
        | (Type::Tuple(types), Type::Vec4) => {
            types.len() == expected_tuple_arity(expected)
                && types.iter().all(|t| matches!(t, Type::Num | Type::Any))
        },
        (Type::Function { params: a, ret: ar }, Type::Function { params: b, ret: br }) => {
            a.len() == b.len()
                && a.iter().zip(b).all(|(actual, expected)| is_param_supertype(actual, expected))
                && is_subtype(ar, br)
        },
        (Type::Actor(_), Type::Actor(_)) => true,
        (Type::Component(_), Type::Actor(_)) => true,
        (Type::Union(actual_types), Type::Union(expected_types)) => actual_types
            .iter()
            .any(|actual| expected_types.iter().any(|expected| is_subtype(actual, expected))),
        (Type::Union(actual_types), expected) => {
            actual_types.iter().any(|actual| is_subtype(actual, expected))
        },
        (actual, Type::Union(expected_types)) => {
            expected_types.iter().any(|expected| is_subtype(actual, expected))
        },
        _ => false,
    }
}

/// Number of dimensions a fixed-size vector type expects (used to accept
/// tuple literals with `auto`/computed dimensions for `size`-like props).
fn expected_tuple_arity(expected: &Type) -> usize {
    match expected {
        Type::Vec2 => 2,
        Type::Vec3 => 3,
        Type::Vec4 | Type::Color => 4,
        _ => 0,
    }
}

/// Infer the type of an expression against a type environment.
pub fn infer_expr_type(expr: &Expr, env: &TypeEnv) -> Type {
    match expr {
        Expr::Num(_) | Expr::Percent(_) => Type::Num,
        Expr::Str(_) => Type::Str,
        Expr::Bool(_) => Type::Bool,
        Expr::Null => Type::Any,
        Expr::Ident(name) => env.lookup_ident(name).unwrap_or(Type::Any),
        Expr::Path(parts) => env.lookup_path(parts).unwrap_or(Type::Any),
        Expr::Index(base, _) => match infer_expr_type(base, env) {
            Type::List(element) => *element,
            Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Color => Type::Num,
            Type::Str => Type::Str,
            _ => Type::Any,
        },
        Expr::Tuple(items) => {
            let types = items.iter().map(|item| infer_expr_type(item, env)).collect::<Vec<_>>();
            let all_num = types.iter().all(|ty| *ty == Type::Num);
            match types.as_slice() {
                [_, _] if all_num => Type::Vec2,
                [_, _, _] if all_num => Type::Vec3,
                [_, _, _, _] if all_num => Type::Vec4,
                _ => Type::Tuple(types),
            }
        },
        Expr::List(items) => {
            let types = items.iter().map(|item| infer_expr_type(item, env)).collect::<Vec<_>>();
            Type::List(Box::new(common_type(&types)))
        },
        Expr::Binary(left, op, right) => {
            let left_ty = infer_expr_type(left, env);
            let right_ty = infer_expr_type(right, env);
            match op {
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Mod
                | BinaryOp::Pow => {
                    if left_ty == Type::Str && *op == BinaryOp::Add {
                        Type::Str
                    } else if left_ty == Type::Num && right_ty == Type::Num {
                        Type::Num
                    } else if left_ty == Type::Color && right_ty == Type::Color {
                        Type::Color
                    } else {
                        Type::Any
                    }
                },
                BinaryOp::Eq
                | BinaryOp::Neq
                | BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Lte
                | BinaryOp::Gte
                | BinaryOp::And
                | BinaryOp::Or => Type::Bool,
            }
        },
        Expr::Unary(_, inner) => match infer_expr_type(inner, env) {
            Type::Num => Type::Num,
            Type::Bool => Type::Bool,
            other => other,
        },
        Expr::Call(name, _) => match env.lookup_ident(name) {
            Some(Type::Function { ret, .. }) => *ret,
            _ => env.function_type(name).unwrap_or(Type::Any),
        },
        Expr::Method(receiver, name, args) => {
            if args.is_empty() {
                let receiver_ty = infer_expr_type(receiver, env);
                if let Some(ty) = env.lookup_property(&receiver_ty, name) {
                    return ty;
                }
                if matches!(receiver_ty, Type::List(_)) && name == "length" {
                    return Type::Num;
                }
                return Type::Any;
            }
            let receiver_ty = infer_expr_type(receiver, env);
            match (name.as_str(), receiver_ty) {
                ("get", Type::List(element)) => *element,
                ("get", Type::Str) => Type::Str,
                _ => Type::Any,
            }
        },
        Expr::Closure(params, body) => Type::Function {
            params: params.iter().map(|_| Type::Any).collect(),
            ret: Box::new(infer_expr_type(body, env)),
        },
        Expr::LetChain(_, tail) => {
            // Consistent with the Closure arm above: let-bound names are not
            // bound into the type scope (closure params are not either), so
            // the chain's type is its tail's type.
            infer_expr_type(tail, env)
        },
        Expr::Conditional(_, then_expr, else_expr) => common_type(&[
            infer_expr_type(then_expr, env),
            infer_expr_type(else_expr, env),
        ]),
        Expr::Match(_, arms) => {
            let types = arms.iter().map(|(_, arm)| infer_expr_type(arm, env)).collect::<Vec<_>>();
            common_type(&types)
        },
        Expr::Construct(name, _) => env.construct_type(name).unwrap_or(Type::Any),
    }
}
