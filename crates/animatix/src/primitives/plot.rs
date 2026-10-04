//! Plot primitives: graphs, curves, vector fields, heatmaps, contours, and number planes.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Graph` plot primitive.
pub struct GraphPrimitive;

/// Singleton instance of `GraphPrimitive`.
pub const GRAPH: GraphPrimitive = GraphPrimitive;

/// The `PlotCurve` plot primitive.
pub struct PlotCurvePrimitive;

/// Singleton instance of `PlotCurvePrimitive`.
pub const PLOT_CURVE: PlotCurvePrimitive = PlotCurvePrimitive;

impl Primitive for GraphPrimitive {
    fn type_name(&self) -> &str {
        "Graph"
    }
    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::RenderCommand;
        if ctx.vector_paths.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vec![RenderCommand::Paths {
                paths: ctx.vector_paths.to_vec(),
            }]))
        }
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        ]
    }
}

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

        // Sample stroke_progress from the track
        let progress = ctx.track.style.stroke_progress.get(ctx.time_ms, 1.0) as f64;
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
        let wants_dash = !ctx.track.style.dash_pattern.get(ctx.time_ms, Vec::new()).is_empty();
        let wants_gradient =
            ctx.track.style.fill_gradient.is_some() || ctx.track.style.stroke_gradient.is_some();
        if wants_dash || wants_gradient {
            crate::primitives::stamp_stroke_decoration(&mut commands, ctx);
        }
        Ok(Some(commands))
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("t_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("kind", Expr::Str("cartesian".into())),
            Property::new(
                "func",
                Expr::Closure(vec!["x".into()], Box::new(Expr::Ident("x".into()))),
            ),
            Property::new("tolerance", Expr::Num(0.5)),
            Property::new("max_depth", Expr::Num(10.0)),
            Property::new("resolution", Expr::Num(96.0)),
        ]
    }
}

/// The `VectorField` plot primitive.
pub struct VectorFieldPrimitive;

/// Singleton instance of `VectorFieldPrimitive`.
pub const VECTOR_FIELD: VectorFieldPrimitive = VectorFieldPrimitive;

impl Primitive for VectorFieldPrimitive {
    fn type_name(&self) -> &str {
        "VectorField"
    }
    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::RenderCommand;
        if ctx.vector_paths.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vec![RenderCommand::Paths {
                paths: ctx.vector_paths.to_vec(),
            }]))
        }
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("density", Expr::Num(16.0)),
            Property::new(
                "func",
                Expr::Closure(
                    vec!["x".into(), "y".into()],
                    Box::new(Expr::Tuple(vec![Expr::Ident("x".into()), Expr::Ident("y".into())])),
                ),
            ),
        ]
    }
}

/// The `Heatmap` plot primitive.
pub struct HeatmapPrimitive;

/// Singleton instance of `HeatmapPrimitive`.
pub const HEATMAP: HeatmapPrimitive = HeatmapPrimitive;

impl Primitive for HeatmapPrimitive {
    fn type_name(&self) -> &str {
        "Heatmap"
    }
    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::RenderCommand;
        if ctx.vector_paths.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vec![RenderCommand::Paths {
                paths: ctx.vector_paths.to_vec(),
            }]))
        }
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("resolution", Expr::Num(64.0)),
            Property::new(
                "func",
                Expr::Closure(vec!["x".into(), "y".into()], Box::new(Expr::Num(0.0))),
            ),
        ]
    }
}

/// The `ContourSet` plot primitive.
pub struct ContourSetPrimitive;

/// Singleton instance of `ContourSetPrimitive`.
pub const CONTOUR_SET: ContourSetPrimitive = ContourSetPrimitive;

impl Primitive for ContourSetPrimitive {
    fn type_name(&self) -> &str {
        "ContourSet"
    }
    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::RenderCommand;
        if ctx.vector_paths.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vec![RenderCommand::Paths {
                paths: ctx.vector_paths.to_vec(),
            }]))
        }
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
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
    }
}

pub struct NumberPlanePrimitive;
/// Singleton instance of `NumberPlanePrimitive`.
pub const NUMBER_PLANE: NumberPlanePrimitive = NumberPlanePrimitive;

impl Primitive for NumberPlanePrimitive {
    fn type_name(&self) -> &str {
        "NumberPlane"
    }
    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::RenderCommand;
        if ctx.vector_paths.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vec![RenderCommand::Paths {
                paths: ctx.vector_paths.to_vec(),
            }]))
        }
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new(
                "x_range",
                Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0), Expr::Num(2.0)]),
            ),
            Property::new(
                "y_range",
                Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0), Expr::Num(2.0)]),
            ),
        ]
    }
}
