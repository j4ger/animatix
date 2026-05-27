//! Timeline panel: ruler, playback strip, actor tracks, keyframes, and range slider.

use crate::app::commands::CommandQueue;
use crate::app::panels::panel_frame;
use crate::app::panels::timeline_panel;
use crate::app::PreviewPaneState;
use animatix::composition::Composition;
use animatix::timeline::Timeline;

pub(crate) struct TimelineViewer<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a Composition>,
    pub active_scene: Option<&'a str>,
    pub commands: &'a mut CommandQueue,
}

pub(crate) fn timeline_ui(ctx: &mut TimelineViewer<'_>, ui: &mut egui::Ui) {
    panel_frame().show(ui, |ui| {
        // For compositions, use the active scene's timeline
        let timeline = ctx.timeline.or_else(|| {
            let comp = ctx.composition?;
            let scene_name = ctx.active_scene?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        });
        timeline_panel::timeline_panel_ui(
            ui,
            ctx.preview,
            timeline,
            ctx.composition,
            ctx.active_scene,
            ctx.commands,
        );
    });
}