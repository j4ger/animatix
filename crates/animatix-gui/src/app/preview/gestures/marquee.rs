//! Gesture handler for marquee (rubber-band) selection on empty canvas.
//!
//! This is the lowest-priority gesture handler: it starts a marquee when
//! the user drags on empty canvas (no actor body, handle, pivot, or keyframe
//! was hit). It updates `selection.marquee_start` / `selection.marquee_current`
//! during the drag and, on release, computes the scene-space marquee rectangle
//! to select intersecting actors.
//!
//! Extracted from the legacy `drag_handler.rs` marquee/selection arms.

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
            Gesture::DragStart { pos, .. } => {
                // Lowest priority: if this handler is called, no actor/handle/
                // pivot/keyframe claimed the drag, so start a marquee.
                ctx.selection.marquee_start = Some(*pos);
                ctx.selection.marquee_current = Some(*pos);
                GestureResult::Claimed
            },
            Gesture::DragMove { pos, .. } => {
                // Only update if marquee is active
                if ctx.selection.marquee_start.is_some() {
                    ctx.selection.marquee_current = Some(*pos);
                    GestureResult::Claimed
                } else {
                    GestureResult::Ignored
                }
            },
            Gesture::DragEnd { modifiers, .. } => {
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
                            let is_locked = ctx
                                .timeline
                                .and_then(|t| t.get_track(label))
                                .map(|tr| tr.locked)
                                .unwrap_or(false);
                            if is_locked {
                                continue;
                            }
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
            _ => GestureResult::Ignored,
        }
    }
}
