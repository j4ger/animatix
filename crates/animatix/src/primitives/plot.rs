//! Plot primitives: graphs, curves, vector fields, heatmaps, contours, and number planes.

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::Primitive;
use crate::timeline::SceneDimensions;

/// Evaluate the pre-sampled paths every path-only plot primitive draws.
fn evaluate_paths(
    ctx: &crate::primitives::EvaluateCtx,
) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError> {
    use crate::primitives::RenderCommand;
    if ctx.vector_paths.is_empty() {
        Ok(None)
    } else {
        Ok(Some(vec![RenderCommand::Paths {
            paths: ctx.vector_paths.to_vec(),
        }]))
    }
}

/// The `at` / `size` / `x_domain` / `y_domain` preamble shared by every plot
/// primitive's defaults.
fn base_default_props() -> Vec<Property> {
    vec![
        Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
        Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
        Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
    ]
}

/// Declare a path-only plot primitive: shared `evaluate`, plot-assignment
/// handling and base defaults, plus the primitive's extra default properties.
macro_rules! path_plot_primitive {
    (
        $(#[$struct_doc:meta])*
        $ty:ident,
        $(#[$const_doc:meta])*
        $const:ident,
        $name:literal,
        [$($extra:expr),* $(,)?]
    ) => {
        $(#[$struct_doc])*
        pub struct $ty;

        $(#[$const_doc])*
        pub const $const: $ty = $ty;

        impl Primitive for $ty {
            fn type_name(&self) -> &str {
                $name
            }

            fn evaluate(
                &self,
                ctx: &crate::primitives::EvaluateCtx,
                _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
            ) -> Result<
                Option<Vec<crate::primitives::RenderCommand>>,
                crate::renderer::error::RenderError,
            > {
                evaluate_paths(ctx)
            }

            fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
                let mut props = base_default_props();
                props.extend([$($extra),*]);
                props
            }

            fn handle_assignment(
                &self,
                property: &str,
                value: &Expr,
                ctx: &mut crate::primitives::AssignmentCtx,
                env: &crate::timeline::Environment,
                diagnostics: &mut Vec<Diagnostic>,
                subject: &str,
            ) -> bool {
                handle_plot_assignment(property, value, ctx, env, diagnostics, subject)
            }
        }
    };
}

path_plot_primitive!(
    /// The `Graph` plot primitive.
    GraphPrimitive,
    /// Singleton instance of `GraphPrimitive`.
    GRAPH,
    "Graph",
    []
);

/// The `PlotCurve` plot primitive.
pub struct PlotCurvePrimitive;

/// Singleton instance of `PlotCurvePrimitive`.
pub const PLOT_CURVE: PlotCurvePrimitive = PlotCurvePrimitive;

impl Primitive for PlotCurvePrimitive {
    fn type_name(&self) -> &str {
        "PlotCurve"
    }

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::RenderCommand;
        use crate::timeline::TrackAccessor;
        use crate::timeline::path_progress::trim_path_by_progress;

        if ctx.vector_paths.is_empty() {
            return Ok(None);
        }

        let style = ctx.style;
        // Sample stroke_progress from the track
        let progress = style.stroke_progress.get(ctx.time_ms, 1.0) as f64;
        let progress = progress.clamp(0.0, 1.0);

        let paths: Vec<_> = if progress < 1.0 {
            ctx.vector_paths
                .iter()
                .map(|vp| {
                    let mut trimmed = vp.clone();
                    trimmed.path = std::sync::Arc::new(trim_path_by_progress(&vp.path, progress));
                    if progress <= 0.0 {
                        // At zero progress, also hide the stroke entirely
                        trimmed.stroke = None;
                    }
                    trimmed
                })
                .collect()
        } else {
            ctx.vector_paths.to_vec()
        };

        let mut commands = vec![RenderCommand::Paths { paths }];
        // Only ask for the decorations this actor actually authored: the stamp
        // samples five style tracks per call. `dash_pattern` gets an emptiness
        // probe because `insert_end_keyframes` creates that track for every
        // actor; the gradient tracks are created only when authored, so
        // `is_some` is enough and avoids cloning a ramp's stop list.
        let wants_dash = !style.dash_pattern.get(ctx.time_ms, Vec::new()).is_empty();
        let wants_gradient = style.fill_gradient.is_some() || style.stroke_gradient.is_some();
        if wants_dash || wants_gradient {
            crate::primitives::stamp_stroke_decoration(&mut commands, ctx);
        }
        Ok(Some(commands))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        let mut props = base_default_props();
        props.extend([
            Property::new("t_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("kind", Expr::Str("cartesian".into())),
            Property::new(
                "func",
                Expr::Closure(vec!["x".into()], Box::new(Expr::Ident("x".into()))),
            ),
            Property::new("tolerance", Expr::Num(0.5)),
            Property::new("max_depth", Expr::Num(10.0)),
            Property::new("resolution", Expr::Num(96.0)),
        ]);
        props
    }

    fn handle_assignment(
        &self,
        property: &str,
        value: &Expr,
        ctx: &mut crate::primitives::AssignmentCtx,
        env: &crate::timeline::Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
    ) -> bool {
        handle_plot_assignment(property, value, ctx, env, diagnostics, subject)
    }
}

path_plot_primitive!(
    /// The `VectorField` plot primitive.
    VectorFieldPrimitive,
    /// Singleton instance of `VectorFieldPrimitive`.
    VECTOR_FIELD,
    "VectorField",
    [
        Property::new("density", Expr::Num(16.0)),
        Property::new(
            "func",
            Expr::Closure(
                vec!["x".into(), "y".into()],
                Box::new(Expr::Tuple(vec![Expr::Ident("x".into()), Expr::Ident("y".into())])),
            ),
        ),
    ]
);

path_plot_primitive!(
    /// The `Heatmap` plot primitive.
    HeatmapPrimitive,
    /// Singleton instance of `HeatmapPrimitive`.
    HEATMAP,
    "Heatmap",
    [
        Property::new("resolution", Expr::Num(64.0)),
        Property::new(
            "func",
            Expr::Closure(vec!["x".into(), "y".into()], Box::new(Expr::Num(0.0))),
        ),
    ]
);

path_plot_primitive!(
    /// The `ContourSet` plot primitive.
    ContourSetPrimitive,
    /// Singleton instance of `ContourSetPrimitive`.
    CONTOUR_SET,
    "ContourSet",
    [
        Property::new("resolution", Expr::Num(96.0)),
        Property::new(
            "levels",
            Expr::List(vec![Expr::Num(-2.0), Expr::Num(0.0), Expr::Num(2.0)]),
        ),
        Property::new(
            "func",
            Expr::Closure(vec!["x".into(), "y".into()], Box::new(Expr::Num(0.0))),
        ),
    ]
);

path_plot_primitive!(
    /// The `NumberPlane` plot primitive.
    NumberPlanePrimitive,
    /// Singleton instance of `NumberPlanePrimitive`.
    NUMBER_PLANE,
    "NumberPlane",
    [
        Property::new(
            "x_range",
            Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0), Expr::Num(2.0)]),
        ),
        Property::new(
            "y_range",
            Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0), Expr::Num(2.0)]),
        ),
    ]
);

pub(crate) use super::plot_assignment::handle_plot_assignment;

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_default_names(primitive: &dyn Primitive, expected: &[&str]) {
        let scene = SceneDimensions {
            width: 640,
            height: 360,
        };
        let props = primitive.default_props(&scene);
        let names: Vec<&str> = props.iter().map(|prop| prop.name.as_str()).collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn plot_default_props_keep_their_declared_names() {
        assert_default_names(&GRAPH, &["at", "size", "x_domain", "y_domain"]);
        assert_default_names(
            &PLOT_CURVE,
            &[
                "at",
                "size",
                "x_domain",
                "y_domain",
                "t_domain",
                "kind",
                "func",
                "tolerance",
                "max_depth",
                "resolution",
            ],
        );
        assert_default_names(
            &VECTOR_FIELD,
            &["at", "size", "x_domain", "y_domain", "density", "func"],
        );
        assert_default_names(
            &HEATMAP,
            &["at", "size", "x_domain", "y_domain", "resolution", "func"],
        );
        assert_default_names(
            &CONTOUR_SET,
            &[
                "at",
                "size",
                "x_domain",
                "y_domain",
                "resolution",
                "levels",
                "func",
            ],
        );
        assert_default_names(
            &NUMBER_PLANE,
            &["at", "size", "x_domain", "y_domain", "x_range", "y_range"],
        );
    }
}
