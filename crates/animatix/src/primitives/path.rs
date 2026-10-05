//! Custom path shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::primitives::{BuildCtx, Primitive, RenderCtx};
use crate::timeline::path_data::parse_svg_path_data;
use crate::timeline::shapes::parse_path_commands_expr;
use crate::timeline::{
    Environment, SceneDimensions, TrackAccessor, Value, VectorShapeState, VelloPath,
};

/// The `Path` primitive.
pub struct PathPrimitive;

/// Singleton instance of `PathPrimitive`.
pub const PATH: PathPrimitive = PathPrimitive;

impl Primitive for PathPrimitive {
    fn type_name(&self) -> &str {
        "Path"
    }

    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let VectorShapeState::Path(state) = ctx.state else {
            return None;
        };
        let path =
            std::sync::Arc::new(state.custom_path.clone().unwrap_or_else(kurbo::BezPath::new));
        Some(vec![crate::timeline::shapes::build_vello_path(
            path,
            ctx.style.color,
            ctx.style.stroke_color,
            ctx.style.stroke_width,
            ctx.style.fill_opacity,
            false,
        )])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new(
                "commands",
                Expr::List(vec![
                    Expr::Call("move_to".into(), vec![Expr::Num(-50.0), Expr::Num(-50.0)]),
                    Expr::Call("line_to".into(), vec![Expr::Num(50.0), Expr::Num(-50.0)]),
                    Expr::Call("line_to".into(), vec![Expr::Num(50.0), Expr::Num(50.0)]),
                    Expr::Call("line_to".into(), vec![Expr::Num(-50.0), Expr::Num(50.0)]),
                    Expr::Call("close".into(), vec![]),
                ]),
            ),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::evaluate_shape_render;
        use crate::timeline::shapes::PathState;

        let half_size = ctx
            .track
            .geometry
            .size
            .get(ctx.time_ms, crate::timeline::DEFAULT_LAYOUT_HALF_SIZE);
        let vector_paths = ctx.track.evaluate_vector_paths(ctx.time_ms);

        let mut state = PathState {
            size: half_size,
            custom_path: vector_paths.first().map(|vp| vp.path.as_ref().clone()),
        };

        if let Some(overrides) = ctx.overrides {
            if let Some(s) = crate::primitives::override_size(overrides) {
                state.size = s;
            }
        }

        evaluate_shape_render(self, ctx, &VectorShapeState::Path(state))
    }

    fn apply_property(
        &self,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        let VectorShapeState::Path(path) = state else {
            return false;
        };
        match name {
            "commands" => {
                path.custom_path = parse_path_commands_expr(value, env);
                true
            },
            "icon" => {
                // `icon:` is a build-time alias for an authored `commands:` list.
                // Resolve the name, expand its verbatim path data into command
                // expressions with the single shared parser, and hand the result
                // to the same geometry path a `commands:` list would take — so
                // `draw-in` traces it identically. A non-string value is reported
                // by `icon_name` (nothing to draw); we still return `true` so the
                // property is not silently re-routed to the `commands` handler.
                if let Some(icon_name) = icon_name(value, env, diagnostics, subject) {
                    match animatix_core::stroke_icons::stroke_icon_path_data(&icon_name) {
                        Some(subpaths) => {
                            let commands = icon_commands_expr(subpaths);
                            path.custom_path = parse_path_commands_expr(&commands, env);
                        },
                        None => {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "unknown icon `{icon_name}`; available icons: {}",
                                        suggest_icons(&icon_name)
                                    ),
                                )
                                .with_subject(subject),
                            );
                        },
                    }
                }
                true
            },
            _ => false,
        }
    }
}

/// Expand one or more verbatim path-data subpaths into a `commands:` list
/// expression. Each subpath is parsed with a fresh origin so a leading relative
/// moveto (`m…`) resolves against `(0, 0)` exactly as a separate `<path>` element
/// does in the source SVG; the resulting command runs are concatenated.
fn icon_commands_expr(subpaths: &[&str]) -> Expr {
    let mut commands = Vec::new();
    for d in subpaths {
        if let Expr::List(items) = parse_svg_path_data(d) {
            commands.extend(items);
        }
    }
    Expr::List(commands)
}

/// Read the `icon:` string. Handles a literal `Expr::Str` and any expression
/// that evaluates to a string; a wrong type is warned about rather than dropped.
fn icon_name(
    value: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<String> {
    if let Expr::Str(s) = value {
        return Some(s.clone());
    }
    match crate::timeline::evaluate_expr(value, env) {
        Ok(Value::Str(s)) => Some(s),
        Ok(other) => {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("`icon` expects an icon-name string, got {other:?}"),
                )
                .with_subject(subject),
            );
            None
        },
        // The evaluation error is already surfaced as a diagnostic elsewhere.
        Err(_) => None,
    }
}

/// A short, deterministic list of icon names closest to `query`, used to make an
/// unknown-icon warning actionable without a full edit-distance table.
fn suggest_icons(query: &str) -> String {
    let names = animatix_core::stroke_icons::stroke_icon_names();
    let needle = query.to_ascii_lowercase();
    let mut scored: Vec<(usize, &&str)> = names
        .iter()
        .map(|name| {
            let lower = name.to_ascii_lowercase();
            // Cheaper heuristic score: shared leading segment, then substring,
            // then disjoint. Smaller is closer.
            let score = if lower == needle {
                0
            } else if lower.split('-').next() == needle.split('-').next() {
                1
            } else if lower.contains(&needle) || needle.contains(&lower) {
                2
            } else {
                3
            };
            (score, name)
        })
        .collect();
    scored.sort_by_key(|(score, name)| (*score, *name));
    let picks: Vec<&str> = scored.into_iter().take(4).map(|(_, name)| *name).collect();
    picks.join(", ")
}
