use super::registry::{BuiltinAction, CallableDescriptor};
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::Timeline;
use crate::timeline::actions::BuiltinInvocation;
use crate::timeline::property_track::TrackAccessor;

/// Fades out the target by animating its overall opacity to 0.
pub struct FadeOut;

impl BuiltinAction for FadeOut {
    fn signature(&self) -> CallableDescriptor {
        animatix_core::builtins::callable::TimelineBuiltin::FadeOut.descriptor().clone()
    }

    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        super::for_each_action_target(
            action,
            time_ms,
            timeline,
            diagnostics,
            |timeline, target, id, timing, diagnostics| {
                if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                    return;
                }
                let style = timeline.world.style_mut(id);
                let start_opacity = style.opacity.get(timing.t_start_ms, 1.0);
                if timing.duration_ms > 0.0 {
                    style.opacity.ensure(1.0).add_keyframe(
                        timing.t_start_ms,
                        start_opacity,
                        Easing::Linear,
                    );
                } else if timing.delay_ms > 0.0 && timing.t_start_ms > 0 {
                    let guard_time = timing.t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut style.opacity, guard_time, 1.0);
                }
                style.opacity.ensure(1.0).add_keyframe(timing.t_end_ms, 0.0, timing.easing);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::text_decl_with;
    use super::*;
    use crate::ast::{Expr, Modifier, Stmt, Time};

    #[test]
    fn fade_out_animates_opacity_to_zero() {
        let ast = vec![Stmt::Keyframe {
            cue_group: None,
            time: Time::Seconds(0.0),
            body: vec![
                text_decl_with("headline", "Bye"),
                crate::timeline::actions::test_support::invocation_stmt(
                    BuiltinInvocation {
                        verb: "fade_out".to_string(),
                        targets: vec!["headline".to_string()],
                        args: vec![],
                        modifiers: vec![Modifier {
                            name: None,
                            value: Expr::Ident("1s".to_string()),
                        }],
                        byte_span: None,
                        target_index: vec![],
                        target_index_position: vec![],
                    },
                    None,
                ),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let headline_id = report.output.world.get_id("headline").expect("headline id");
        let style = report.output.world.style(headline_id);

        assert_eq!(style.opacity.get(0, 1.0), 1.0);
        assert!(style.opacity.get(500, 1.0) < 1.0);
        assert!(style.opacity.get(500, 1.0) > 0.0);
        assert_eq!(style.opacity.get(1000, 1.0), 0.0);
        assert!(report.diagnostics.is_empty());
    }
}
