//! Shared `gap` / `padding` / `align` parsing for layout containers.
//!
//! `Row`, `Col`, `Grid` and `Stack` read these three properties the same way;
//! keeping one parser here means tuple handling and diagnostics cannot drift
//! between them. Kind-specific properties (`vertical_align`, `cols`) and the
//! per-container build warnings stay with their primitive.

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::unreadable_layout_value;
use crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::utils::evaluate_expr;
use crate::timeline::{Environment, Value};

/// The container properties every layout container reads identically.
#[derive(Debug, Default)]
pub(crate) struct ContainerProps {
    /// `[main-axis, cross-axis]` gap; `0` when absent or unreadable.
    pub(crate) gap: [f32; 2],
    /// `[left, top, right, bottom]` padding; `0` when absent or unreadable.
    pub(crate) padding: [f32; 4],
    /// Explicit cross-axis alignment string, when authored and readable.
    pub(crate) align: Option<String>,
    /// Whether the author declared `gap` at all — `Stack` warns it is ignored.
    pub(crate) saw_gap: bool,
}

/// Parse the `gap` / `padding` / `align` properties shared by every layout
/// container.
///
/// `warn_unreadable` controls whether an unreadable `gap` is reported: `Row`,
/// `Col` and `Grid` warn with [`unreadable_layout_value`], while `Stack`
/// declares gap meaningless and emits its own `IgnoredStackGap` warning
/// instead. `padding` never warns; it keeps the default when unreadable.
pub(crate) fn parse_container_props(
    props: &[Property],
    env: &Environment,
    label: &str,
    diagnostics: &mut Vec<Diagnostic>,
    warn_unreadable: bool,
) -> ContainerProps {
    let mut parsed = ContainerProps::default();

    for prop in props {
        match prop.name.as_str() {
            "gap" => {
                parsed.saw_gap = true;
                match &prop.value {
                    Expr::Tuple(items) if items.len() == 2 => {
                        match (evaluate_expr(&items[0], env), evaluate_expr(&items[1], env)) {
                            (Ok(Value::Num(a)), Ok(Value::Num(b))) => {
                                parsed.gap = [a as f32, b as f32];
                            },
                            _ => {
                                if warn_unreadable {
                                    diagnostics.push(unreadable_layout_value(label, "gap"));
                                }
                            },
                        }
                    },
                    _ => match evaluate_expr(&prop.value, env) {
                        Ok(Value::Num(n)) => parsed.gap = [n as f32, n as f32],
                        _ => {
                            if warn_unreadable {
                                diagnostics.push(unreadable_layout_value(label, "gap"));
                            }
                        },
                    },
                }
            },
            "padding" => {
                match &prop.value {
                    Expr::Tuple(items) if items.len() == 4 => {
                        if let (
                            Ok(Value::Num(a)),
                            Ok(Value::Num(b)),
                            Ok(Value::Num(c)),
                            Ok(Value::Num(d)),
                        ) = (
                            evaluate_expr(&items[0], env),
                            evaluate_expr(&items[1], env),
                            evaluate_expr(&items[2], env),
                            evaluate_expr(&items[3], env),
                        ) {
                            parsed.padding = [a as f32, b as f32, c as f32, d as f32];
                        }
                    },
                    Expr::Tuple(items) if items.len() == 2 => {
                        // (horizontal, vertical) → [h, v, h, v]
                        if let (Ok(Value::Num(h)), Ok(Value::Num(v))) =
                            (evaluate_expr(&items[0], env), evaluate_expr(&items[1], env))
                        {
                            parsed.padding = [h as f32, v as f32, h as f32, v as f32];
                        }
                    },
                    _ => {
                        if let Ok(Value::Num(n)) = evaluate_expr(&prop.value, env) {
                            let v = n as f32;
                            parsed.padding = [v, v, v, v];
                        }
                    },
                }
            },
            "align" => {
                if let Some(v) =
                    evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, label)
                {
                    parsed.align = Some(v.as_str().to_string());
                }
            },
            _ => {},
        }
    }

    parsed
}
