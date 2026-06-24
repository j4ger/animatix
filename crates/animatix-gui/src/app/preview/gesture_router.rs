use super::DragState;
use super::context::PreviewContext;
use super::gesture::{Gesture, GestureHandler, GestureResult, PointerButton};
use super::gestures::common::GestureFrame;
use egui::{Pos2, Rect, Response, Ui};

pub struct GestureRouter;

impl GestureRouter {
    #[allow(clippy::needless_return)]
    pub fn handle_preview_gestures(
        ctx: &mut PreviewContext<'_>,
        ui: &mut Ui,
        preview_rect: Rect,
        response: &Response,
    ) {
        // Capture drag_state and marquee state before any mutable borrows
        let is_active_pivot = matches!(ctx.drag_state, DragState::MovePivot { .. });
        let is_active_rotate = matches!(ctx.drag_state, DragState::Rotate { .. });
        let is_active_scale = matches!(ctx.drag_state, DragState::Scale { .. });
        let is_active_motion_path = matches!(ctx.drag_state, DragState::MotionPath { .. });
        let is_active_move = matches!(ctx.drag_state, DragState::Move { .. });
        let is_active_reorder = matches!(ctx.drag_state, DragState::Reorder { .. });
        let is_active_marquee = ctx.selection.marquee_start.is_some();
        let is_active_vertex = matches!(ctx.drag_state, DragState::EditVertices { .. });
        let is_active_callout = matches!(ctx.drag_state, DragState::CalloutLabel { .. } | DragState::CalloutTip { .. });
        let is_drag_started = response.drag_started();

        // Build per-frame gesture frame
        let frame = GestureFrame {
            screen_pos: ui.ctx().input(|i| i.pointer.latest_pos()),
            modifiers: ui.ctx().input(|i| i.modifiers),
            drag_stopped: response.drag_stopped(),
            any_down: ui.ctx().input(|i| i.pointer.any_down()),
            any_released: ui.ctx().input(|i| i.pointer.any_released()),
        };

        // ── Active extracted drags: route to handler ──
        let build_drag_end = || -> Gesture {
            Gesture::DragEnd {
                pos: frame.screen_pos.unwrap_or(Pos2::ZERO),
                button: PointerButton::Primary,
                modifiers: frame.modifiers,
            }
        };
        let build_drag_move = |pos| -> Gesture {
            Gesture::DragMove {
                pos,
                delta: egui::Vec2::ZERO,
                button: PointerButton::Primary,
                modifiers: frame.modifiers,
            }
        };
        let mut route_active = |handler: &mut dyn GestureHandler| {
            if frame.drag_stopped || frame.any_released || !frame.any_down {
                handler.handle(&build_drag_end(), ctx, preview_rect);
            } else if let Some(pos) = frame.screen_pos {
                handler.handle(&build_drag_move(pos), ctx, preview_rect);
            }
        };

        if is_active_pivot { route_active(&mut super::gestures::pivot::PivotGesture); return; }
        if is_active_rotate { route_active(&mut super::gestures::rotate::RotateGesture); return; }
        if is_active_scale { route_active(&mut super::gestures::scale::ScaleGesture); return; }
        if is_active_motion_path { route_active(&mut super::gestures::motion_path::MotionPathGesture); return; }
        if is_active_marquee { route_active(&mut super::gestures::marquee::MarqueeGesture); return; }
        if is_active_vertex { route_active(&mut super::gestures::vertex::VertexGesture); return; }
        if is_active_callout { route_active(&mut super::gestures::callout::CalloutGesture); return; }
        if is_active_reorder { route_active(&mut super::gestures::reorder::ReorderGesture); return; }
        if is_active_move { route_active(&mut super::gestures::move_actor::MoveActorGesture); return; }

        // ── Drag start: try extracted start handlers in priority order ──
        if is_drag_started {
            if let Some(pos) = frame.screen_pos {
                let start_gesture = Gesture::DragStart {
                    pos,
                    button: PointerButton::Primary,
                    modifiers: frame.modifiers,
                };
                let mut pivot_handler = super::gestures::pivot::PivotGesture;
                if pivot_handler.handle(&start_gesture, ctx, preview_rect) == GestureResult::Claimed { return; }
                let mut rotate_handler = super::gestures::rotate::RotateGesture;
                if rotate_handler.handle(&start_gesture, ctx, preview_rect) == GestureResult::Claimed { return; }
                let mut scale_handler = super::gestures::scale::ScaleGesture;
                if scale_handler.handle(&start_gesture, ctx, preview_rect) == GestureResult::Claimed { return; }
                let mut motion_handler = super::gestures::motion_path::MotionPathGesture;
                if motion_handler.handle(&start_gesture, ctx, preview_rect) == GestureResult::Claimed { return; }
                let mut vertex_handler = super::gestures::vertex::VertexGesture;
                if vertex_handler.handle(&start_gesture, ctx, preview_rect) == GestureResult::Claimed { return; }
                let mut callout_handler = super::gestures::callout::CalloutGesture;
                if callout_handler.handle(&start_gesture, ctx, preview_rect) == GestureResult::Claimed { return; }
                let mut move_handler = super::gestures::move_actor::MoveActorGesture;
                if move_handler.handle(&start_gesture, ctx, preview_rect) == GestureResult::Claimed { return; }
                let mut reorder_handler = super::gestures::reorder::ReorderGesture;
                if reorder_handler.handle(&start_gesture, ctx, preview_rect) == GestureResult::Claimed { return; }
                if super::gestures::marquee::MarqueeGesture.handle(&start_gesture, ctx, preview_rect) == GestureResult::Claimed { return; }
            }
        }
    }
}
