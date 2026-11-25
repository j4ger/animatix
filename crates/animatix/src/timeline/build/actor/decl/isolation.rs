use super::*;

impl Timeline {
    pub(super) fn set_group_opacity_isolation(
        &mut self,
        id: EntityId,
        props: &[Property],
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(prop) = props.iter().rev().find(|prop| prop.name == "isolate_opacity") else {
            return;
        };
        match crate::timeline::evaluate_expr(&prop.value, env) {
            Ok(Value::Bool(isolate)) => self.world.geometry_mut(id).isolate_opacity = isolate,
            _ => diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    "Group isolate_opacity expects a boolean",
                )
                .with_subject(format!(
                    "{}.isolate_opacity",
                    self.world.label(id).unwrap_or_default()
                ))
                .with_byte_span(prop.value_span),
            ),
        }
    }
}
