//! Viewport container primitive.
//!
//! A viewport displays another scene's content in a rectangular region
//! (picture-in-picture). Viewports are declared via `viewport` statements
//! at the AST level, but they can also be used as actor primitives.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Viewport` primitive.
pub struct ViewportPrimitive;

/// Singleton instance of [`ViewportPrimitive`].
pub const VIEWPORT: ViewportPrimitive = ViewportPrimitive;

impl Primitive for ViewportPrimitive {
    fn type_name(&self) -> &'static str {
        "Viewport"
    }
    fn display_name(&self) -> &'static str {
        "Viewport"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Container
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::CIRCLE
    }
    fn is_advanced(&self) -> bool {
        true
    }
    fn is_container(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Viewport
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // When Viewport is used as an actor declaration (e.g. `vp: Viewport,
        // scene: "Other"`), extract the relevant properties and register
        // a viewport in the timeline.

        // Helper to extract a vec2 from an expression (e.g. (100, 200))
        let eval_vec2 = |expr: &Expr| -> Option<[f32; 2]> {
            match expr {
                Expr::Tuple(items) if items.len() == 2 => {
                    match (&items[0], &items[1]) {
                        (Expr::Num(x), Expr::Num(y)) => Some([*x as f32, *y as f32]),
                        _ => None,
                    }
                }
                _ => None,
            }
        };

        // Helper to extract a string from an expression
        let eval_string = |expr: &Expr| -> Option<String> {
            match expr {
                Expr::Str(s) => Some(s.clone()),
                _ => None,
            }
        };

        // Helper to extract a number from an expression
        let eval_num = |expr: &Expr| -> Option<f32> {
            match expr {
                Expr::Num(n) => Some(*n as f32),
                _ => None,
            }
        };

        // Extract "at" (position)
        let position = props.iter().find(|p| p.name == "at" || p.name == "position")
            .and_then(|p| eval_vec2(&p.value))
            .unwrap_or([0.0, 0.0]);

        // Extract "size"
        let size = props.iter().find(|p| p.name == "size")
            .and_then(|p| eval_vec2(&p.value))
            .unwrap_or([320.0, 240.0]);

        // Extract "scene"
        let scene = props.iter().find(|p| p.name == "scene")
            .and_then(|p| eval_string(&p.value))
            .unwrap_or_default();

        // Extract "opacity"
        let opacity = props.iter().find(|p| p.name == "opacity")
            .and_then(|p| eval_num(&p.value))
            .unwrap_or(1.0);

        // Extract "border"
        let border = props.iter().find(|p| p.name == "border")
            .and_then(|p| eval_num(&p.value));

        // Extract "border_color"
        let border_color = props.iter()
            .find(|p| p.name == "border_color" || p.name == "stroke")
            .and_then(|p| match &p.value {
                Expr::Str(s) => {
                    // Try to parse simple color names
                    match s.as_str() {
                        "white" | "White" => Some([1.0, 1.0, 1.0, 1.0]),
                        "black" | "Black" => Some([0.0, 0.0, 0.0, 1.0]),
                        "red" | "Red" => Some([1.0, 0.0, 0.0, 1.0]),
                        "green" | "Green" => Some([0.0, 1.0, 0.0, 1.0]),
                        "blue" | "Blue" => Some([0.0, 0.0, 1.0, 1.0]),
                        _ => Some([1.0, 1.0, 1.0, 1.0]), // default white
                    }
                }
                _ => None,
            });

        // Extract "mask"
        let mask = props.iter().find(|p| p.name == "mask")
            .and_then(|p| match &p.value {
                Expr::Str(s) => Some(s.clone()),
                Expr::Ident(s) => Some(s.clone()),
                _ => None,
            });

        let viewport = crate::timeline::Viewport {
            label: label.to_string(),
            position,
            size,
            opacity,
            border,
            border_color,
            scene,
            mask,
        };
        ctx.timeline.viewports.push(viewport);

        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(240.0)])),
            Property::new("scene", Expr::Str(String::new())),
            Property::new("opacity", Expr::Num(1.0)),
            Property::new("border", Expr::Num(0.0)),
            Property::new("border_color", Expr::Str("white".to_string())),
        ]
    }
}