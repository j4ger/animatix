//! Unified actor template pipeline for insertion and instantiation.

use animatix::timeline::{SceneDimensions, Timeline};
use animatix_syntax::ast::{Expr, Property};

/// A fully prepared template for creating a new actor in the document.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorTemplate {
    /// Primitive actor type name (e.g. `"Rect"`, `"Text"`, `"Graph"`).
    pub ty: String,
    /// Unique label for the new actor (e.g. `"rect1"`, `"graph2"`).
    pub label: String,
    /// Initial properties for the actor declaration.
    pub props: Vec<Property>,
}

impl ActorTemplate {
    /// Create an actor template for the given primitive type at an optional drop position.
    ///
    /// If `drop_position` is `None`, defaults to the center of `scene_dimensions`.
    pub fn for_primitive(
        type_name: &str,
        drop_position: Option<[f32; 2]>,
        scene_dimensions: SceneDimensions,
        timeline: Option<&Timeline>,
    ) -> Self {
        Self::with_suggested_label(
            type_name,
            None,
            drop_position,
            scene_dimensions,
            timeline,
            Vec::new(),
        )
    }

    /// Create an actor template with an optional suggested label and extra property overrides.
    pub fn with_suggested_label(
        type_name: &str,
        suggested_label: Option<String>,
        drop_position: Option<[f32; 2]>,
        scene_dimensions: SceneDimensions,
        timeline: Option<&Timeline>,
        extra_props: Vec<Property>,
    ) -> Self {
        let label = suggested_label
            .unwrap_or_else(|| crate::app::utils::labels::unique_label(timeline, type_name));

        let target_pos = drop_position.unwrap_or([
            scene_dimensions.width as f32 / 2.0,
            scene_dimensions.height as f32 / 2.0,
        ]);

        let registry = timeline.map(Timeline::primitive_registry_snapshot);
        let primitive = registry
            .as_deref()
            .and_then(|r| r.find(type_name))
            .or_else(|| animatix::primitives::find_primitive(type_name));

        let mut props = if let Some(primitive) = primitive {
            primitive.default_props(&scene_dimensions)
        } else {
            Vec::new()
        };

        // If default_props was empty, check default_stub in the registry info
        if props.is_empty() {
            let info = registry
                .as_deref()
                .and_then(|r| r.info_of(type_name))
                .or_else(|| animatix::primitives::primitive_info_by_name(type_name));
            if let Some(info) = info {
                if let Some(stub) = &info.default_stub {
                    let dummy_src = format!("_dummy: {type_name}, {stub}");
                    if let (Some(stmts), _, _) =
                        animatix_syntax::parser::parse_source_diagnostics(&dummy_src)
                    {
                        if let Some(animatix_syntax::ast::Stmt::ActorDecl {
                            props: parsed_props,
                            ..
                        }) = stmts.first()
                        {
                            props = parsed_props.clone();
                        }
                    }
                }
            }
        }

        // Ensure `at` property matches the target position
        let at_expr = Expr::Tuple(vec![
            Expr::Num(target_pos[0] as f64),
            Expr::Num(target_pos[1] as f64),
        ]);

        if let Some(existing_at) = props.iter_mut().find(|p| p.name == "at") {
            existing_at.value = at_expr;
        } else {
            props.insert(
                0,
                Property {
                    name: "at".into(),
                    value: at_expr,
                    value_span: None,
                    trailing_comment: None,
                },
            );
        }

        // Apply any extra property overrides
        for extra in extra_props {
            if let Some(existing) = props.iter_mut().find(|p| p.name == extra.name) {
                *existing = extra;
            } else {
                props.push(extra);
            }
        }

        Self {
            ty: type_name.to_string(),
            label,
            props,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_template_creates_rect_with_sensible_defaults() {
        let scene = SceneDimensions {
            width: 1920,
            height: 1080,
        };
        let template = ActorTemplate::for_primitive("Rect", Some([300.0, 400.0]), scene, None);
        assert_eq!(template.ty, "Rect");
        assert_eq!(template.label, "rect1");
        assert!(template.props.iter().any(|p| p.name == "at"));
        assert!(template.props.iter().any(|p| p.name == "size"));
        assert!(template.props.iter().any(|p| p.name == "color"));

        let at = template.props.iter().find(|p| p.name == "at").unwrap();
        assert_eq!(at.value, Expr::Tuple(vec![Expr::Num(300.0), Expr::Num(400.0)]));
    }

    #[test]
    fn actor_template_creates_text_with_sensible_defaults() {
        let scene = SceneDimensions {
            width: 1920,
            height: 1080,
        };
        let template = ActorTemplate::for_primitive("Text", None, scene, None);
        assert_eq!(template.ty, "Text");
        assert_eq!(template.label, "text1");
        assert!(template.props.iter().any(|p| p.name == "at"));
        assert!(template.props.iter().any(|p| p.name == "text"));
    }
}
