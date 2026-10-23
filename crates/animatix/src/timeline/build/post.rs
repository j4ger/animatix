//! Post-build passes: synthesized idle tracks and namespace export seeding.

use super::*;

pub(super) fn synthesize_idle_tracks(timeline: &mut Timeline) {
    use crate::timeline::TrackAccessor;
    let duration_ms = ((timeline.duration_seconds() * 1000.0).round() as u64).max(3000);
    for track in timeline.world.tracks.values_mut() {
        if let Some(crate::timeline::property_engine::PropertyValue::String(idle_mode)) =
            crate::timeline::read_property_value(
                track,
                crate::timeline::ActorField::Tagged("idle"),
                0,
            )
        {
            let period_ms: u64 = 3000;
            let num_cycles = duration_ms.div_ceil(period_ms);
            match idle_mode.as_str() {
                "float" => {
                    let base_offset = track.geometry.motion_offset.get(0, [0.0, 0.0]);
                    let mot = track.geometry.motion_offset.ensure(base_offset);
                    for c in 0..num_cycles {
                        let t0 = c * period_ms;
                        mot.add_keyframe(t0, base_offset, crate::timeline::Easing::EaseInOut);
                        mot.add_keyframe(
                            t0 + 750,
                            [base_offset[0], base_offset[1] - 6.0],
                            crate::timeline::Easing::EaseInOut,
                        );
                        mot.add_keyframe(
                            t0 + 1500,
                            base_offset,
                            crate::timeline::Easing::EaseInOut,
                        );
                        mot.add_keyframe(
                            t0 + 2250,
                            [base_offset[0], base_offset[1] + 6.0],
                            crate::timeline::Easing::EaseInOut,
                        );
                        mot.add_keyframe(
                            t0 + 3000,
                            base_offset,
                            crate::timeline::Easing::EaseInOut,
                        );
                    }
                },
                "breath" => {
                    let base_scale = track.geometry.scale.get(0, 1.0);
                    let peak_scale = base_scale * 1.03;
                    let sc = track.geometry.scale.ensure(base_scale);
                    for c in 0..num_cycles {
                        let t0 = c * period_ms;
                        sc.add_keyframe(t0, base_scale, crate::timeline::Easing::EaseInOut);
                        sc.add_keyframe(t0 + 1500, peak_scale, crate::timeline::Easing::EaseInOut);
                        sc.add_keyframe(t0 + 3000, base_scale, crate::timeline::Easing::EaseInOut);
                    }
                },
                _ => {},
            }
        }
    }
}

pub(super) fn seed_namespace_exports(
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    prefix: &str,
    namespace: &crate::module::Namespace,
) {
    for (name, expr) in &namespace.exports {
        let key = format!("{prefix}.{name}");
        match evaluate_expr(expr, &timeline.env) {
            Ok(value) => {
                timeline.env.set(&key, value);
            },
            Err(e) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::ModuleExportEvalError,
                        DiagnosticPhase::Build,
                        format!("Failed to evaluate export '{key}': {e}; using default."),
                    )
                    .with_subject(&key),
                );
            },
        }
    }

    for (alias, nested) in &namespace.namespaces {
        seed_namespace_exports(timeline, diagnostics, &format!("{prefix}.{alias}"), nested);
    }
}
