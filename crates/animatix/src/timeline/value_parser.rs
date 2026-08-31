//! # Value Parser
//!
//! Generic `Expr → PropertyValue` dispatch driven by `ValueType`.
//!
//! This module implements the per-type parsing logic that replaces the old
//! N×M match-block explosion. It is the single source of truth for converting
//! an AST expression to a typed `PropertyValue`.
//!
//! ## Usage
//!
//! ```ignore
//! let pv = parse_value(schema.value_type, &expr, &env, &mut diagnostics, &subject);
//! ```
//!
//! ## Exceptions
//!
//! The following `ValueType` variants return `None` because they require
//! context-specific handling (group resolution, auto-color, etc.):
//!
//! - `ShapeType` — resolved from actor type + shape kind, not from expressions
//! - `PlacementMode` — set via layout engine, not keyframed directly
//! - `SceneAnchor` — consumed during position binding resolution
//! - `PositionBinding` — built from at + anchor + offset group resolution
//! - `MorphOptions` — parsed from timing modifiers, not property expressions
//! - `BuildTimeOnly` — side-effect properties (func, grid, ticks, etc.)

// Re-use the canonical PropertyValue from property_engine
use super::property_engine::PropertyValue;
use super::{evaluate_expr_with_lookup_diagnostic, parse_color_in_env_with_lookup_diagnostic};
use crate::ast::Expr;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::env::{Environment, Value};
use crate::timeline::property_registry::ValueType;

/// Parse an `Expr` into a `PropertyValue` based on the expected `ValueType`.
///
/// Returns `None` when:
/// - Parsing fails (invalid expression for the expected type)
/// - The value type requires context-specific handling (group properties, build-time-only
///   properties, etc.)
pub(crate) fn parse_value(
    value_type: ValueType,
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<PropertyValue> {
    match value_type {
        ValueType::Enum(variants) => {
            let text = match expr {
                Expr::Ident(name) | Expr::Str(name) => name.clone(),
                _ => {
                    evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?.as_str()
                },
            };
            if variants.contains(&text.as_str()) {
                Some(PropertyValue::Enum(text))
            } else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!(
                            "'{}' expects one of {}, got '{}'",
                            subject,
                            variants.join(" | "),
                            text
                        ),
                    )
                    .with_subject(subject),
                );
                None
            }
        },
        ValueType::Sum(variants) => {
            for variant in variants {
                if let Some(literal) = variant.literal {
                    let value =
                        evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject);
                    let matches_literal = match (&literal, &value) {
                        (
                            super::property_registry::SumLiteral::Bool(expected),
                            Some(Value::Bool(actual)),
                        ) => expected == actual,
                        (
                            super::property_registry::SumLiteral::Str(expected),
                            Some(Value::Str(actual)),
                        ) => expected == actual,
                        _ => false,
                    };
                    if matches_literal
                        && let Some(payload) =
                            parse_value(variant.value_type, expr, env, diagnostics, subject)
                    {
                        return Some(PropertyValue::Variant {
                            name: variant.name.to_string(),
                            value: Box::new(payload),
                        });
                    }
                }
            }
            for variant in variants {
                let mut branch_diagnostics = Vec::new();
                if let Some(payload) =
                    parse_value(variant.value_type, expr, env, &mut branch_diagnostics, subject)
                {
                    return Some(PropertyValue::Variant {
                        name: variant.name.to_string(),
                        value: Box::new(payload),
                    });
                }
            }
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("'{}' expects a supported choice, got an unsupported value", subject),
                )
                .with_subject(subject),
            );
            None
        },
        ValueType::Union(variants) => {
            for variant in variants {
                let mut branch_diagnostics = Vec::new();
                if let Some(value) =
                    parse_value(*variant, expr, env, &mut branch_diagnostics, subject)
                {
                    return Some(value);
                }
            }
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "'{}' expects {}, got an unsupported value",
                        subject,
                        union_type_name(variants)
                    ),
                )
                .with_subject(subject),
            );
            None
        },
        ValueType::F32 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            Some(PropertyValue::F32(v.as_num() as f32))
        },
        ValueType::U32 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            let n = v.as_num();
            Some(PropertyValue::U32(n.max(0.0) as u32))
        },
        ValueType::Vec2 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Vec2([x, y]) => Some(PropertyValue::Vec2([x as f32, y as f32])),
                _ => None,
            }
        },
        ValueType::Vec4 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Vec4([a, b, c, d]) => {
                    Some(PropertyValue::Vec4([a as f32, b as f32, c as f32, d as f32]))
                },
                Value::Color([a, b, c, d]) => {
                    Some(PropertyValue::Vec4([a as f32, b as f32, c as f32, d as f32]))
                },
                _ => None,
            }
        },
        ValueType::Color => {
            parse_color_in_env_with_lookup_diagnostic("", "color", expr, env, diagnostics, subject)
                .map(PropertyValue::Color)
        },
        ValueType::String => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Str(s) => Some(PropertyValue::String(s)),
                _ => None,
            }
        },
        ValueType::Bool => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Bool(b) => Some(PropertyValue::Bool(b)),
                _ => None,
            }
        },
        ValueType::PointList => {
            let items = match expr {
                Expr::List(items) => items,
                _ => {
                    // Allow a variable/expression that evaluates to List(Vec2)
                    match evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject) {
                        Some(Value::List(items)) => {
                            let mut points = Vec::with_capacity(items.len());
                            for item in items {
                                if let Value::Vec2([x, y]) = item {
                                    points.push([x as f32, y as f32]);
                                } else {
                                    diagnostics.push(
                                        Diagnostic::warning(
                                            DiagnosticCode::InvalidPropertyValue,
                                            DiagnosticPhase::Build,
                                            format!("PointList expected Vec2, got {:?}", item),
                                        )
                                        .with_subject(subject),
                                    );
                                }
                            }
                            return if points.is_empty() {
                                None
                            } else {
                                Some(PropertyValue::PointList(points))
                            };
                        },
                        _ => return None,
                    }
                },
            };
            let mut points = Vec::with_capacity(items.len());
            for item in items {
                let pair = match item {
                    Expr::Tuple(t) if t.len() == 2 => {
                        let x =
                            evaluate_expr_with_lookup_diagnostic(&t[0], env, diagnostics, subject);
                        let y =
                            evaluate_expr_with_lookup_diagnostic(&t[1], env, diagnostics, subject);
                        match (x, y) {
                            (Some(Value::Num(xv)), Some(Value::Num(yv))) => [xv as f32, yv as f32],
                            _ => {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        DiagnosticCode::InvalidPropertyValue,
                                        DiagnosticPhase::Build,
                                        "PointList point coordinates must be numbers".to_string(),
                                    )
                                    .with_subject(subject),
                                );
                                continue;
                            },
                        }
                    },
                    _ => {
                        match evaluate_expr_with_lookup_diagnostic(item, env, diagnostics, subject)
                        {
                            Some(Value::Vec2([x, y])) => [x as f32, y as f32],
                            _ => {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        DiagnosticCode::InvalidPropertyValue,
                                        DiagnosticPhase::Build,
                                        "PointList expected Vec2 point".to_string(),
                                    )
                                    .with_subject(subject),
                                );
                                continue;
                            },
                        }
                    },
                };
                points.push(pair);
            }
            if points.is_empty() {
                None
            } else {
                Some(PropertyValue::PointList(points))
            }
        },
        ValueType::CommandList => crate::timeline::parse_path_commands_expr(expr, env)
            .map(|path| PropertyValue::CommandList(path.to_svg())),
        ValueType::Transform => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Vec2([a, b]) => {
                    Some(PropertyValue::Transform([a as f32, b as f32, 0.0, 1.0, 0.0, 0.0]))
                },
                Value::Vec4([a, b, c, d]) => Some(PropertyValue::Transform([
                    a as f32, b as f32, c as f32, d as f32, 0.0, 0.0,
                ])),
                Value::List(items) if items.len() == 6 => {
                    let mut arr = [0.0f32; 6];
                    for (i, item) in items.iter().enumerate() {
                        arr[i] = item.as_num() as f32;
                    }
                    Some(PropertyValue::Transform(arr))
                },
                _ => None,
            }
        },
        // These types require context-specific handling (group resolution,
        // auto-color, etc.) and cannot be parsed generically.
        ValueType::ShapeType
        | ValueType::PlacementMode
        | ValueType::SceneAnchor
        | ValueType::PositionBinding
        | ValueType::MorphOptions
        | ValueType::BuildTimeOnly => None,
        ValueType::CalloutPlace => {
            use crate::timeline::animation_track::CalloutPlace;
            // Accept bare identifier or quoted string: `right`, `"right"`, etc.
            let s = match expr {
                Expr::Ident(s) | Expr::Str(s) => s.as_str(),
                _ => {
                    match evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject) {
                        Some(v) => {
                            let owned = v.as_str();
                            return if let Some(p) = CalloutPlace::from_str(&owned) {
                                Some(PropertyValue::Enum(p.as_str().to_string()))
                            } else {
                                diagnostics.push(crate::diagnostics::Diagnostic::warning(
                                    crate::diagnostics::DiagnosticCode::InvalidPropertyValue,
                                    crate::diagnostics::DiagnosticPhase::Build,
                                    format!("'place' expects right|left|top|bottom|auto, got '{owned}'"),
                                ).with_subject(subject));
                                None
                            };
                        },
                        None => return None,
                    }
                },
            };
            if let Some(p) = CalloutPlace::from_str(s) {
                Some(PropertyValue::Enum(p.as_str().to_string()))
            } else {
                diagnostics.push(
                    crate::diagnostics::Diagnostic::warning(
                        crate::diagnostics::DiagnosticCode::InvalidPropertyValue,
                        crate::diagnostics::DiagnosticPhase::Build,
                        format!("'place' expects right|left|top|bottom|auto, got '{s}'"),
                    )
                    .with_subject(subject),
                );
                None
            }
        },
    }
}

fn union_type_name(types: &[ValueType]) -> String {
    types.iter().map(|ty| value_type_name(*ty)).collect::<Vec<_>>().join(" | ")
}

/// Convert a runtime `Value` back to a declarative AST expression.
///
/// This is the inverse of the general `Expr → PropertyValue` parsing and is
/// used to translate plugin-provided default property values into source
/// defaults. Returns `None` for values that cannot be authored as a scalar
/// property expression (functions, closures, objects).
#[cfg_attr(
    not(feature = "plugin-loading"),
    allow(dead_code) // Consumed by the native-plugin adapter's `default_props`
                     // (extension_native_plugin.rs) only when the
                     // `plugin-loading` feature is enabled.
)]
pub fn value_to_expr(value: &crate::timeline::Value) -> Option<Expr> {
    use crate::timeline::Value;
    match value {
        Value::Num(n) => Some(Expr::Num(*n)),
        Value::Str(s) => Some(Expr::Str(s.clone())),
        Value::Bool(b) => Some(Expr::Bool(*b)),
        Value::Vec2(v) => Some(Expr::Tuple(vec![Expr::Num(v[0]), Expr::Num(v[1])])),
        Value::Vec3(v) => {
            Some(Expr::Tuple(vec![Expr::Num(v[0]), Expr::Num(v[1]), Expr::Num(v[2])]))
        },
        Value::Vec4(v) | Value::Color(v) => Some(Expr::Tuple(vec![
            Expr::Num(v[0]),
            Expr::Num(v[1]),
            Expr::Num(v[2]),
            Expr::Num(v[3]),
        ])),
        Value::List(items) => {
            items.iter().map(value_to_expr).collect::<Option<Vec<_>>>().map(Expr::List)
        },
        // Functions/closures/objects cannot be authored as scalar defaults.
        Value::NativeFn(_) | Value::Closure(..) | Value::UserFn { .. } | Value::Object(..) => None,
    }
}

fn value_type_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::F32 => "Num",
        ValueType::U32 => "Int",
        ValueType::Bool => "Bool",
        ValueType::Vec2 => "Vec2",
        ValueType::Vec4 => "Vec4",
        ValueType::Color => "Color",
        ValueType::String => "Str",
        ValueType::ShapeType => "ShapeType",
        ValueType::PlacementMode => "PlacementMode",
        ValueType::SceneAnchor => "SceneAnchor",
        ValueType::PositionBinding => "PositionBinding",
        ValueType::MorphOptions => "MorphOptions",
        ValueType::CalloutPlace => "CalloutPlace",
        ValueType::PointList => "PointList",
        ValueType::CommandList => "CommandList",
        ValueType::Transform => "Transform",
        ValueType::BuildTimeOnly => "BuildTimeOnly",
        ValueType::Union(_) => "Union",
        ValueType::Sum(_) => "Choice",
        ValueType::Enum(_) => "Enum",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_to_expr_roundtrips_scalars_and_tuples() {
        use crate::timeline::Value;
        assert_eq!(value_to_expr(&Value::Num(7.0)), Some(Expr::Num(7.0)));
        assert_eq!(value_to_expr(&Value::Str("hi".to_string())), Some(Expr::Str("hi".to_string())));
        assert_eq!(value_to_expr(&Value::Bool(true)), Some(Expr::Bool(true)));
        assert_eq!(
            value_to_expr(&Value::Vec2([1.0, 2.0])),
            Some(Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0)]))
        );
        assert_eq!(
            value_to_expr(&Value::Color([0.2, 0.4, 0.6, 1.0])),
            Some(Expr::Tuple(vec![
                Expr::Num(0.2),
                Expr::Num(0.4),
                Expr::Num(0.6),
                Expr::Num(1.0)
            ]))
        );
    }

    #[test]
    fn value_to_expr_rejects_non_scalar_values() {
        use crate::timeline::Value;
        let env_closure = Value::Closure(
            Vec::new(),
            Box::new(crate::ir::CompiledExpr::Const(crate::timeline::Value::Num(0.0))),
            crate::timeline::CapturedEnv::default(),
        );
        assert!(value_to_expr(&env_closure).is_none());
    }

    #[test]
    fn enum_parses_allowed_choice() {
        let env = Environment::new();
        let mut diagnostics = Vec::new();
        let value = parse_value(
            ValueType::Enum(&["visible", "hidden"]),
            &Expr::Ident("hidden".to_string()),
            &env,
            &mut diagnostics,
            "overflow",
        );
        assert_eq!(value, Some(PropertyValue::Enum("hidden".to_string())));
    }

    #[test]
    fn sum_parses_named_variants() {
        let env = Environment::new();
        let mut diagnostics = Vec::new();
        let sum = ValueType::Sum(crate::timeline::property_registry::LEGEND_SUM_VARIANTS);

        let parsed_hidden = parse_value(sum, &Expr::Bool(false), &env, &mut diagnostics, "legend");
        assert_eq!(
            parsed_hidden,
            Some(PropertyValue::Variant {
                name: "hidden".to_string(),
                value: Box::new(PropertyValue::Bool(false)),
            })
        );

        let parsed_label =
            parse_value(sum, &Expr::Str("Revenue".to_string()), &env, &mut diagnostics, "legend");
        assert_eq!(
            parsed_label,
            Some(PropertyValue::Variant {
                name: "label".to_string(),
                value: Box::new(PropertyValue::String("Revenue".to_string())),
            })
        );
    }

    #[test]
    fn union_parses_bool_and_string_variants() {
        let env = Environment::new();
        let mut diagnostics = Vec::new();
        let union = ValueType::Union(&[ValueType::Bool, ValueType::String]);

        let parsed_bool = parse_value(union, &Expr::Bool(false), &env, &mut diagnostics, "legend");
        assert_eq!(parsed_bool, Some(PropertyValue::Bool(false)));

        let parsed_string =
            parse_value(union, &Expr::Str("Revenue".to_string()), &env, &mut diagnostics, "legend");
        assert_eq!(parsed_string, Some(PropertyValue::String("Revenue".to_string())));

        let parsed_number = parse_value(union, &Expr::Num(42.0), &env, &mut diagnostics, "legend");
        assert_eq!(parsed_number, None);
        assert!(!diagnostics.is_empty(), "union should report an unsupported value");
    }
}
