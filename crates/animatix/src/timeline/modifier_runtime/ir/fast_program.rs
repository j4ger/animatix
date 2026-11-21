//! Fast specialized evaluation for modifier IR assignments.
//!
//! Replaces boxed `Value` tree traversal with unboxed `FastScalarExpr` evaluation
//! for numeric and Vec2 property assignments in `always` and `drive` blocks.

use super::exec::{execute_stmt, record_override};
use super::fast_scalar::{FastScalarExpr, compile_fast_scalar, compile_fast_vec2};
use super::types::{BuiltinFn, CompiledExpr, ModifierIrStmt, ModifierOverrides};
use crate::timeline::frame_env::apply_override_incremental;
use crate::timeline::{Environment, EvalError, Value};

/// A compiled fast expression evaluating to either a scalar or a Vec2.
#[derive(Clone, Debug, PartialEq)]
pub enum FastValueExpr {
    /// A single numeric scalar expression.
    Scalar(FastScalarExpr),
    /// A Boolean expression represented as 0/1 inside the scalar evaluator.
    Bool(FastScalarExpr),
    /// A 2D vector expression of two scalar components (X, Y).
    Vec2(FastScalarExpr, FastScalarExpr),
}

/// A specialized fast property assignment statement.
#[derive(Clone, Debug, PartialEq)]
pub struct FastAssignStmt {
    /// Object path segments.
    pub target: Vec<String>,
    /// Precomputed joined target path (`target.join(".")`).
    pub target_key: String,
    /// Property name to assign.
    pub property: String,
    /// The specialized fast expression to evaluate.
    pub expr: FastValueExpr,
    /// Environment variable names that must be resolved into slot indices.
    pub slot_names: Vec<String>,
    /// Unqualified built-ins whose lexical bindings must stay canonical for
    /// this expression to use its intrinsic fast path.
    pub builtin_names: Vec<String>,
    /// Generic fallback statement if runtime slot types are non-numeric.
    pub generic: ModifierIrStmt,
}

/// A statement in a fast modifier program.
#[derive(Clone, Debug, PartialEq)]
pub enum FastStmt {
    /// A specialized fast assignment statement.
    FastAssign(FastAssignStmt),
    /// A generic IR statement falling back to general evaluation.
    Generic(ModifierIrStmt),
}

/// A specialized modifier program with fast paths for numeric assignments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FastModifierProgram {
    /// Statements in this program.
    pub statements: Vec<FastStmt>,
}

/// Compile a single IR statement into a specialized `FastStmt` if possible.
pub fn compile_fast_stmt(stmt: &ModifierIrStmt) -> FastStmt {
    if let ModifierIrStmt::Assign {
        target,
        target_key,
        property,
        value,
    } = stmt
    {
        // A bare LoadEnv may refer to a vector or complex type; keep it generic.
        if matches!(value, CompiledExpr::LoadEnv(_)) {
            return FastStmt::Generic(stmt.clone());
        }
        let builtin_names = unqualified_builtin_names(value);

        let mut slot_names_raw = Vec::new();
        if let Some((x, y)) = compile_fast_vec2(value, "\0", &mut slot_names_raw) {
            let slot_names = slot_names_raw.into_iter().map(String::from).collect();
            return FastStmt::FastAssign(FastAssignStmt {
                target: target.clone(),
                target_key: target_key.clone(),
                property: property.clone(),
                expr: FastValueExpr::Vec2(x, y),
                slot_names,
                builtin_names: builtin_names.clone(),
                generic: stmt.clone(),
            });
        }

        if let CompiledExpr::Const(Value::Vec2([x, y])) = value {
            return FastStmt::FastAssign(FastAssignStmt {
                target: target.clone(),
                target_key: target_key.clone(),
                property: property.clone(),
                expr: FastValueExpr::Vec2(FastScalarExpr::Const(*x), FastScalarExpr::Const(*y)),
                slot_names: Vec::new(),
                builtin_names: Vec::new(),
                generic: stmt.clone(),
            });
        }

        let mut slot_names_raw = Vec::new();
        if let Some(s) = compile_fast_scalar(value, "\0", &mut slot_names_raw) {
            let slot_names = slot_names_raw.into_iter().map(String::from).collect();
            return FastStmt::FastAssign(FastAssignStmt {
                target: target.clone(),
                target_key: target_key.clone(),
                property: property.clone(),
                expr: if super::fast_scalar::boolean_result(value) {
                    FastValueExpr::Bool(s)
                } else {
                    FastValueExpr::Scalar(s)
                },
                slot_names,
                builtin_names,
                generic: stmt.clone(),
            });
        }
    }

    FastStmt::Generic(stmt.clone())
}

fn unqualified_builtin_names(expression: &CompiledExpr) -> Vec<String> {
    fn builtin_name(builtin: &BuiltinFn) -> &'static str {
        match builtin {
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
        }
    }

    fn walk(expression: &CompiledExpr, names: &mut Vec<String>) {
        match expression {
            CompiledExpr::Const(_)
            | CompiledExpr::LoadEnv(_)
            | CompiledExpr::Timeline(_)
            | CompiledExpr::SequencePlan(_)
            | CompiledExpr::StaggerPlan(..)
            | CompiledExpr::PropRef { .. }
            | CompiledExpr::AnchorLookup { .. } => {},
            CompiledExpr::Field(base, _)
            | CompiledExpr::Unary(_, base)
            | CompiledExpr::Closure(_, base) => walk(base, names),
            CompiledExpr::MakeVec(items) | CompiledExpr::MakeList(items) => {
                for item in items {
                    walk(item, names);
                }
            },
            CompiledExpr::Binary(left, _, right) => {
                walk(left, names);
                walk(right, names);
            },
            CompiledExpr::Select(condition, then_expr, else_expr) => {
                walk(condition, names);
                walk(then_expr, names);
                walk(else_expr, names);
            },
            CompiledExpr::CallBuiltin(builtin, args) => {
                names.push(builtin_name(builtin).to_string());
                for arg in args {
                    walk(arg, names);
                }
            },
            CompiledExpr::CallNamed(_, args) => {
                for (_, arg) in args {
                    walk(arg, names);
                }
            },
            CompiledExpr::CallMember(receiver, _, args) => {
                walk(receiver, names);
                for (_, arg) in args {
                    walk(arg, names);
                }
            },
            CompiledExpr::CallValue(callee, args) => {
                walk(callee, names);
                for (_, arg) in args {
                    walk(arg, names);
                }
            },
            CompiledExpr::Index(base, index) => {
                walk(base, names);
                walk(index, names);
            },
            CompiledExpr::Construct(_, fields) => {
                for (_, field) in fields {
                    walk(field, names);
                }
            },
            CompiledExpr::LetChain(bindings, tail) => {
                for (_, value) in bindings {
                    walk(value, names);
                }
                walk(tail, names);
            },
        }
    }

    let mut names = Vec::new();
    walk(expression, &mut names);
    names.sort();
    names.dedup();
    names
}

impl FastModifierProgram {
    /// Compile a slice of IR statements into a specialized `FastModifierProgram`.
    pub fn compile(stmts: &[ModifierIrStmt]) -> FastModifierProgram {
        FastModifierProgram {
            statements: stmts.iter().map(compile_fast_stmt).collect(),
        }
    }
}

/// Execute a specialized fast modifier program against a frame environment.
pub fn execute_fast_modifier_ir(
    program: &FastModifierProgram,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
) -> Result<(), EvalError> {
    execute_fast_with_geometry(program, frame_env, overrides, None)
}

pub(crate) fn execute_fast_with_geometry(
    program: &FastModifierProgram,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
    geometry: Option<super::ModifierGeometry<'_>>,
) -> Result<(), EvalError> {
    execute_fast_statements(&program.statements, frame_env, overrides, geometry)
}

pub(super) fn execute_fast_statements(
    statements: &[FastStmt],
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
    geometry: Option<super::ModifierGeometry<'_>>,
) -> Result<(), EvalError> {
    let mut slots = smallvec::SmallVec::<[f64; 8]>::new();

    for stmt in statements {
        match stmt {
            FastStmt::FastAssign(assign) => {
                slots.clear();
                let mut fallback_generic = assign
                    .builtin_names
                    .iter()
                    .any(|name| !crate::timeline::builtins::is_standard_callable(name, frame_env));
                for name in &assign.slot_names {
                    if fallback_generic {
                        break;
                    }
                    match frame_env.get_path(name) {
                        Some(Value::Num(n)) => slots.push(n),
                        _ => {
                            fallback_generic = true;
                            break;
                        },
                    }
                }

                if fallback_generic {
                    execute_stmt(&assign.generic, frame_env, overrides, geometry)?;
                    continue;
                }

                let value = match &assign.expr {
                    FastValueExpr::Scalar(expr) => {
                        let n = expr.eval(0.0, &slots);
                        Value::Num(n)
                    },
                    FastValueExpr::Bool(expr) => Value::Bool(expr.eval(0.0, &slots) != 0.0),
                    FastValueExpr::Vec2(x_expr, y_expr) => {
                        let x = x_expr.eval(0.0, &slots);
                        let y = y_expr.eval(0.0, &slots);
                        Value::Vec2([x, y])
                    },
                };

                // Frame-local object field writes take priority over actor/property overrides.
                if let Some((root, rest)) = assign.target_key.split_once('.') {
                    let path: Vec<String> =
                        rest.split('.').map(|segment| segment.to_string()).collect();
                    if frame_env.set_object_path(&path, root, &assign.property, value.clone()) {
                        continue;
                    }
                } else if frame_env.set_object_field(
                    &assign.target_key,
                    &assign.property,
                    value.clone(),
                ) {
                    continue;
                }

                record_override(overrides, &assign.target_key, &assign.property, &value);
                apply_override_incremental(frame_env, &assign.target_key, &assign.property, value);
            },
            FastStmt::Generic(generic_stmt) => {
                execute_stmt(generic_stmt, frame_env, overrides, geometry)?;
            },
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinaryOp;

    #[test]
    fn fast_boolean_assignment_preserves_the_generic_value_kind() {
        let statement = ModifierIrStmt::Assign {
            target: vec!["state".into()],
            target_key: "state".into(),
            property: "enabled".into(),
            value: CompiledExpr::Binary(
                Box::new(CompiledExpr::LoadEnv("t".into())),
                BinaryOp::Gt,
                Box::new(CompiledExpr::Const(Value::Num(1.0))),
            ),
        };
        let mut generic = Environment::new();
        generic.set(
            "state",
            Value::Object(
                "State".into(),
                std::collections::HashMap::from([("enabled".into(), Value::Bool(false))]),
            ),
        );
        generic.set("t", Value::Num(2.0));
        let mut fast = generic.clone();
        execute_stmt(&statement, &mut generic, &mut ModifierOverrides::default(), None).unwrap();
        let program = FastModifierProgram::compile(&[statement]);
        execute_fast_modifier_ir(&program, &mut fast, &mut ModifierOverrides::default()).unwrap();
        assert_eq!(fast.get_path("state.enabled"), Some(Value::Bool(true)));
        assert_eq!(fast.get("state"), generic.get("state"));
    }

    #[test]
    fn test_compile_fast_scalar_assign() {
        let stmt = ModifierIrStmt::Assign {
            target: vec!["pulse".to_string()],
            target_key: "pulse".to_string(),
            property: "opacity".to_string(),
            value: CompiledExpr::Binary(
                Box::new(CompiledExpr::LoadEnv("t".to_string())),
                BinaryOp::Div,
                Box::new(CompiledExpr::Const(Value::Num(2.0))),
            ),
        };

        let fast_stmt = compile_fast_stmt(&stmt);
        match fast_stmt {
            FastStmt::FastAssign(assign) => {
                assert_eq!(assign.target_key, "pulse");
                assert_eq!(assign.property, "opacity");
                assert_eq!(assign.slot_names, vec!["t"]);
                assert!(matches!(assign.expr, FastValueExpr::Scalar(_)));
            },
            FastStmt::Generic(_) => panic!("Expected FastAssign"),
        }
    }

    #[test]
    fn test_compile_fast_vec2_assign() {
        let stmt = ModifierIrStmt::Assign {
            target: vec!["orbiter".to_string()],
            target_key: "orbiter".to_string(),
            property: "at".to_string(),
            value: CompiledExpr::MakeVec(vec![
                CompiledExpr::Binary(
                    Box::new(CompiledExpr::Const(Value::Num(640.0))),
                    BinaryOp::Add,
                    Box::new(CompiledExpr::LoadEnv("x".to_string())),
                ),
                CompiledExpr::Binary(
                    Box::new(CompiledExpr::Const(Value::Num(390.0))),
                    BinaryOp::Add,
                    Box::new(CompiledExpr::LoadEnv("y".to_string())),
                ),
            ]),
        };

        let fast_stmt = compile_fast_stmt(&stmt);
        match fast_stmt {
            FastStmt::FastAssign(assign) => {
                assert_eq!(assign.target_key, "orbiter");
                assert_eq!(assign.property, "at");
                assert_eq!(assign.slot_names, vec!["x", "y"]);
                assert!(matches!(assign.expr, FastValueExpr::Vec2(_, _)));
            },
            FastStmt::Generic(_) => panic!("Expected FastAssign"),
        }
    }

    #[test]
    fn test_execute_fast_modifier_ir_scalar_and_vec2() {
        let stmts = vec![
            ModifierIrStmt::Assign {
                target: vec!["pulse".to_string()],
                target_key: "pulse".to_string(),
                property: "opacity".to_string(),
                value: CompiledExpr::Binary(
                    Box::new(CompiledExpr::LoadEnv("t".to_string())),
                    BinaryOp::Div,
                    Box::new(CompiledExpr::Const(Value::Num(2.0))),
                ),
            },
            ModifierIrStmt::Assign {
                target: vec!["orbiter".to_string()],
                target_key: "orbiter".to_string(),
                property: "at".to_string(),
                value: CompiledExpr::MakeVec(vec![
                    CompiledExpr::Binary(
                        Box::new(CompiledExpr::Const(Value::Num(100.0))),
                        BinaryOp::Add,
                        Box::new(CompiledExpr::LoadEnv("t".to_string())),
                    ),
                    CompiledExpr::Const(Value::Num(200.0)),
                ]),
            },
        ];

        let program = FastModifierProgram::compile(&stmts);
        assert_eq!(program.statements.len(), 2);

        let mut env = Environment::new();
        env.set("t", Value::Num(4.0));
        let mut overrides = ModifierOverrides::default();

        execute_fast_modifier_ir(&program, &mut env, &mut overrides).expect("fast eval succeeds");

        assert_eq!(overrides["pulse"]["opacity"], Value::Num(2.0));
        assert_eq!(overrides["orbiter"]["at"], Value::Vec2([104.0, 200.0]));
        assert_eq!(env.get_path("pulse.opacity"), Some(Value::Num(2.0)));
        assert_eq!(env.get_path("orbiter.at"), Some(Value::Vec2([104.0, 200.0])));
    }

    #[test]
    fn test_execute_fast_fallback_for_non_numeric_slot() {
        let stmt = ModifierIrStmt::Assign {
            target: vec!["panel".to_string()],
            target_key: "panel".to_string(),
            property: "color".to_string(),
            value: CompiledExpr::LoadEnv("my_color".to_string()),
        };

        let program = FastModifierProgram::compile(&[stmt]);
        let mut env = Environment::new();
        env.set("my_color", Value::Color([1.0, 0.0, 0.0, 1.0]));
        let mut overrides = ModifierOverrides::default();

        execute_fast_modifier_ir(&program, &mut env, &mut overrides).expect("fallback succeeds");
        assert_eq!(overrides["panel"]["color"], Value::Color([1.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn shadowed_builtin_uses_lexical_callable_instead_of_fast_intrinsic() {
        let stmt = ModifierIrStmt::Assign {
            target: vec!["panel".to_string()],
            target_key: "panel".to_string(),
            property: "opacity".to_string(),
            value: CompiledExpr::CallBuiltin(
                BuiltinFn::Sin,
                vec![CompiledExpr::LoadEnv("t".to_string())],
            ),
        };
        let mut ir =
            crate::timeline::modifier_runtime::ir::ModifierIrProgram::without_fast(vec![stmt]);
        crate::timeline::modifier_runtime::ir::optimize_program(
            &mut ir,
            &std::collections::HashSet::new(),
        );
        assert!(matches!(
            &ir.statements[0],
            ModifierIrStmt::Assign {
                value: CompiledExpr::CallBuiltin(BuiltinFn::Sin, _),
                ..
            }
        ));
        let program = FastModifierProgram::compile(&ir.statements);
        let mut env = Environment::new();
        env.set("t", Value::Num(0.5));
        env.set(
            "sin",
            Value::NativeFn(std::sync::Arc::new(|args, _| match args {
                [Value::Num(value)] => Ok(Value::Num(value + 2.0)),
                _ => Err(EvalError::TypeMismatch("sin expects one number".into())),
            })),
        );
        let mut overrides = ModifierOverrides::default();

        execute_fast_modifier_ir(&program, &mut env, &mut overrides)
            .expect("shadowed callable evaluates through the generic path");

        assert_eq!(overrides["panel"]["opacity"], Value::Num(2.5));
    }
}
