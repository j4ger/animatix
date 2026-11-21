use super::types::{BuiltinFn, CompiledExpr};
use crate::ast::UnaryOp;
use crate::timeline::callout_geometry::env_anchor_point;
use crate::timeline::env::CapturedEnv;
use crate::timeline::{ActorPoint, Environment, EvalError, Value};
use smallvec::SmallVec;

pub(crate) type PointLookup<'a> =
    &'a dyn Fn(&crate::timeline::plan_value::ActorRef, ActorPoint) -> Option<kurbo::Point>;

/// Evaluate a compiled expression against the given environment.
pub(crate) fn evaluate_compiled_expr(
    expr: &CompiledExpr,
    env: &Environment,
) -> Result<Value, EvalError> {
    evaluate_with_points(expr, env, None)
}

pub(crate) fn evaluate_with_points(
    expr: &CompiledExpr,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    match expr {
        CompiledExpr::Const(Value::Num(n)) => Ok(Value::Num(*n)),
        CompiledExpr::Const(value) => Ok(value.clone()),
        CompiledExpr::LoadEnv(name) => load_env(name, env),
        CompiledExpr::Field(base, field) => {
            let base = evaluate_with_points(base, env, points)?;
            match base {
                Value::ActorRef(actor) => {
                    if let Some(point) = ActorPoint::named(field)
                        && let Some(position) = points.and_then(|read| read(&actor, point))
                    {
                        return Ok(Value::Vec2([position.x, position.y]));
                    }
                    if env.is_frame_phase() {
                        actor.child_resolved(field).map(Value::ActorRef).ok_or_else(|| {
                            EvalError::UnsupportedConstruct(format!(
                                "actor member '{field}' was not resolved during build"
                            ))
                        })
                    } else {
                        let child = actor.child(field);
                        Ok(env.get_path(child.label()).unwrap_or(Value::ActorRef(child)))
                    }
                },
                value => value.read_field(field).ok_or_else(|| {
                    EvalError::TypeMismatch(format!("value has no field '{field}'"))
                }),
            }
        },
        CompiledExpr::MakeVec(items) => eval_make_vec(items, env, points),
        CompiledExpr::MakeList(items) => {
            let values = items
                .iter()
                .map(|item| evaluate_with_points(item, env, points))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(values.into()))
        },
        CompiledExpr::Unary(op, expr) => {
            if *op == crate::ast::UnaryOp::Ref {
                // lowering compiles Unary(Ref, two-segment path) into
                // CompiledExpr::PropRef; reaching this arm means the operand
                // was not a path.
                return Err(EvalError::TypeMismatch(
                    "& expects a two-segment path: &actor.prop".to_string(),
                ));
            }
            let value = evaluate_with_points(expr, env, points)?;
            match (op, value) {
                (UnaryOp::Neg, Value::Num(number)) => Ok(Value::Num(-number)),
                (UnaryOp::Neg, Value::Duration(seconds)) => Ok(Value::Duration(-seconds)),
                (UnaryOp::Not, value) => Ok(Value::Bool(!value.is_truthy())),
                (UnaryOp::Neg, value) => Err(EvalError::TypeMismatch(format!(
                    "unary '-' expects Num or Duration, got {value:?}"
                ))),
                (UnaryOp::Ref, _) => unreachable!("handled above"),
            }
        },
        CompiledExpr::Binary(left, op, right) => {
            let left = evaluate_with_points(left, env, points)?;
            match op {
                crate::ast::BinaryOp::And if !left.is_truthy() => return Ok(Value::Bool(false)),
                crate::ast::BinaryOp::Or if left.is_truthy() => return Ok(Value::Bool(true)),
                _ => {},
            }
            let right = evaluate_with_points(right, env, points)?;
            crate::timeline::eval_shared::eval_binary_op(left, op, right)
        },
        CompiledExpr::Select(condition, then_expr, else_expr) => {
            let cond = evaluate_with_points(condition, env, points)?;
            if cond.is_truthy() {
                evaluate_with_points(then_expr, env, points)
            } else {
                evaluate_with_points(else_expr, env, points)
            }
        },
        CompiledExpr::CallBuiltin(builtin, args) => {
            eval_call_builtin(builtin.clone(), args, env, points)
        },
        CompiledExpr::CallNamed(name, args) => {
            let arg_values = evaluate_named_call_arguments(name, args, env, points)?;
            crate::timeline::utils::evaluate_call_arguments(name, arg_values, env, points)
        },
        CompiledExpr::CallMember(receiver, name, args) => {
            let receiver = evaluate_with_points(receiver, env, points)?;
            evaluate_member_call(receiver, name, args, env, points)
        },
        CompiledExpr::CallValue(callee, args) => {
            let callee = evaluate_with_points(callee, env, points)?;
            let arg_values = evaluate_value_call_arguments(&callee, args, env, points)?;
            crate::timeline::utils::call_with_points(callee, arg_values, "<call>", env, points)
        },
        CompiledExpr::Timeline(plan_body) => {
            if env.is_frame_phase() {
                return Err(EvalError::UnsupportedConstruct(
                    "timeline plans can only be created during build".into(),
                ));
            }
            Ok(Value::TimelinePlan(std::sync::Arc::new(
                crate::timeline::plan_value::TimelinePlan::capture_plan_body(
                    plan_body.clone(),
                    env,
                ),
            )))
        },
        CompiledExpr::SequencePlan(plan_body) => {
            if env.is_frame_phase() {
                return Err(EvalError::UnsupportedConstruct(
                    "sequence plans can only be created during build".into(),
                ));
            }
            let source = vec![crate::ast::Stmt::Sequence {
                body: plan_body.statements.clone(),
                span: None,
            }];
            Ok(Value::TimelinePlan(std::sync::Arc::new(
                crate::timeline::plan_value::TimelinePlan::capture_plan_body(
                    crate::ast::PlanBody {
                        statements: source,
                        instance_methods: plan_body.instance_methods.clone(),
                    },
                    env,
                ),
            )))
        },
        CompiledExpr::StaggerPlan(modifiers, plan_body) => {
            if env.is_frame_phase() {
                return Err(EvalError::UnsupportedConstruct(
                    "stagger plans can only be created during build".into(),
                ));
            }
            let source = vec![crate::ast::Stmt::Stagger {
                modifiers: modifiers.clone(),
                body: plan_body.statements.clone(),
                span: None,
            }];
            Ok(Value::TimelinePlan(std::sync::Arc::new(
                crate::timeline::plan_value::TimelinePlan::capture_plan_body(
                    crate::ast::PlanBody {
                        statements: source,
                        instance_methods: plan_body.instance_methods.clone(),
                    },
                    env,
                ),
            )))
        },
        CompiledExpr::Index(container, index) => eval_index(container, index, env, points),
        CompiledExpr::Closure(params, body) => {
            Ok(Value::Closure(params.clone(), body.clone(), CapturedEnv::snapshot(env)))
        },
        CompiledExpr::PropRef { label, prop } => Ok(Value::PropRef {
            label: label.clone(),
            prop: prop.clone(),
        }),
        CompiledExpr::LetChain(bindings, tail) => eval_let_chain(bindings, tail, env, points),
        CompiledExpr::Construct(name, fields) => eval_construct(name, fields, env, points),
        CompiledExpr::AnchorLookup { actor, anchor } => {
            eval_actor_point(actor, *anchor, env, points)
        },
    }
}

fn load_env(name: &str, env: &Environment) -> Result<Value, EvalError> {
    if name == crate::timeline::env::FRAME_TIME_IDENT {
        env.frame_time_seconds
            .map(Value::Num)
            .ok_or_else(|| EvalError::UndefinedVariable(name.to_owned()))
    } else if let Some(value) = env.get_path(name) {
        Ok(wrap_registered_callable(name, value, env))
    } else if let Some((root, suffix)) = name.split_once('.')
        && let Some(Value::ActorRef(actor)) = env.get_path(root)
    {
        let actor = if env.is_frame_phase() {
            suffix.split('.').try_fold(actor, |actor, field| {
                actor.child_resolved(field).ok_or_else(|| {
                    EvalError::UnsupportedConstruct(format!(
                        "actor member '{field}' was not resolved during build"
                    ))
                })
            })?
        } else {
            suffix.split('.').fold(actor, |actor, field| actor.child(field))
        };
        Ok(Value::ActorRef(actor))
    } else if !env.is_frame_phase() && env.has_component_methods(name) {
        Ok(Value::ActorRef(crate::timeline::plan_value::ActorRef::caller(name)))
    } else if let Some(descriptor) = env.callable_descriptor(name) {
        Ok(Value::RegisteredFn {
            name: descriptor.qualified_name(),
            descriptor: std::sync::Arc::new(descriptor.clone()),
            native: None,
        })
    } else if let Some(easing) = crate::easing::parse_easing_name(name) {
        Ok(Value::Easing(easing))
    } else {
        Err(EvalError::UndefinedVariable(name.to_owned()))
    }
}

fn eval_actor_point(
    actor: &str,
    anchor: ActorPoint,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    if let Some(read) = points
        && let Ok(Value::ActorRef(reference)) = load_env(actor, env)
        && let Some(position) = read(&reference, anchor)
    {
        return Ok(Value::Vec2([position.x, position.y]));
    }
    let name = format!("{actor}.{}", anchor.as_str());
    if let Some(value) = env.get_path(&name) {
        return Ok(value);
    }
    if let ActorPoint::Bounds(bounds) = anchor
        && let Some(point) = env_anchor_point(env, actor, bounds)
    {
        return Ok(Value::Vec2(point));
    }
    Err(EvalError::UndefinedVariable(name))
}

fn eval_construct(
    name: &str,
    fields: &[(String, CompiledExpr)],
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    let fields = fields
        .iter()
        .map(|(name, expr)| {
            evaluate_with_points(expr, env, points).map(|value| (name.clone(), value))
        })
        .collect::<Result<_, _>>()?;
    Ok(Value::Object(name.to_string(), fields))
}

fn wrap_registered_callable(name: &str, value: Value, env: &Environment) -> Value {
    let Value::NativeFn(native) = value else {
        return value;
    };
    let Some(descriptor) = env.callable_descriptor(name) else {
        return Value::NativeFn(native);
    };
    Value::RegisteredFn {
        name: descriptor.qualified_name(),
        descriptor: std::sync::Arc::new(descriptor.clone()),
        native: Some(native),
    }
}

fn evaluate_named_call_arguments(
    name: &str,
    arguments: &[(Option<String>, CompiledExpr)],
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Vec<(Option<String>, Value)>, EvalError> {
    evaluate_named_call_arguments_with_offset(name, arguments, env, points, 0)
}

fn evaluate_named_call_arguments_with_offset(
    name: &str,
    arguments: &[(Option<String>, CompiledExpr)],
    env: &Environment,
    points: Option<PointLookup<'_>>,
    positional_offset: usize,
) -> Result<Vec<(Option<String>, Value)>, EvalError> {
    let unqualified = name.strip_prefix("std::").unwrap_or(name);
    let function = if name.starts_with("std::") {
        env.get(name)
    } else {
        env.get(name).or_else(|| env.get(unqualified))
    };
    if let Some(Value::UserFn { params, .. }) = function {
        let params = params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    param
                        .param_type
                        .as_ref()
                        .map(crate::timeline::callable::type_annotation_to_callable),
                    false,
                )
            })
            .collect::<Vec<_>>();
        return evaluate_arguments_with_params_offset(
            arguments,
            &params,
            env,
            points,
            positional_offset,
        );
    }
    let descriptor = env.callable_descriptor(name).or_else(|| env.callable_descriptor(unqualified));
    let params: Vec<(String, Option<animatix_core::callable::CallableType>, bool)> = descriptor
        .map(|descriptor| {
            descriptor
                .params
                .iter()
                .map(|param| (param.name.clone(), Some(param.ty.clone()), param.variadic))
                .collect()
        })
        .unwrap_or_default();
    evaluate_arguments_with_params_offset(arguments, &params, env, points, positional_offset)
}

fn evaluate_member_call(
    receiver: Value,
    name: &str,
    arguments: &[(Option<String>, CompiledExpr)],
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    if let Value::ActorRef(actor) = &receiver {
        if let Some(method) = env.component_method_for_actor(actor, name) {
            if !component_method_is_visible(&method, name, env) {
                return Err(EvalError::UnsupportedMethod(format!(
                    "component method '{name}' is private"
                )));
            }
            let method = if env.is_frame_phase() {
                method
            } else {
                crate::timeline::fn_eval::bind_component_method_receiver(method, actor)
            };
            let values = evaluate_value_call_arguments(&method, arguments, env, points)?;
            return crate::timeline::utils::call_with_points(method, values, name, env, points);
        }

        if callable_has_actor_receiver(name, env) {
            let mut values =
                evaluate_named_call_arguments_with_offset(name, arguments, env, points, 1)?;
            values.insert(0, (None, Value::ActorRef(actor.clone())));
            return crate::timeline::utils::evaluate_call_arguments(name, values, env, points);
        }
    }

    let values = evaluate_named_call_arguments("<method>", arguments, env, points)?;
    crate::timeline::utils::evaluate_method_arguments(receiver, name, values, env)
}

fn component_method_is_visible(method: &Value, name: &str, env: &Environment) -> bool {
    let Value::UserFn {
        component_scope: Some(scope),
        ..
    } = method
    else {
        return true;
    };
    if scope.methods.get(name).is_none_or(|template| template.is_pub) {
        return true;
    }
    env.active_component_scope
        .as_ref()
        .is_some_and(|active| std::sync::Arc::ptr_eq(active, scope))
}

fn callable_has_actor_receiver(name: &str, env: &Environment) -> bool {
    use animatix_core::callable::CallableType;

    fn accepts_actor(ty: &CallableType) -> bool {
        match ty {
            CallableType::Actor => true,
            CallableType::Union(types) => types.iter().any(accepts_actor),
            _ => false,
        }
    }

    let unqualified = name.strip_prefix("std::").unwrap_or(name);
    let callable = if name.starts_with("std::") {
        env.get(name)
    } else {
        env.get(name).or_else(|| env.get(unqualified))
    };
    match callable {
        Some(Value::UserFn { params, .. }) => params
            .first()
            .and_then(|param| param.param_type.as_ref())
            .map(crate::timeline::callable::type_annotation_to_callable)
            .is_some_and(|ty| accepts_actor(&ty)),
        Some(Value::RegisteredFn { descriptor, .. }) => {
            descriptor.params.first().is_some_and(|param| accepts_actor(&param.ty))
        },
        _ => env
            .callable_descriptor(name)
            .or_else(|| env.callable_descriptor(unqualified))
            .and_then(|descriptor| descriptor.params.first())
            .is_some_and(|param| accepts_actor(&param.ty)),
    }
}

fn evaluate_value_call_arguments(
    callee: &Value,
    arguments: &[(Option<String>, CompiledExpr)],
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Vec<(Option<String>, Value)>, EvalError> {
    if let Value::UserFn { params, .. } = callee {
        let params = params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    param
                        .param_type
                        .as_ref()
                        .map(crate::timeline::callable::type_annotation_to_callable),
                    false,
                )
            })
            .collect::<Vec<_>>();
        evaluate_arguments_with_params(arguments, &params, env, points)
    } else if let Value::RegisteredFn { descriptor, .. } = callee {
        let params = descriptor
            .params
            .iter()
            .map(|param| (param.name.clone(), Some(param.ty.clone()), param.variadic))
            .collect::<Vec<_>>();
        evaluate_arguments_with_params(arguments, &params, env, points)
    } else {
        evaluate_arguments_with_params(arguments, &[], env, points)
    }
}

fn evaluate_arguments_with_params(
    arguments: &[(Option<String>, CompiledExpr)],
    params: &[(String, Option<animatix_core::callable::CallableType>, bool)],
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Vec<(Option<String>, Value)>, EvalError> {
    evaluate_arguments_with_params_offset(arguments, params, env, points, 0)
}

fn evaluate_arguments_with_params_offset(
    arguments: &[(Option<String>, CompiledExpr)],
    params: &[(String, Option<animatix_core::callable::CallableType>, bool)],
    env: &Environment,
    points: Option<PointLookup<'_>>,
    positional_offset: usize,
) -> Result<Vec<(Option<String>, Value)>, EvalError> {
    let mut positional_index = positional_offset;
    arguments
        .iter()
        .map(|(name, expression)| {
            let param_index = if let Some(name) = name {
                params.iter().position(|(param_name, _, _)| param_name == name)
            } else {
                let index = (positional_index < params.len()).then_some(positional_index);
                if index.is_some() && !params[positional_index].2 {
                    positional_index += 1;
                }
                index
            };
            let expected = param_index
                .and_then(|index| params.get(index))
                .and_then(|(_, ty, _)| ty.as_ref());
            let value = evaluate_argument(expression, expected, env, points)?;
            Ok((name.clone(), value))
        })
        .collect()
}

fn evaluate_argument(
    expression: &CompiledExpr,
    expected: Option<&animatix_core::callable::CallableType>,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    use animatix_core::callable::CallableType as T;
    if let CompiledExpr::LoadEnv(symbol) = expression
        && expected.is_some_and(argument_accepts_color)
        && env.get_path(symbol).is_none()
        && let Some(color) = animatix_syntax::typing::named_color_rgba(symbol)
    {
        return Ok(Value::Color(color));
    }
    match (expected, expression) {
        (Some(T::Actor), CompiledExpr::LoadEnv(label)) => {
            return actor_reference_value(label, env);
        },
        (Some(T::List(inner)), CompiledExpr::MakeList(items))
            if matches!(inner.as_ref(), T::Actor) =>
        {
            let items = items
                .iter()
                .map(|item| evaluate_argument(item, Some(inner), env, points))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Value::List(items.into()));
        },
        (Some(T::Enum(_)), CompiledExpr::LoadEnv(symbol)) if env.get_path(symbol).is_none() => {
            return Ok(Value::Str(symbol.clone()));
        },
        (Some(T::Union(types)), CompiledExpr::LoadEnv(symbol))
            if types.iter().any(|ty| matches!(ty, T::Enum(_)))
                && env.get_path(symbol).is_none() =>
        {
            return Ok(Value::Str(symbol.clone()));
        },
        (Some(T::Easing), CompiledExpr::LoadEnv(symbol)) => match env.get_path(symbol) {
            Some(Value::Easing(easing)) => return Ok(Value::Easing(easing)),
            Some(Value::NativeFn(_) | Value::RegisteredFn { .. }) | None => {
                if let Some(easing) = crate::easing::parse_easing_name(symbol) {
                    return Ok(Value::Easing(easing));
                }
            },
            Some(_) => {}, // Explicit lexical values keep their own type.
        },
        _ => {},
    }
    evaluate_with_points(expression, env, points)
}

fn argument_accepts_color(ty: &animatix_core::callable::CallableType) -> bool {
    use animatix_core::callable::CallableType as T;
    match ty {
        T::Color => true,
        T::Union(types) => types.iter().any(argument_accepts_color),
        _ => false, // Other parameter kinds do not interpret color literals.
    }
}

fn actor_reference_value(label: &str, env: &Environment) -> Result<Value, EvalError> {
    match load_env(label, env) {
        Ok(Value::ActorRef(reference)) => Ok(Value::ActorRef(reference)),
        Ok(value) => Err(EvalError::TypeMismatch(format!(
            "actor parameter expects an actor reference, got {value:?} for '{label}'"
        ))),
        Err(_) => Ok(Value::ActorRef(crate::timeline::plan_value::ActorRef::caller(label))),
    }
}

fn eval_make_vec(
    items: &[CompiledExpr],
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    match items.len() {
        2 => {
            let v0 = evaluate_with_points(&items[0], env, points)?;
            let v1 = evaluate_with_points(&items[1], env, points)?;
            if matches!((&v0, &v1), (Value::Num(_), Value::Num(_))) {
                Ok(Value::Vec2([v0.as_num(), v1.as_num()]))
            } else {
                make_vec_value(vec![v0, v1])
            }
        },
        3 => {
            let v0 = evaluate_with_points(&items[0], env, points)?;
            let v1 = evaluate_with_points(&items[1], env, points)?;
            let v2 = evaluate_with_points(&items[2], env, points)?;
            if matches!((&v0, &v1, &v2), (Value::Num(_), Value::Num(_), Value::Num(_))) {
                Ok(Value::Vec3([v0.as_num(), v1.as_num(), v2.as_num()]))
            } else {
                make_vec_value(vec![v0, v1, v2])
            }
        },
        4 => {
            let v0 = evaluate_with_points(&items[0], env, points)?;
            let v1 = evaluate_with_points(&items[1], env, points)?;
            let v2 = evaluate_with_points(&items[2], env, points)?;
            let v3 = evaluate_with_points(&items[3], env, points)?;
            if matches!(
                (&v0, &v1, &v2, &v3),
                (Value::Num(_), Value::Num(_), Value::Num(_), Value::Num(_))
            ) {
                Ok(Value::Vec4([v0.as_num(), v1.as_num(), v2.as_num(), v3.as_num()]))
            } else {
                make_vec_value(vec![v0, v1, v2, v3])
            }
        },
        _ => {
            let values = items
                .iter()
                .map(|item| evaluate_with_points(item, env, points))
                .collect::<Result<Vec<_>, _>>()?;
            make_vec_value(values)
        },
    }
}

fn eval_call_builtin(
    builtin: BuiltinFn,
    args: &[CompiledExpr],
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    let args: SmallVec<[Value; 4]> = args
        .iter()
        .map(|arg| evaluate_with_points(arg, env, points))
        .collect::<Result<_, _>>()?;
    let name = match builtin {
        BuiltinFn::Sin => "sin",
        BuiltinFn::Cos => "cos",
        BuiltinFn::Lerp => "lerp",
        BuiltinFn::Format => "format",
        BuiltinFn::Tan => "tan",
        BuiltinFn::Sqrt => "sqrt",
        BuiltinFn::Exp => "exp",
        BuiltinFn::Log => "ln",
        BuiltinFn::Atan2 => "atan2",
        BuiltinFn::Clamp => "clamp",
        BuiltinFn::Abs => "abs",
        BuiltinFn::Min => "min",
        BuiltinFn::Max => "max",
        BuiltinFn::Floor => "floor",
        BuiltinFn::Ceil => "ceil",
        BuiltinFn::Deg => "deg",
        BuiltinFn::Rad => "rad",
        BuiltinFn::ListSwap => "list_swap",
        BuiltinFn::ListSet => "list_set",
        BuiltinFn::Signum => "signum",
        BuiltinFn::Fract => "fract",
        BuiltinFn::Hypot => "hypot",
        BuiltinFn::Pow => "pow",
        BuiltinFn::Rem => "rem",
        BuiltinFn::Step => "step",
        BuiltinFn::Round => "round",
        BuiltinFn::Factorial => "factorial",
        BuiltinFn::SumList => "sum",
        BuiltinFn::CurveAt => "curve_at",
        BuiltinFn::CurveSmooth => "curve_smooth",
        BuiltinFn::SeededRand => "seeded_rand",
    };
    if args.iter().any(|value| !matches!(value, Value::Num(_)))
        || env.get(name).is_some() && !crate::timeline::builtins::is_standard_callable(name, env)
    {
        return crate::timeline::utils::evaluate_call_value(name, args.into_vec(), env);
    }
    crate::timeline::eval_shared::eval_builtin_fn(name, &args)
}

fn eval_index(
    container: &CompiledExpr,
    index: &CompiledExpr,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    let index_val = evaluate_with_points(index, env, points)?;
    let container_val = match container {
        CompiledExpr::LoadEnv(name) if env.get_path(name).is_none() => {
            Value::ActorRef(crate::timeline::plan_value::ActorRef::caller(name.as_str()))
        },
        _ => evaluate_with_points(container, env, points)?,
    };
    let raw_index = match index_val {
        Value::Num(value) => value,
        other => {
            return Err(EvalError::TypeMismatch(format!("index must be Num, got {other:?}")));
        },
    };
    if let Value::ActorRef(reference) = &container_val {
        if !raw_index.is_finite() || raw_index < 0.0 || raw_index.fract() != 0.0 {
            return Err(EvalError::InvalidActorIndex(raw_index));
        }
        let index = raw_index as usize;
        if env.is_frame_phase() {
            return reference
                .indexed_resolved(index)
                .map(Value::ActorRef)
                .ok_or(EvalError::InvalidActorIndex(raw_index));
        }
        return Ok(Value::ActorRef(reference.indexed(index)));
    }
    if !raw_index.is_finite() || raw_index < 0.0 || raw_index.fract() != 0.0 {
        return Err(EvalError::TypeMismatch(format!(
            "index must be a non-negative integer, got {raw_index}"
        )));
    }
    let idx = raw_index as usize;
    match container_val {
        Value::List(items) => items.get(idx).cloned().ok_or_else(|| {
            EvalError::TypeMismatch(format!(
                "Index {} out of bounds for list of length {}",
                idx,
                items.len()
            ))
        }),
        Value::Str(s) => s.chars().nth(idx).map(|c| Value::Str(c.to_string())).ok_or_else(|| {
            EvalError::TypeMismatch(format!(
                "Index {} out of bounds for string of length {}",
                idx,
                s.len()
            ))
        }),
        Value::Vec2(v) => match idx {
            0 => Ok(Value::Num(v[0])),
            1 => Ok(Value::Num(v[1])),
            _ => Err(EvalError::TypeMismatch(format!("Index {} out of bounds for Vec2", idx))),
        },
        Value::Vec3(v) => match idx {
            0 => Ok(Value::Num(v[0])),
            1 => Ok(Value::Num(v[1])),
            2 => Ok(Value::Num(v[2])),
            _ => Err(EvalError::TypeMismatch(format!("Index {} out of bounds for Vec3", idx))),
        },
        Value::Vec4(v) => match idx {
            0 => Ok(Value::Num(v[0])),
            1 => Ok(Value::Num(v[1])),
            2 => Ok(Value::Num(v[2])),
            3 => Ok(Value::Num(v[3])),
            _ => Err(EvalError::TypeMismatch(format!("Index {} out of bounds for Vec4", idx))),
        },
        Value::Color(c) => match idx {
            0 => Ok(Value::Num(c[0])),
            1 => Ok(Value::Num(c[1])),
            2 => Ok(Value::Num(c[2])),
            3 => Ok(Value::Num(c[3])),
            _ => Err(EvalError::TypeMismatch(format!("Index {} out of bounds for Color", idx))),
        },
        other => Err(EvalError::TypeMismatch(format!("Cannot index into {:?}", other))),
    }
}

fn eval_let_chain(
    bindings: &[(String, CompiledExpr)],
    tail: &CompiledExpr,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    // Bindings evaluate in order; each becomes a let-scope entry so
    // later bindings and the tail see it. Scoped lookup lives in
    // `env.get`; pops restore the caller env exactly (the plot
    // sampler shares one env across sample points).
    let mut pushed = 0;
    let mut result = Ok(Value::Num(0.0));
    for (name, value) in bindings {
        match evaluate_with_points(value, env, points) {
            Ok(v) => {
                env.push_let_scope([(name.clone(), v)]);
                pushed += 1;
            },
            Err(e) => {
                result = Err(e);
                break;
            },
        }
    }
    if result.is_ok() {
        result = evaluate_with_points(tail, env, points);
    }
    for _ in 0..pushed {
        env.pop_let_scope();
    }
    result
}

pub(crate) fn make_vec_value(values: Vec<Value>) -> Result<Value, EvalError> {
    let all_num = values.iter().all(|v| matches!(v, Value::Num(_)));
    if all_num {
        match values.len() {
            2 => Ok(Value::Vec2([values[0].as_num(), values[1].as_num()])),
            3 => Ok(Value::Vec3([values[0].as_num(), values[1].as_num(), values[2].as_num()])),
            4 => Ok(Value::Vec4([
                values[0].as_num(),
                values[1].as_num(),
                values[2].as_num(),
                values[3].as_num(),
            ])),
            _ => Ok(Value::List(values.into())),
        }
    } else {
        // Heterogeneous or non-numeric tuple (e.g. tuple of colors, strings, or nested tuples) becomes a List
        Ok(Value::List(values.into()))
    }
}

#[cfg(test)]
#[path = "eval/tests.rs"]
mod tests;
