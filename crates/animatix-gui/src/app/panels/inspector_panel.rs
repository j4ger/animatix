//! Inspector panel: actor property editor, pivot controls, and keyframe view.

use std::collections::{HashMap, HashSet};

use animatix::timeline::Timeline;

use crate::app::commands::CommandQueue;
use crate::app::panels::panel_frame;
use crate::app::panels::inspector;
use crate::app::panels::inspector::{PropertyViewMode, KeyframeViewMode};
use crate::app::PreviewPaneState;
use animatix::timeline::SceneDimensions;

pub(crate) struct InspectorContext<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub selected_actors: &'a mut HashSet<String>,
    pub commands: &'a mut CommandQueue,
    pub keyframe_mode: bool,
    pub scene_dimensions: SceneDimensions,
    pub pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    pub property_view_mode: &'a mut PropertyViewMode,
    pub keyframe_view_mode: &'a mut KeyframeViewMode,
}

pub(crate) fn inspector_ui(ctx: &mut InspectorContext<'_>, ui: &mut egui::Ui) {
    panel_frame().show(ui, |ui| {
        let current_time_s = ctx.preview.playback.current_time_s;
        // For compositions, use the active scene's timeline.
        // During a transition, if the selected actor is in the other scene,
        // use that scene's timeline so the inspector shows correct properties.
        let timeline = ctx.timeline.or_else(|| {
            let comp = ctx.composition?;
            let active_scene = ctx.active_scene?;
            let active_has_actor = ctx.selected_actors.iter().next().is_some_and(|sel| {
                comp.scenes.get(active_scene).is_some_and(|s| s.timeline.has_actor(sel))
            });
            if !active_has_actor {
                let (_, _, transition) = comp.evaluate(current_time_s);
                if transition.is_some() {
                    for (name, scene) in &comp.scenes {
                        if name != active_scene
                            && ctx.selected_actors.iter().any(|sel| scene.timeline.has_actor(sel))
                        {
                            return Some(&scene.timeline);
                        }
                    }
                }
            }
            comp.scenes.get(active_scene).map(|s| &s.timeline)
        });
        inspector::inspector_ui(
            ui,
            timeline,
            ctx.selected_actors,
            current_time_s,
            ctx.commands,
            ctx.keyframe_mode,
            ctx.scene_dimensions,
            ctx.pivot_offsets,
            ctx.property_view_mode,
            ctx.keyframe_view_mode,
        );
    });
}