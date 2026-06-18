//! Property resolution: extract actor properties from AST declarations,
//! map math coordinates to screen pixels for graph children.

use super::*;
use crate::ast::Property;

impl Timeline {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn extract_actor_properties(
        &self,
        label: &str,
        _ty: &str,
        props: &[Property],
        time_ms: f64,
        _existing_track: &AnimationTrack,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> ExtractedActorProperties {
        let initial_eval_env = self.build_eval_env(time_ms as u64);
        let mut x_domain = [-10.0, 10.0];
        let mut y_domain = [-10.0, 10.0];
        let mut t_domain = [0.0, std::f64::consts::TAU];
        let mut func = None;
        let mut initial_size = DEFAULT_LAYOUT_HALF_SIZE;
        let mut tolerance = 0.5;
        let mut max_depth = 10.0;
        let mut resolution = 96.0;
        let mut kind: Option<PlotCurveKind> = None;
        let mut at_expr: Option<Expr> = None;
        let mut anchor_expr: Option<Expr> = None;
        let mut offset_expr: Option<Expr> = None;

        for prop in props {
            let prop_subject = format!("{}.{}", label, prop.name);
            match prop.name.as_str() {
                "size" => {
                    // Parse the size spec for percentage/auto/fill/fit support
                    // Store it on the track later (in process_actor_decl)
                    // For initial_size calculation (used by plot actors), try numeric tuple first,
                    // fall back to default if percentage/auto
                    let size_val = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([w, h]) = size_val {
                        initial_size[0] = w as f32 / 2.0;
                        initial_size[1] = h as f32 / 2.0;
                    }
                }
                "radius" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    let r = v.as_num() as f32;
                    initial_size = [r, r];
                }
                "x_domain" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([min, max]) = v {
                        x_domain = [min, max];
                    }
                }
                "y_domain" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([min, max]) = v {
                        y_domain = [min, max];
                    }
                }
                "t_domain" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([min, max]) = v {
                        t_domain = [min, max];
                    }
                }
                "func" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Closure(args, body) = v {
                        func = Some((args, body));
                    }
                }
                "tolerance" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    tolerance = v.as_num();
                }
                "max_depth" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    max_depth = v.as_num();
                }
                "resolution" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(96.0));
                    resolution = v.as_num();
                }
                "kind" => {
                    if let Expr::Str(s) = &prop.value {
                        kind = PlotCurveKind::from_str(s);
                    } else if let Expr::Ident(s) = &prop.value {
                        kind = PlotCurveKind::from_str(s);
                    }
                }
                "at" => at_expr = Some(prop.value.clone()),
                "anchor" => anchor_expr = Some(prop.value.clone()),
                "offset" => offset_expr = Some(prop.value.clone()),
                _ => {}
            }
        }

        ExtractedActorProperties {
            initial_size,
            x_domain,
            y_domain,
            t_domain,
            func,
            tolerance,
            max_depth,
            resolution,
            kind,
            at_expr,
            anchor_expr,
            offset_expr,
            gap: 0.0,
            padding: 0.0,
            align: None,
            cols: None,
        }
    }

    /// When an actor is declared inside a Graph, map its position properties
    /// from math coordinates to screen pixels based on the parent's domain and size.
    pub(super) fn map_props_to_graph_parent(
        &self,
        parent_label: &str,
        props: &[Property],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<Property> {
        let eval_env = self.build_eval_env(time_ms as u64);

        // Look up parent's coordinate system from the environment.
        let x_domain = self
            .env
            .get(&format!("{}_x_domain", parent_label))
            .and_then(|v| match v {
                Value::Vec2(d) => Some(d),
                _ => None,
            })
            .unwrap_or([-10.0, 10.0]);
        let y_domain = self
            .env
            .get(&format!("{}_y_domain", parent_label))
            .and_then(|v| match v {
                Value::Vec2(d) => Some(d),
                _ => None,
            })
            .unwrap_or([-10.0, 10.0]);
        let p_size = self
            .env
            .get(&format!("{}_size", parent_label))
            .and_then(|v| match v {
                Value::Vec2(s) => Some(s),
                _ => None,
            })
            .unwrap_or([500.0, 500.0]);
        let parent_pos = self
            .tracks
            .get(parent_label)
            .map(|t| t.position.last([0.0, 0.0]))
            .unwrap_or([0.0, 0.0]);

        let half_w = p_size[0] / 2.0;
        let half_h = p_size[1] / 2.0;

        props
            .iter()
            .map(|prop| {
                let needs_mapping = prop.name == "at"
                    || prop.name == "position"
                    || prop.name == "from"
                    || prop.name == "to";
                if !needs_mapping {
                    return prop.clone();
                }

                let val = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    &eval_env,
                    diagnostics,
                    &format!("{}.{}", parent_label, prop.name),
                );
                let (mx, my) = match val {
                    Some(Value::Vec2([x, y])) => (x, y),
                    _ => return prop.clone(),
                };

                // Math-to-screen mapping (same formula as plot curves):
                // offset_x = half_w * (-1.0 + 2.0 * (mx - x_min) / (x_max - x_min))
                // offset_y = half_h * (1.0 - 2.0 * (my - y_min) / (y_max - y_min))
                let x_range = x_domain[1] - x_domain[0];
                let y_range = y_domain[1] - y_domain[0];
                let offset_x = if x_range != 0.0 {
                    half_w * (-1.0 + 2.0 * (mx - x_domain[0]) / x_range)
                } else {
                    0.0
                };
                let offset_y = if y_range != 0.0 {
                    half_h * (1.0 - 2.0 * (my - y_domain[0]) / y_range)
                } else {
                    0.0
                };

                let screen_x = parent_pos[0] as f64 + offset_x;
                let screen_y = parent_pos[1] as f64 + offset_y;

                Property::new(
                    &prop.name,
                    Expr::Tuple(vec![Expr::Num(screen_x), Expr::Num(screen_y)]),
                )
            })
            .collect()
    }
}