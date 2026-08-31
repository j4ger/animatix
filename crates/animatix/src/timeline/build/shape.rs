//! Vector shape state building: constructs VectorShapeState from actor
//! type and properties, applying defaults and property overrides.

use super::*;
use crate::ast::Property;

impl Timeline {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_vector_shape_state(
        &self,
        ty: &str,
        props: &[Property],
        time_ms: f64,
        size: [f32; 2],
        line_from: [f32; 2],
        line_to: [f32; 2],
        arc_angles: [f32; 2],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> VectorShapeState {
        let eval_env = self.build_eval_env(time_ms as u64);
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        // Callout has its own state struct; resolve via kind instead of a
        // type-name string.
        let is_callout =
            super::ActorKindId::from_type_name(ty) == Some(super::ActorKindId::Callout);
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
            _ => {},
        }
        apply_vector_shape_defaults(ty, &mut vector_shape_state);

        for prop in props {
            let prop_subject = format!("{}.{}", ty, prop.name);
            match prop.name.as_str() {
                "at" | "anchor" | "offset" => {},
                "size" => {
                    let size_val = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([w, h]) = size_val {
                        if let Some(size) = vector_shape_state.size_mut() {
                            *size = [w as f32 / 2.0, h as f32 / 2.0];
                        } else {
                            diagnostics.push(Diagnostic::warning(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!("size is not supported on {}", ty),
                            ));
                        }
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
