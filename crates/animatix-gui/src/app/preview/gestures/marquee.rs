//! Gesture handler for marquee (rubber-band) selection on empty canvas.
//!
//! This is the lowest-priority gesture handler: it starts a marquee when
//! the user drags on empty canvas (no actor body, handle, pivot, or keyframe
//! was hit). It updates `selection.marquee_start` / `selection.marquee_current`
//! during the drag and, on release, computes the scene-space marquee rectangle
//! to select intersecting actors.

use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};

pub(crate) struct MarqueeGesture;

impl GestureHandler for MarqueeGesture {
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext,
        preview_rect: egui::Rect,
    ) -> GestureResult {
        match gesture {
            Gesture::Start { pos, .. } => {
                // Lowest priority: if this handler is called, no actor/handle/
                // pivot/keyframe claimed the drag, so start a marquee.
                ctx.selection.marquee_start = Some(*pos);
                ctx.selection.marquee_current = Some(*pos);
                GestureResult::Claimed
            },
            Gesture::Move { pos, .. } => {
                // Only update if marquee is active
                if ctx.selection.marquee_start.is_some() {
                    ctx.selection.marquee_current = Some(*pos);
                    GestureResult::Claimed
                } else {
                    GestureResult::Ignored
                }
            },
            Gesture::End { modifiers, .. } => {
                // Only handle if marquee was active
                if ctx.selection.marquee_start.is_some() {
                    if let (Some(start), Some(current)) =
                        (ctx.selection.marquee_start, ctx.selection.marquee_current)
                    {
                        let start_scene = ctx.preview_screen_to_scene(preview_rect, start);
                        let current_scene = ctx.preview_screen_to_scene(preview_rect, current);

                        let marquee_rect = egui::Rect::from_two_pos(
                            egui::pos2(start_scene.x as f32, start_scene.y as f32),
                            egui::pos2(current_scene.x as f32, current_scene.y as f32),
                        );

                        let multi = modifiers.shift || modifiers.ctrl || modifiers.command;
                        if !multi {
                            ctx.selected_actors.clear();
                        }

                        for (label, bounds) in ctx.hit_regions {
                            let center = egui::pos2(
                                ((bounds.x0 + bounds.x1) / 2.0) as f32,
                                ((bounds.y0 + bounds.y1) / 2.0) as f32,
                            );
                            if marquee_rect.contains(center) {
                                if multi && ctx.selected_actors.contains(label) {
                                    ctx.selected_actors.remove(label);
                                } else {
                                    ctx.selected_actors.insert(label.clone());
                                }
                            }
                        }
                    }

                    ctx.selection.marquee_start = None;
                    ctx.selection.marquee_current = None;

                    // NOTE: does NOT call finish_drag() or emit DragEnded —
                    // marquee doesn't mutate document source.

                    GestureResult::Claimed
                } else {
                    GestureResult::Ignored
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::ActionQueue;
    use crate::app::preview::{
        ToolMode, context::PreviewContext, performance::PerformanceMetrics,
        selection::SelectionState,
    };
    use std::collections::HashSet;

    #[test]
    fn marquee_end_selects_hits_and_clears_transient_state_without_source_commands() {
        let dimensions = animatix::timeline::SceneDimensions {
            width: 100,
            height: 100,
        };
        let mut preview = crate::app::PreviewPaneState::new(1.0, dimensions);
        preview.viewport.preview_pan = egui::vec2(50.0, 50.0);
        let mut commands = ActionQueue::default();
        let mut selection = SelectionState::default();
        let mut selected = HashSet::from(["previous".to_owned()]);
        let regions = vec![
            ("inside".to_owned(), kurbo::Rect::new(20.0, 20.0, 30.0, 30.0)),
            ("outside".to_owned(), kurbo::Rect::new(80.0, 80.0, 90.0, 90.0)),
        ];
        let mut tool = ToolMode::Select;
        let mut performance = PerformanceMetrics::new();
        let mut ctx = PreviewContext {
            scene_dimensions: dimensions,
            preview: &mut preview,
            preview_texture_id: None,
            commands: &mut commands,
            selection: &mut selection,
            selected_actors: &mut selected,
            hit_regions: &regions,
            timeline: None,
            tool_mode: &mut tool,
            composition: None,
            active_scene: None,
            performance_metrics: &mut performance,
            debug_layout: false,
            debug_spacing: false,
            rebuild_in_progress: false,
            has_scene: true,
            source_text: Some("#0s\n"),
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        for event in [
            Gesture::Start {
                pos: egui::pos2(10.0, 10.0),
            },
            Gesture::Move {
                pos: egui::pos2(60.0, 60.0),
            },
            Gesture::End {
                modifiers: egui::Modifiers::NONE,
            },
        ] {
            assert_eq!(MarqueeGesture.handle(&event, &mut ctx, rect), GestureResult::Claimed);
        }
        assert_eq!(*ctx.selected_actors, HashSet::from(["inside".to_owned()]));
        assert!(ctx.selection.marquee_start.is_none());
        assert!(ctx.selection.marquee_current.is_none());
        assert!(ctx.commands.is_empty());
        assert_eq!(
            MarqueeGesture.handle(
                &Gesture::End {
                    modifiers: egui::Modifiers::NONE
                },
                &mut ctx,
                rect
            ),
            GestureResult::Ignored
        );
    }
}
