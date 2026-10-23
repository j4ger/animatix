/// Describes a property edit made in the inspector panel.
#[derive(Debug, Clone)]
pub struct PropertyEdit {
    pub actor: String,
    pub property: String,
    pub value: PropertyValue,
    /// When true, create a keyframe at current time instead of overwriting defaults.
    pub create_keyframe: bool,
    /// Optional explicit time override (seconds). When set, the edit applies at
    /// this time instead of the current playback time. Used by motion path editing.
    pub time_s: Option<f64>,
}

/// The typed value of a property edit.
///
/// This is the same schema-driven value model used by the core timeline so GUI
/// edits, inspector display, and engine evaluation do not need a parallel enum.
pub type PropertyValue = animatix::timeline::PropertyValue;

pub(crate) fn property_value_to_expr(
    pv: PropertyValue,
) -> Result<animatix_syntax::ast::Expr, String> {
    let expr = match &pv {
        PropertyValue::Vec2([x, y]) => animatix_syntax::ast::Expr::Tuple(vec![
            animatix_syntax::ast::Expr::Num(*x as f64),
            animatix_syntax::ast::Expr::Num(*y as f64),
        ]),
        PropertyValue::F32(v) => animatix_syntax::ast::Expr::Num(*v as f64),
        PropertyValue::Color([r, g, b, a]) | PropertyValue::Vec4([r, g, b, a]) => {
            if (*a - 1.0).abs() < 0.001 && r.fract() == 0.0 && g.fract() == 0.0 && b.fract() == 0.0
            {
                animatix_syntax::ast::Expr::Call(
                    "rgb".into(),
                    vec![
                        animatix_syntax::ast::Expr::Num((*r * 255.0) as i64 as f64),
                        animatix_syntax::ast::Expr::Num((*g * 255.0) as i64 as f64),
                        animatix_syntax::ast::Expr::Num((*b * 255.0) as i64 as f64),
                    ],
                )
            } else {
                animatix_syntax::ast::Expr::Call(
                    "rgba".into(),
                    vec![
                        animatix_syntax::ast::Expr::Num(*r as f64),
                        animatix_syntax::ast::Expr::Num(*g as f64),
                        animatix_syntax::ast::Expr::Num(*b as f64),
                        animatix_syntax::ast::Expr::Num(*a as f64),
                    ],
                )
            }
        },
        PropertyValue::Bool(b) => animatix_syntax::ast::Expr::Bool(*b),
        PropertyValue::U32(v) => animatix_syntax::ast::Expr::Num(*v as f64),
        PropertyValue::String(s) => animatix_syntax::ast::Expr::Str(s.clone()),
        PropertyValue::Enum(s) => animatix_syntax::ast::Expr::Str(s.clone()),
        PropertyValue::StringList(items) => animatix_syntax::ast::Expr::List(
            items.iter().map(|s| animatix_syntax::ast::Expr::Str(s.clone())).collect(),
        ),
        PropertyValue::PointList(points) => animatix_syntax::ast::Expr::List(
            points
                .iter()
                .map(|&[x, y]| {
                    animatix_syntax::ast::Expr::Tuple(vec![
                        animatix_syntax::ast::Expr::Num(x as f64),
                        animatix_syntax::ast::Expr::Num(y as f64),
                    ])
                })
                .collect(),
        ),
        PropertyValue::Transform(matrix) => animatix_syntax::ast::Expr::List(
            matrix.iter().map(|v| animatix_syntax::ast::Expr::Num(*v as f64)).collect(),
        ),
        PropertyValue::Variant { value, .. } => property_value_to_expr((**value).clone())?,
        other => return Err(format!("cannot serialize property value {other:?}")),
    };
    crate::validation::validate_roundtrip(&expr, &pv)?;
    Ok(expr)
}

// =========================================================================
// ── Undo / redo ─────────────────────────────────────────────────────────
// =========================================================================
