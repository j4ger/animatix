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

use crate::ast::Expr;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::env::{Environment, Value};
use crate::timeline::property_registry::ValueType;

use super::{
    evaluate_expr_with_lookup_diagnostic, parse_color_in_env_with_lookup_diagnostic,
};

// Re-use the canonical PropertyValue from property_engine
use super::property_engine::PropertyValue;

/// Parse an `Expr` into a `PropertyValue` based on the expected `ValueType`.
///
/// Returns `None` when:
/// - Parsing fails (invalid expression for the expected type)
/// - The value type requires context-specific handling (group properties,
///   build-time-only properties, etc.)
pub(crate) fn parse_value(
    value_type: ValueType,
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<PropertyValue> {
    match value_type {
        ValueType::F32 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            Some(PropertyValue::F32(v.as_num() as f32))
        }
        ValueType::U32 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            let n = v.as_num();
            Some(PropertyValue::U32(n.max(0.0) as u32))
        }
        ValueType::Vec2 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Vec2([x, y]) => Some(PropertyValue::Vec2([x as f32, y as f32])),
                _ => None,
            }
        }
        ValueType::Vec4 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Vec4([a, b, c, d]) => Some(PropertyValue::Vec4([a as f32, b as f32, c as f32, d as f32])),
                Value::Color([a, b, c, d]) => Some(PropertyValue::Vec4([a as f32, b as f32, c as f32, d as f32])),
                _ => None,
            }
        }
        ValueType::Color => {
            parse_color_in_env_with_lookup_diagnostic("", "color", expr, env, diagnostics, subject)
                .map(PropertyValue::Color)
        }
        ValueType::String => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            Some(PropertyValue::String(v.as_str()))
        }
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
                                    diagnostics.push(Diagnostic::warning(
                                        DiagnosticCode::InvalidPropertyValue,
                                        DiagnosticPhase::Build,
                                        format!("PointList expected Vec2, got {:?}", item),
                                    ).with_subject(subject));
                                }
                            }
                            return if points.is_empty() { None } else { Some(PropertyValue::PointList(points)) };
                        }
                        _ => return None,
                    }
                }
            };
            let mut points = Vec::with_capacity(items.len());
            for item in items {
                let pair = match item {
                    Expr::Tuple(t) if t.len() == 2 => {
                        let x = evaluate_expr_with_lookup_diagnostic(&t[0], env, diagnostics, subject);
                        let y = evaluate_expr_with_lookup_diagnostic(&t[1], env, diagnostics, subject);
                        match (x, y) {
                            (Some(Value::Num(xv)), Some(Value::Num(yv))) => [xv as f32, yv as f32],
                            _ => {
                                diagnostics.push(Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    "PointList point coordinates must be numbers".to_string(),
                                ).with_subject(subject));
                                continue;
                            }
                        }
                    }
                    _ => {
                        match evaluate_expr_with_lookup_diagnostic(item, env, diagnostics, subject) {
                            Some(Value::Vec2([x, y])) => [x as f32, y as f32],
                            _ => {
                                diagnostics.push(Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    "PointList expected Vec2 point".to_string(),
                                ).with_subject(subject));
                                continue;
                            }
                        }
                    }
                };
                points.push(pair);
            }
            if points.is_empty() { None } else { Some(PropertyValue::PointList(points)) }
        }
        ValueType::CommandList => {
            crate::timeline::parse_path_commands_expr(expr, env)
                .map(|path| PropertyValue::CommandList(path.to_svg()))
        }
        ValueType::Transform => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Vec2([a, b]) => Some(PropertyValue::Transform([a as f32, b as f32, 0.0, 1.0, 0.0, 0.0])),
                Value::Vec4([a, b, c, d]) => Some(PropertyValue::Transform([a as f32, b as f32, c as f32, d as f32, 0.0, 0.0])),
                Value::List(items) if items.len() == 6 => {
                    let mut arr = [0.0f32; 6];
                    for (i, item) in items.iter().enumerate() {
                        arr[i] = item.as_num() as f32;
                    }
                    Some(PropertyValue::Transform(arr))
                }
                _ => None,
            }
        }
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
                                Some(PropertyValue::CalloutPlace(p))
                            } else {
                                diagnostics.push(crate::diagnostics::Diagnostic::warning(
                                    crate::diagnostics::DiagnosticCode::InvalidPropertyValue,
                                    crate::diagnostics::DiagnosticPhase::Build,
                                    format!("'place' expects right|left|top|bottom|auto, got '{owned}'"),
                                ).with_subject(subject));
                                None
                            };
                        }
                        None => return None,
                    }
                }
            };
            if let Some(p) = CalloutPlace::from_str(s) {
                Some(PropertyValue::CalloutPlace(p))
            } else {
                diagnostics.push(crate::diagnostics::Diagnostic::warning(
                    crate::diagnostics::DiagnosticCode::InvalidPropertyValue,
                    crate::diagnostics::DiagnosticPhase::Build,
                    format!("'place' expects right|left|top|bottom|auto, got '{s}'"),
                ).with_subject(subject));
                None
            }
        }
    }
}