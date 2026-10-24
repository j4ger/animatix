//! Action guards and target expansion.

use super::*;

pub(super) fn push_unknown_action_diagnostic(
    action: &Action,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) {
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::UnknownAction,
            DiagnosticPhase::Build,
            format!("Unknown action '{}'; no built-in action matches this name.", action.verb),
        )
        .with_subject(&action.verb)
        .with_ast_span(span),
    );
}

pub(super) fn push_unsupported_action_target_diagnostic(
    verb: &str,
    target: &str,
    reason: &str,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) {
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::UnsupportedActionTarget,
            DiagnosticPhase::Build,
            format!("Action '{verb}' does not support target '{target}': {reason}."),
        )
        .with_subject(format!("{verb} {target}"))
        .with_ast_span(span),
    );
}

/// Insert a guard keyframe at `guard_time` for a single property track if one doesn't already
/// exist. This preserves the pre-delay value for instant-change actions (duration == 0) with delay.
pub(crate) fn ensure_guard_keyframe<T: Interpolate>(
    track: &mut Option<PropertyTrack<T>>,
    guard_time: u64,
    default: T,
) {
    if !track.has_keyframe_at(guard_time) {
        let prior = track.get(guard_time, default.clone());
        track.ensure(default).add_keyframe(guard_time, prior, Easing::Linear);
    }
}

/// Lift the build-time "hidden by default" opacity seed across an entrance
/// action's window.
///
/// Actors first declared before the first keyframe are seeded `opacity = 0`
/// (docs/spec.md "Pre-Keyframe Actor Declarations"). `fade-in` animates
/// opacity itself, but the other entrances (`draw-in`, `wipe-in`,
/// `reveal-in`) only animate `stroke_progress` / `fill_opacity` /
/// `char_progress` — without this lift their target would stay fully
/// transparent forever, contradicting the documented contract that any
/// entrance reveals the actor. Consumes the track's `hidden_by_default` flag
/// so the lift happens at most once and never stomps an explicit opacity.
pub(crate) fn lift_hidden_by_default(
    track: &mut crate::timeline::AnimationTrack,
    meta: &mut crate::timeline::world::EntityMeta,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) {
    if !meta.hidden_by_default {
        return;
    }
    meta.hidden_by_default = false;
    let start_opacity = track.style.opacity.get(t_start_ms, 1.0);
    track
        .style
        .opacity
        .ensure(1.0)
        .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
    track.style.opacity.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
}

/// The opacity an entrance action settles on.
///
/// An authored `opacity: 0` is a "start hidden" seed — an entrance action exists
/// to reveal it — while any other authored opacity is the value the target rests
/// at and must not be overridden. Reach this only for targets that are *not*
/// `hidden_by_default`: that path seeds its own 0 and lifts to 1.0.
///
/// Before this rule `fade-in` ended every target at a hardcoded 1.0, so an
/// authored `opacity: 0.25` was discarded with no diagnostic, while `wipe-in`,
/// `draw-in` and `reveal-in` never touched `opacity` at all — the same authoring
/// meant opposite things depending on which entrance action was picked.
pub(crate) fn entrance_opacity_target(
    track: &crate::timeline::AnimationTrack,
    t_start_ms: u64,
) -> f32 {
    let authored = track.style.opacity.get(t_start_ms, 1.0);
    if authored == 0.0 { 1.0 } else { authored }
}

/// Lift an explicitly authored `opacity: 0` for an action that reveals by other
/// means (stroke progress, fill opacity, char progress).
///
/// Such an action never writes `opacity`, so the authored 0 keeps the target
/// invisible for the whole timeline — the mirror image of `fade-in` forcing 1.0.
/// Call only after confirming the target was *not* `hidden_by_default`; that
/// path's lift already wrote this pair.
pub(crate) fn reveal_authored_zero_opacity(
    track: &mut crate::timeline::AnimationTrack,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) {
    if track.style.opacity.get(t_start_ms, 1.0) != 0.0 {
        return;
    }
    track.style.opacity.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);
    track.style.opacity.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
}

/// Lift the hidden-by-default seed for a target and its whole subtree.
///
/// A container target (e.g. a `Graph` hosting `PlotCurve` children) must
/// reveal its subtree: pre-keyframe children carry their own seeded opacity 0
/// and opacity multiplies down the scene graph, so a container-level entrance
/// without the cascade fades the container in while its children stay
/// invisible — silently (dogfood probe 010; 07_plots.amx shipped with an
/// invisible headline curve). Children that are not hidden-by-default are
/// untouched, so a container-level entrance never un-hides a child the author
/// hid by hand with `opacity: 0`.
///
/// That is deliberate and does not contradict [`entrance_opacity_target`]: an
/// entrance action aimed *directly* at an `opacity: 0` target treats it as a
/// seed and reveals it, while fading a *container* leaves an explicitly hidden
/// child alone. Same authoring, different intent.
pub(crate) fn lift_hidden_by_default_subtree(
    timeline: &mut Timeline,
    label: &str,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) {
    let children: Vec<String> = timeline.child_labels(label).map(str::to_string).collect();
    if let Some(track) = timeline.world.tracks.get_mut(label) {
        let meta = timeline
            .world
            .meta
            .get_or_insert_with(track.id, crate::timeline::world::EntityMeta::default);
        lift_hidden_by_default(track, meta, t_start_ms, t_end_ms, easing);
    }
    for child in children {
        lift_hidden_by_default_subtree(timeline, &child, t_start_ms, t_end_ms, easing);
    }
}
