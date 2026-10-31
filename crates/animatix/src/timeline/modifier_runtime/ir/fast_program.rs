//! Fast specialized evaluation for modifier IR assignments.
//!
//! Replaces boxed `Value` tree traversal with unboxed `FastScalarExpr` evaluation
//! for numeric and Vec2 property assignments in `always` and `drive` blocks.

use super::eval::{FastScalarExpr, compile_fast_scalar, compile_fast_vec2};
use super::exec::{execute_stmt, record_override};
use super::types::{CompiledExpr, ModifierIrStmt, ModifierOverrides};
use crate::timeline::frame_env::apply_override_incremental;
use crate::timeline::{Environment, EvalError, Value};

/// A compiled fast expression evaluating to either a scalar or a Vec2.
#[derive(Clone, Debug, PartialEq)]
pub enum FastValueExpr {
    /// A single numeric scalar expression.
    Scalar(FastScalarExpr),
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

        let mut slot_names_raw = Vec::new();
        if let Some((x, y)) = compile_fast_vec2(value, "\0", &mut slot_names_raw) {
            let slot_names = slot_names_raw.into_iter().map(String::from).collect();
            return FastStmt::FastAssign(FastAssignStmt {
                target: target.clone(),
                target_key: target_key.clone(),
                property: property.clone(),
                expr: FastValueExpr::Vec2(x, y),
                slot_names,
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
                expr: FastValueExpr::Scalar(s),
                slot_names,
                generic: stmt.clone(),
            });
        }
    }

    FastStmt::Generic(stmt.clone())
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
    let mut slots: Vec<f64> = Vec::with_capacity(8);

    for stmt in &program.statements {
        match stmt {
            FastStmt::FastAssign(assign) => {
                slots.clear();
                let mut fallback_generic = false;
                for name in &assign.slot_names {
                    match frame_env.get_path(name) {
                        Some(Value::Num(n)) => slots.push(n),
                        _ => {
                            fallback_generic = true;
                            break;
                        },
                    }
                }

                if fallback_generic {
                    execute_stmt(&assign.generic, frame_env, overrides)?;
                    continue;
                }

                let value = match &assign.expr {
                    FastValueExpr::Scalar(expr) => {
                        let n = expr.eval(0.0, &slots);
                        Value::Num(n)
                    },
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
                execute_stmt(generic_stmt, frame_env, overrides)?;
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
}
