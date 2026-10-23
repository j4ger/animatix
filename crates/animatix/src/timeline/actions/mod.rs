/// Visual effect actions (bounce, pulse, shake).
pub mod effects;
/// Entrance actions that bring actors into view (fade-in, wipe-in).
pub mod entrance;
/// Exit actions that remove actors from view (fade-out).
pub mod exit;
/// Highlight actions for equation fragments (highlight, unhighlight).
pub mod highlight;
/// Motion actions that transform actor placement (move, shift, rotate, scale).
pub mod motion;
/// Persistence actions (persist, remove) for multi-scene carry-forward.
pub mod persistence;
mod presets;
/// Action registry types: signatures, parameters, and the [`BuiltinAction`] trait.
pub mod registry;
/// Reorder actions that change container child order (swap, reorder).
pub mod reorder;
/// Reveal actions that animate vector stroke and fill visibility
/// (draw-in, reveal-in, wipe-out, reveal-out, draw-out).
pub mod reveal;

use effects::{Bounce, Pulse, Shake};
use entrance::{FadeIn, WipeIn};
use exit::FadeOut;
use highlight::{Highlight, Unhighlight};
use motion::{Move, Rotate, Scale, Shift};
use persistence::{Persist, Remove};
use presets::{PopIn, SettleIn};
use registry::{ActionSignature, BuiltinAction};
use reorder::{Reorder, Swap};
use reveal::{DrawIn, DrawOut, RevealIn, RevealOut, WipeOut};
use tracing::debug;

use crate::ast::Action;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::extension_context::ExtensionContext;
use crate::timeline::Timeline;
use crate::timeline::property_track::{Interpolate, PropertyTrack, TrackAccessor};

mod dispatch;
mod guards;
mod targets;

use dispatch::{execute_action, get_builtin_actions};
use guards::push_unknown_action_diagnostic;
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
    action: &Action,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) {
    process_action_with_extensions(action, time_ms, timeline, diagnostics, span, None);
}

/// Like [`process_action`], but honors custom actions from an extension context.
pub fn process_action_with_extensions(
    action: &Action,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
    extensions: Option<&ExtensionContext>,
) {
    if let Some(handler) = extensions.and_then(|ctx| ctx.action(&action.verb)) {
        execute_action(action, time_ms, timeline, diagnostics, handler);
        return;
    }

    for builtin in get_builtin_actions() {
        if builtin.signature().name == action.verb {
            execute_action(action, time_ms, timeline, diagnostics, builtin.as_ref());
            return;
        }
    }

    push_unknown_action_diagnostic(action, diagnostics, span);
}

/// Exposes all action signatures for LSP/UI integration.
pub fn get_action_signatures() -> Vec<ActionSignature> {
    get_builtin_actions().iter().map(|a| a.signature()).collect()
}

/// Exposes built-in and extension action signatures.
pub fn get_action_signatures_with_extensions(
    extensions: Option<&ExtensionContext>,
) -> Vec<ActionSignature> {
    let mut signatures = get_action_signatures();
    if let Some(ctx) = extensions {
        for signature in ctx.action_signatures() {
            if !signatures.iter().any(|existing| existing.name == signature.name) {
                signatures.push(signature);
            }
        }
    }
    signatures
}
