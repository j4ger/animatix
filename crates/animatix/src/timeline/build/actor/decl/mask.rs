use super::*;
use crate::ast::Expr;
use crate::timeline::animation_track::MaskClip;

impl Timeline {
    pub(super) fn set_mask_clip(
        &mut self,
        id: crate::timeline::EntityId,
        props: &[Property],
        env: &crate::timeline::Environment,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(prop) = props.iter().rev().find(|prop| prop.name == "clip") else {
            return;
        };
        let label = match crate::timeline::evaluate_expr(&prop.value, env) {
            Ok(Value::ActorRef(actor)) => Some(actor.label().to_owned()),
            // The Mask is built before its child declarations. Component
            // expansion and plan mounting have already remapped this expression.
            Err(_) if !matches!(prop.value, Expr::Str(_)) => {
                crate::timeline::lookup::parse_actor_ref_literal(&prop.value)
                    .filter(|label| env.get_path(label).is_none())
            },
            _ => None,
        };
        if let Some(label) = label {
            self.world.geometry_mut(id).mask_clip = Some(MaskClip { label, id: None });
        } else {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    "Mask clip must be a reference to one of its direct children",
                )
                .with_subject(format!("{}.clip", self.world.label(id).unwrap_or_default()))
                .with_byte_span(prop.value_span),
            );
        }
    }
}

pub(in crate::timeline::build) fn bind_mask_clips(
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let world = &mut timeline.world;
    let masks: Vec<_> = world
        .tracks
        .iter()
        .filter_map(|(_, id)| world.geometry(id).mask_clip.as_ref().map(|_| id))
        .collect();
    for mask in masks {
        let clip = world.geometry(mask).mask_clip.as_ref().expect("collected explicit clips");
        let clip_id = world
            .resolve_path(&clip.label.split('.').collect::<Vec<_>>())
            .and_then(|label| world.get_id(&label))
            .filter(|&id| world.hierarchy(id).parent == Some(mask));
        if clip_id.is_none() {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("Mask clip '{}' must be one of its direct children", clip.label),
                )
                .with_subject(format!("{}.clip", world.label(mask).unwrap_or_default())),
            );
        }
        if let Some(clip) = &mut world.geometry_mut(mask).mask_clip {
            clip.id = clip_id;
        }
    }
}
