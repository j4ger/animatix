/// Common action execution context and target processing utilities.
pub mod base;
/// Visual effect actions (bounce, pulse, shake).
pub mod effects;
pub use base::{ActionTiming, for_each_action_target};
/// Entrance actions that bring actors into view (fade_in, wipe_in).
pub mod entrance;
/// Exit actions that remove actors from view (fade_out).
pub mod exit;
/// Highlight actions for equation fragments (highlight, unhighlight).
pub mod highlight;
/// Motion actions that transform actor placement (move, shift, rotate, scale).
pub mod motion;
/// Persistence actions (persist, remove) for multi-scene carry-forward.
pub mod persistence;
mod presets;
/// BuiltinInvocation registry types: signatures, parameters, and the [`BuiltinAction`] trait.
pub mod registry;
/// Reorder actions that change container child order (swap, reorder).
pub mod reorder;
/// Reveal actions that animate vector stroke and fill visibility
/// (draw_in, reveal_in, wipe_out, reveal_out, draw_out).
pub mod reveal;
/// Pivot-anchored scale presets (`grow_in`, `shrink_out`).
pub mod scale_presets;

use effects::{Bounce, Pulse, Shake};
use entrance::{FadeIn, WipeIn};
use exit::FadeOut;
use highlight::{Highlight, Unhighlight};
use motion::{Move, Rotate, Scale, Shift};
use persistence::{Persist, Remove};
use presets::{PopIn, SettleIn};
use registry::{BuiltinAction, CallableDescriptor};
use reorder::{Reorder, Swap};
use reveal::{DrawIn, DrawOut, RevealIn, RevealOut, WipeOut};
use scale_presets::{GrowIn, ShrinkOut};
use tracing::debug;

use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::timeline::Timeline;
use crate::timeline::property_track::{Interpolate, PropertyTrack, TrackAccessor};

mod invocation;
pub use invocation::BuiltinInvocation;

mod dispatch;
mod guards;
pub(crate) mod handoff;
mod targets;

use animatix_core::builtins::callable::TimelineBuiltin as BuiltinActionKind;
use dispatch::execute_action;
use guards::push_unknown_action_diagnostic;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
pub(crate) use guards::{
    ensure_guard_keyframe, entrance_opacity_target, lift_hidden_by_default,
    lift_hidden_by_default_subtree, reveal_authored_zero_opacity,
};
pub(crate) use targets::{
    ensure_target_exists, ensure_vector_reveal_target, expand_group_targets, resolve_action_target,
};
/// Looks up the action by verb and executes it if found.
pub fn process_action(
    action: &BuiltinInvocation,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) {
    if let Some(kind) = BuiltinActionKind::from_name(&action.verb) {
        execute_action(action, time_ms, timeline, diagnostics, &kind);
        return;
    }

    push_unknown_action_diagnostic(action, diagnostics, span);
}

/// Exposes all action signatures for LSP/UI integration.
pub fn get_action_signatures() -> Vec<CallableDescriptor> {
    BuiltinActionKind::ALL.iter().map(|kind| kind.signature()).collect()
}
