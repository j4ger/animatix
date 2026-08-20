//! Persistence actions that control which actors are carried across scene
//! boundaries in multi-scene compositions.
//!
//! **`persist`** marks actor targets to be carried forward; **`remove`**
//! fades them out and clears their persistence flag.

use super::registry::{ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

/// Helper: apply a fade-out to the target track at the given time range.
///
/// Reuses the fade-out logic from `exit.rs::FadeOut` to avoid duplication
/// between the `fade-out` action and the `remove` action.
pub(crate) fn apply_fade_out_opacity(
    timeline: &mut Timeline,
    target: &str,
    t_start_ms: u64,
    t_end_ms: u64,
    delay_ms: f64,
    easing: Easing,
) {
    let track = match timeline.tracks.get_mut(target) {
        Some(t) => t,
        None => return,
    };

    let start_opacity = track.style.opacity.get(t_start_ms, 1.0);
    if t_end_ms > t_start_ms {
        track
            .style
            .opacity
            .ensure(1.0)
            .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
    } else if delay_ms > 0.0 && t_start_ms > 0 {
        let guard_time = t_start_ms.saturating_sub(1);
        super::ensure_guard_keyframe(&mut track.style.opacity, guard_time, 1.0);
    }
    track.style.opacity.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
}

// ---------------------------------------------------------------------------
// Persist action
// ---------------------------------------------------------------------------

/// Marks actors to be carried forward when crossing a scene boundary.
///
/// ```amx
/// [persist actor_a, actor_b]
/// ```
pub struct Persist;

impl BuiltinAction for Persist {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "persist".to_string(),
            category: "Persistence".to_string(),
            description: "Marks the target actor(s) to be carried forward across \
                          scene boundaries in multi-scene compositions. \
                          Timing modifiers are ignored."
                .to_string(),
            params: vec![],
            modifiers: vec![], // no timing modifiers accepted
        }
    }

    fn execute(
        &self,
        action: &Action,
        _time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Warn if any timing modifier is present (persist ignores duration)
        if !action.modifiers.is_empty() {
            // Check if there are any timing-related modifiers (bare duration, delay, ease)
            let parsed = parse_timing_modifiers(
                &action.modifiers,
                ModifierHost::Action,
                Some(&action.verb),
                diagnostics,
            );
            if parsed.duration_ms > 0.0 || parsed.delay_ms > 0.0 {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::PersistIgnoresDuration,
                        DiagnosticPhase::Build,
                        "Persist ignores duration; timing modifiers will be ignored.",
                    )
                    .with_subject(&action.verb)
                    .with_ast_span(None),
                );
            }
        }

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            // Warn if the actor was removed (flag set to false) earlier in this scene.
            if let Some(&false) = timeline.persistence_flags.get(target) {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::PersistAfterRemove,
                        DiagnosticPhase::Build,
                        format!(
                            "Actor '{}' was removed earlier in this scene; persisting it \
                             now will carry it at opacity 0.",
                            target
                        ),
                    )
                    .with_subject(target),
                );
            }

            timeline.persistence_flags.insert(target.clone(), true);
        }
    }
}

// ---------------------------------------------------------------------------
// Remove action
// ---------------------------------------------------------------------------

/// Fades out the target and clears its persistence flag.
///
/// ```amx
/// [remove actor_a]         # instant remove (default)
/// [remove actor_a, 500ms]  # fade out over 500ms
/// ```
pub struct Remove;

impl BuiltinAction for Remove {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "remove".to_string(),
            category: "Persistence".to_string(),
            description: "Fades out the target actor and clears its persistence flag, \
                          preventing it from being carried to the next scene. \
                          Optional duration controls the fade-out speed."
                .to_string(),
            params: vec![],
            modifiers: base_timing_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let parsed = parse_timing_modifiers(
            &action.modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            // Apply fade-out opacity keyframes (reuse shared helper)
            apply_fade_out_opacity(timeline, target, t_start_ms, t_end_ms, delay_ms, easing);

            // Clear the persistence flag so this actor is NOT carried forward
            timeline.persistence_flags.insert(target.clone(), false);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, Expr, Modifier, Property, Stmt, Time};
    use crate::diagnostics::DiagnosticSeverity;

    fn text_decl(label: &str) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.to_string(),
            array_index: None,
            ty: "Text".to_string(),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("Hello".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(32.0),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(180.0)]),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    }

    // -----------------------------------------------------------------------
    // Persist tests
    // -----------------------------------------------------------------------

    #[test]
    fn persist_sets_flag() {
        let mut timeline = Timeline::new();
        let mut track = crate::timeline::AnimationTrack::new("actor".to_string());
        track.style.opacity.ensure(1.0);
        timeline.tracks.insert("actor".to_string(), track);

        let action = Action {
            verb: "persist".to_string(),
            targets: vec!["actor".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
        };

        let mut diagnostics = Vec::new();
        Persist.execute(&action, 0.0, &mut timeline, &mut diagnostics);

        assert!(diagnostics.is_empty());
        assert_eq!(
            timeline.persistence_flags.get("actor"),
            Some(&true),
            "persist should set flag to true"
        );
    }

    #[test]
    fn persist_supports_multiple_targets() {
        let mut timeline = Timeline::new();
        for label in ["a", "b", "c"] {
            let mut track = crate::timeline::AnimationTrack::new(label.to_string());
            track.style.opacity.ensure(1.0);
            timeline.tracks.insert(label.to_string(), track);
        }

        let action = Action {
            verb: "persist".to_string(),
            targets: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
        };

        let mut diagnostics = Vec::new();
        Persist.execute(&action, 0.0, &mut timeline, &mut diagnostics);

        assert!(diagnostics.is_empty());
        assert_eq!(timeline.persistence_flags.get("a"), Some(&true));
        assert_eq!(timeline.persistence_flags.get("b"), Some(&true));
        assert_eq!(timeline.persistence_flags.get("c"), Some(&true));
    }

    #[test]
    fn persist_emits_warning_for_duration_modifier() {
        let mut timeline = Timeline::new();
        let mut track = crate::timeline::AnimationTrack::new("actor".to_string());
        track.style.opacity.ensure(1.0);
        timeline.tracks.insert("actor".to_string(), track);

        let action = Action {
            verb: "persist".to_string(),
            targets: vec!["actor".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("500ms".to_string()),
            }],
            byte_span: None,
            target_index: vec![],
        };

        let mut diagnostics = Vec::new();
        Persist.execute(&action, 0.0, &mut timeline, &mut diagnostics);

        let has_warning = diagnostics.iter().any(|d| {
            d.code == DiagnosticCode::PersistIgnoresDuration
                && d.severity == DiagnosticSeverity::Warning
        });
        assert!(has_warning, "persist with duration should emit warning");

        // Flag should still be set despite warning
        assert_eq!(timeline.persistence_flags.get("actor"), Some(&true));
    }

    #[test]
    fn persist_emits_error_for_nonexistent_target() {
        let mut timeline = Timeline::new();

        let action = Action {
            verb: "persist".to_string(),
            targets: vec!["nonexistent".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
        };

        let mut diagnostics = Vec::new();
        Persist.execute(&action, 0.0, &mut timeline, &mut diagnostics);

        let has_error =
            diagnostics.iter().any(|d| d.code == DiagnosticCode::UnsupportedActionTarget);
        assert!(has_error, "persist on nonexistent target should emit error");
        assert!(!timeline.persistence_flags.contains_key("nonexistent"));
    }

    // -----------------------------------------------------------------------
    // Remove tests
    // -----------------------------------------------------------------------

    #[test]
    fn remove_clears_persistence_flag() {
        let mut timeline = Timeline::new();
        let mut track = crate::timeline::AnimationTrack::new("actor".to_string());
        track.style.opacity.ensure(1.0);
        timeline.tracks.insert("actor".to_string(), track);
        timeline.persistence_flags.insert("actor".to_string(), true);

        let action = Action {
            verb: "remove".to_string(),
            targets: vec!["actor".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
        };

        let mut diagnostics = Vec::new();
        Remove.execute(&action, 0.0, &mut timeline, &mut diagnostics);

        assert!(diagnostics.is_empty());
        assert_eq!(
            timeline.persistence_flags.get("actor"),
            Some(&false),
            "remove should clear persistence flag"
        );
    }

    #[test]
    fn remove_instant_sets_opacity_to_zero() {
        let mut timeline = Timeline::new();
        let mut track = crate::timeline::AnimationTrack::new("actor".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        timeline.tracks.insert("actor".to_string(), track);

        let action = Action {
            verb: "remove".to_string(),
            targets: vec!["actor".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
        };

        let mut diagnostics = Vec::new();
        Remove.execute(&action, 0.0, &mut timeline, &mut diagnostics);

        assert!(diagnostics.is_empty());
        let track = timeline.tracks.get("actor").unwrap();
        assert_eq!(track.style.opacity.get(0, 1.0), 0.0);
    }

    #[test]
    fn remove_animated_sets_fade_out_keyframes() {
        let mut timeline = Timeline::new();
        let mut track = crate::timeline::AnimationTrack::new("actor".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        timeline.tracks.insert("actor".to_string(), track);

        let action = Action {
            verb: "remove".to_string(),
            targets: vec!["actor".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("1s".to_string()),
            }],
            byte_span: None,
            target_index: vec![],
        };

        let mut diagnostics = Vec::new();
        Remove.execute(&action, 0.0, &mut timeline, &mut diagnostics);

        assert!(diagnostics.is_empty());
        let track = timeline.tracks.get("actor").unwrap();
        assert_eq!(track.style.opacity.get(0, 1.0), 1.0);
        assert!(track.style.opacity.get(500, 1.0) > 0.0);
        assert!(track.style.opacity.get(500, 1.0) < 1.0);
        assert_eq!(track.style.opacity.get(1000, 1.0), 0.0);
    }

    #[test]
    fn remove_emits_error_for_nonexistent_target() {
        let mut timeline = Timeline::new();

        let action = Action {
            verb: "remove".to_string(),
            targets: vec!["nonexistent".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
        };

        let mut diagnostics = Vec::new();
        Remove.execute(&action, 0.0, &mut timeline, &mut diagnostics);

        assert!(
            diagnostics.iter().any(|d| d.code == DiagnosticCode::UnsupportedActionTarget),
            "remove on nonexistent target should emit error"
        );
    }

    // -----------------------------------------------------------------------
    // Integration tests (via build)
    // -----------------------------------------------------------------------

    #[test]
    fn persist_via_build_sets_flag() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                text_decl("headline"),
                Stmt::Action(
                    Action {
                        verb: "persist".to_string(),
                        targets: vec!["headline".to_string()],
                        args: vec![],
                        modifiers: vec![],
                        byte_span: None,
                        target_index: vec![],
                    },
                    None,
                ),
            ],
            span: None,
        }];

        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        assert_eq!(report.output.persistence_flags.get("headline"), Some(&true));
    }

    #[test]
    fn remove_via_build_clears_flag_and_sets_opacity() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                text_decl("headline"),
                Stmt::Action(
                    Action {
                        verb: "remove".to_string(),
                        targets: vec!["headline".to_string()],
                        args: vec![],
                        modifiers: vec![Modifier {
                            name: None,
                            value: Expr::Ident("1s".to_string()),
                        }],
                        byte_span: None,
                        target_index: vec![],
                    },
                    None,
                ),
            ],
            span: None,
        }];

        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        assert_eq!(
            report.output.persistence_flags.get("headline"),
            Some(&false),
            "remove should clear persistence flag"
        );
        let track = report.output.tracks.get("headline").unwrap();
        assert_eq!(
            track.style.opacity.get(1000, 1.0),
            0.0,
            "opacity should reach zero at end of removal"
        );
    }
}
