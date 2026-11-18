//! Vector shape state building: constructs VectorShapeState from actor
//! type and properties, applying defaults and property overrides.

use super::*;
use crate::ast::Property;

impl Timeline {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_vector_shape_state(
        &self,
        label: &str,
        ty: &str,
        props: &[Property],
        time_ms: f64,
        size: [f32; 2],
        line_from: [f32; 2],
        line_to: [f32; 2],
        arc_angles: [f32; 2],
        corner_radius: f32,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> VectorShapeState {
        let eval_env = self.build_eval_env(time_ms as u64);
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        // Callout has its own state struct; resolve via kind instead of a
        // type-name string.
        let is_callout = animatix_std::caps_for_type(ty).is_some_and(|c| c.callout);
        let mut vector_shape_state = if is_callout {
            VectorShapeState::Callout(crate::timeline::shapes::CalloutState::default())
        } else {
            VectorShapeState::new(shape_type, size)
        };
        // Initialize shape-specific fields
        match &mut vector_shape_state {
            VectorShapeState::Line(line) => {
                line.line_from = line_from;
                line.line_to = line_to;
            },
            VectorShapeState::Arrow(arrow) => {
                arrow.from = line_from;
                arrow.to = line_to;
            },
            VectorShapeState::Callout(callout) => {
                callout.from = line_from;
                callout.to = line_to;
            },
            VectorShapeState::Ellipse(ellipse) => {
                ellipse.arc_angles = arc_angles;
            },
            // Seeded from the existing track like the values above, so a
            // re-declaration that omits `corner_radius` keeps the radius it had.
            VectorShapeState::Rect(rect) => {
                rect.corner_radius = corner_radius;
            },
            _ => {}, // Shapes without seeded custom geometry parameters.
        }
        apply_vector_shape_defaults(ty, &mut vector_shape_state);

        for prop in props {
            let prop_subject = format!("{}.{}", label, prop.name);
            match prop.name.as_str() {
                "at" | "anchor" | "offset" => {},
                "corner_radius" => {
                    // Glass also uses Rect-backed state for its frost clip,
                    // despite not exposing a Rect primitive or ShapeKind.
                    let value = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    );
                    if let VectorShapeState::Rect(rect) = &mut vector_shape_state {
                        match value {
                            Some(Value::Num(radius)) => rect.corner_radius = radius as f32,
                            Some(other) => diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!("corner_radius expects a number, got {other:?}"),
                                )
                                .with_subject(&prop_subject),
                            ),
                            None => rect.corner_radius = 0.0,
                        }
                    }
                },
                "size" => {
                    let axes = super::property::evaluate_numeric_size_axes(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    );
                    if let Some(size) = vector_shape_state.size_mut() {
                        for (axis, value) in axes.into_iter().enumerate() {
                            if let Some(value) = value {
                                size[axis] = value;
                            }
                        }
                    } else if axes.into_iter().any(|value| value.is_some()) {
                        diagnostics.push(Diagnostic::warning(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!("size is not supported on {}", ty),
                        ));
                    }
                },
                _ => {
                    let _ = apply_vector_shape_property(
                        ty,
                        &prop.name,
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                        &mut vector_shape_state,
                    );
                },
            }
        }

        finalize_vector_shape_state(ty, &mut vector_shape_state);
        vector_shape_state
    }
}
