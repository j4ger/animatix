//! Timeline panel: ruler, playback strip, actor tracks, keyframes, and range slider.

use std::collections::HashSet;

use crate::app::commands::CommandQueue;
use crate::app::panels::timeline_panel;
use crate::app::PreviewPaneState;
use animatix::composition::Composition;
use animatix::timeline::Timeline;

pub(crate) struct TimelineContext<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a Composition>,
    pub active_scene: Option<&'a str>,
    pub commands: &'a mut CommandQueue,
    pub collapsed_actors: &'a mut HashSet<String>,
    pub selected_actors: &'a mut HashSet<String>,
    /// Cached actor labels (recomputed in behavior.rs when stale).
    pub actor_labels: &'a [String],
    /// Cached per-actor keyframe property lists.
    pub actor_keyframes: &'a [(String, Vec<(u64, &'static str)>)],
}

pub(crate) fn timeline_ui(ctx: &mut TimelineContext<'_>, ui: &mut egui::Ui) {
    // No outer panel frame — the timeline manages its own padding so tracks
    // sit flush against the tile edge.
    // Note: the timeline is already resolved by behavior.rs before constructing
    // TimelineContext, so we use it directly here.
    timeline_panel::timeline_panel_ui(
        ui,
        ctx.preview,
        ctx.timeline,
        ctx.composition,
        ctx.active_scene,
        ctx.commands,
        ctx.collapsed_actors,
        ctx.selected_actors,
        ctx.actor_labels,
        ctx.actor_keyframes,
    );
}