//! Generic anchored Popover widget for the Animatix UI library.
//!
//! A `Popover` is a floating panel anchored to a trigger element (a button,
//! icon, etc.).  It manages its own open/closed state in `egui::Memory`,
//! participates in the [`crate::widget::overlay`] coordination layer so that
//! Escape and click-outside are consumed by exactly one overlay (the
//! topmost), and restores keyboard focus to the previously-focused widget
//! when it closes — mirroring the [`crate::widget::dialog`] focus-restore
//! pattern (D3).
//!
//! # Design decisions
//!
//! * **State lives in `egui::Memory`**, keyed by the popover's `Id`.  No retained widget instance
//!   holds open/closed state.
//! * **Overlay coordination**: on open the popover registers itself with [`push_overlay`] at
//!   [`OverlayLayer::Popover`] priority; on close it calls [`remove_overlay`].
//! * **Dismissal** uses [`escape_pressed`] and [`clicked_outside`] — only the topmost overlay
//!   consumes the event.
//! * **Focus restore**: on open the previously-focused `egui::Id` is saved to Memory; on full close
//!   `ctx.memory.request_focus(saved_id)` is called.
//! * **Theming**: every colour comes from `tokens::semantic` roles (`surface::floating_card_bg`,
//!   `border::DEFAULT`, `RADIUS_M`, `overlay::shadow_direct`).
//!
//! # Usage
//!
//! ```ignore
//! // Self-managing (toggles on trigger click):
//! Popover::new("my_popover")
//!     .below()
//!     .show(ui, trigger_response, |ui| {
//!         ui.label("Popover content here");
//!     });
//!
//! // Externally controlled:
//! let popover = Popover::new("filter_popover").below();
//! popover.open(&ctx, true);
//! // ... later ...
//! popover.open(&ctx, false);
//! ```
//!
//! The builder pattern matches the framework convention (AGENTS.md §2).

use egui::{Context, CornerRadius, Id, Order, Pos2, Rect, Response, Ui};

use crate::spatial;
use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH};
use crate::widget::overlay::{
    OverlayLayer, clicked_outside, escape_pressed, is_topmost, push_overlay, remove_overlay,
};

// ─── Memory keys ─────────────────────────────────────────────────────────────

/// Per-popover open-state key suffix.
const OPEN_KEY: &str = "__popover_open";
/// Per-popover saved-focus key suffix.
const PREV_FOCUS_KEY: &str = "__popover_prev_focus";
/// Per-popover "opened-this-instance" key (avoids re-saving focus on
/// subsequent frames while the popover remains open).
const OPENED_KEY: &str = "__popover_opened";
/// Per-popover "closing-this-instance" key.
const CLOSING_KEY: &str = "__popover_closing";

// ─── Public response ─────────────────────────────────────────────────────────

/// Response returned by [`Popover::show`].
pub struct PopoverResponse {
    /// Whether the popover is currently open after this frame.
    pub is_open: bool,
    /// The rect of the popover content area in screen space, or
    /// [`Rect::NOTHING`] when the popover is closed.
    pub popover_rect: Rect,
}

// ─── Direction ───────────────────────────────────────────────────────────────

/// Which side of the trigger the popover opens on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverDirection {
    /// Below the trigger (default).
    #[default]
    Below,
    /// Above the trigger.
    Above,
    /// To the right of the trigger.
    Right,
    /// To the left of the trigger.
    Left,
}

// ─── Popover ─────────────────────────────────────────────────────────────────

/// An anchored, immediate-mode floating popover panel.
///
/// Open/closed state is managed in `egui::Memory` when [`show`](Self::show)
/// is called and the trigger is clicked.  Alternatively, call
/// [`open`](Self::open) to control the state externally.
///
/// The popover participates in the overlay coordination layer
/// ([`OverlayLayer::Popover`]) so that multiple simultaneous floating panels
/// cooperate on Escape and click-outside dismissal.
///
/// Focus is automatically restored to the widget that had focus before the
/// popover opened (mirrors the dialog D3 pattern).
///
/// The builder pattern follows the eparts framework convention (AGENTS.md §2).
#[derive(Clone, Debug)]
pub struct Popover {
    id: Id,
    direction: PopoverDirection,
    gap: f32,
    max_width: Option<f32>,
    arrow: bool,
}

impl Popover {
    /// Create a new Popover with the given `id` seed.
    ///
    /// The `id` must be unique among all open popovers (and other overlays)
    /// within the same viewport.
    pub fn new(id: impl Into<Id>) -> Self {
        Self {
            id: id.into(),
            direction: PopoverDirection::default(),
            gap: 8.0,
            max_width: None,
            arrow: false,
        }
    }

    /// Anchor the popover below the trigger.
    pub fn below(mut self) -> Self {
        self.direction = PopoverDirection::Below;
        self
    }

    /// Anchor the popover above the trigger.
    pub fn above(mut self) -> Self {
        self.direction = PopoverDirection::Above;
        self
    }

    /// Anchor the popover to the right of the trigger.
    pub fn right(mut self) -> Self {
        self.direction = PopoverDirection::Right;
        self
    }

    /// Anchor the popover to the left of the trigger.
    pub fn left(mut self) -> Self {
        self.direction = PopoverDirection::Left;
        self
    }

    /// Set the gap (in points) between the trigger edge and the popover edge.
    /// Default: `8.0`.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Constrain the popover's maximum width.  When `None` the popover sizes
    /// to its content.  Default: `None`.
    pub fn max_width(mut self, max_width: f32) -> Self {
        self.max_width = Some(max_width);
        self
    }

    /// Draw a small triangular arrow pointing at the trigger.
    /// Default: `false`.
    pub fn with_arrow(mut self, arrow: bool) -> Self {
        self.arrow = arrow;
        self
    }

    /// Open/close the popover programmatically (bypasses the trigger-click
    /// toggle).  Set to `true` to open, `false` to close.
    ///
    /// When the caller uses this method, the trigger's click no longer
    /// toggles the popover — the caller owns the state.
    pub fn open(&self, ctx: &Context, open: bool) {
        ctx.data_mut(|data| {
            data.insert_temp(open_key(self.id), open);
        });
    }

    /// Return `true` if the popover is currently open, `false` otherwise.
    pub fn is_open(ctx: &Context, id: impl Into<Id>) -> bool {
        let id = id.into();
        ctx.data(|data| data.get_temp::<bool>(open_key(id)).unwrap_or(false))
    }

    /// Force-close the popover.
    pub fn close(&self, ctx: &Context) {
        Self::close_by_id(ctx, self.id);
    }

    /// Close the popover by its `Id`.
    pub fn close_by_id(ctx: &Context, id: Id) {
        ctx.data_mut(|data| {
            data.insert_temp(open_key(id), false);
            data.insert_temp(closing_key(id), true);
        });
    }

    /// Render the popover.
    ///
    /// When `trigger_response.clicked()` is `true` the popover toggles
    /// open/closed (unless `.open()` was called externally, which takes
    /// over state management).
    ///
    /// `content` receives the inner `&mut Ui` — the popover is fully themed
    /// and ready for arbitrary content.
    ///
    /// Returns [`PopoverResponse`] with `is_open` and `popover_rect`.
    pub fn show(
        &self,
        ui: &mut Ui,
        trigger_response: &Response,
        content: impl FnOnce(&mut Ui),
    ) -> PopoverResponse {
        let ctx = ui.ctx();
        let screen_rect = ctx.viewport_rect();
        let is_open_key = open_key(self.id);
        let prev_focus_key = prev_focus_key(self.id);
        let opened_key = opened_key(self.id);
        let closing_key = closing_key(self.id);

        // ── Read current state ────────────────────────────────────────────
        let mut is_open: bool = ctx.data(|d| d.get_temp::<bool>(is_open_key).unwrap_or(false));
        let mut is_closing: bool = ctx.data(|d| d.get_temp::<bool>(closing_key).unwrap_or(false));

        // ── Detect external control: if the caller has written the open
        //    state at least once, they own the toggle logic.
        let externally_controlled: bool = ctx.data(|d| d.get_temp::<bool>(is_open_key).is_some());

        if externally_controlled {
            if !is_open && is_closing {
                finish_close(ctx, self.id, prev_focus_key, opened_key, closing_key);
            }
        } else if trigger_response.clicked() {
            // Auto-toggle: caller hasn't used `.open()`, so trigger click toggles.
            if is_open && !is_closing {
                // Start close.
                is_closing = true;
                ctx.data_mut(|d| d.insert_temp(closing_key, true));
            } else {
                // Open.
                is_open = true;
                is_closing = false;
                save_focus(ctx, prev_focus_key);
                push_overlay(ctx, self.id, OverlayLayer::Popover);
                ctx.data_mut(|d| {
                    d.insert_temp(is_open_key, true);
                    d.insert_temp(opened_key, true);
                });
            }
        }

        // ── Dismissal: Escape (only topmost) ──────────────────────────────
        if is_open && !is_closing && is_topmost(ctx, self.id) && escape_pressed(ctx, self.id) {
            is_open = false;
            is_closing = true;
            ctx.data_mut(|d| {
                d.insert_temp(is_open_key, false);
                d.insert_temp(closing_key, true);
            });
        }

        // ── Render ───────────────────────────────────────────────────────
        let _s = spatial(ui);
        let popover_rect = if is_open {
            // Anchor position clamped to viewport so the Area's fixed_pos is
            // already within screen bounds before rendering.
            let anchor_pos = compute_anchor_pos_clamped(
                trigger_response,
                self.direction,
                self.gap,
                screen_rect,
                self.max_width.unwrap_or(300.0) + 24.0, // width + frame inner margin
            );

            let area_id = self.id.with("__area");
            let area = egui::Area::new(area_id).fixed_pos(anchor_pos).order(Order::Foreground);

            let inner = area.show(ctx, |ui| {
                if let Some(mw) = self.max_width {
                    ui.set_max_width(mw);
                }
                let t = crate::tokens::theme::theme(ui);
                popover_frame(&t).show(ui, content).inner
            });

            let rect = inner.response.rect;

            // ── Click-outside detection ───────────────────────────────────
            if is_open && is_topmost(ctx, self.id) && clicked_outside(ui, rect, &inner.response) {
                is_open = false;
                is_closing = true;
                ctx.data_mut(|d| {
                    d.insert_temp(is_open_key, false);
                    d.insert_temp(closing_key, true);
                });
            }

            rect
        } else {
            Rect::NOTHING
        };

        // ── Finalise closing ─────────────────────────────────────────────
        if is_closing {
            finish_close(ctx, self.id, prev_focus_key, opened_key, closing_key);
        }

        PopoverResponse {
            is_open,
            popover_rect,
        }
    }
}

// ─── Focus helpers ───────────────────────────────────────────────────────────

/// Save the currently focused widget Id to Memory.
fn save_focus(ctx: &Context, key: Id) {
    // Read focus first (releases the Memory lock), THEN write to data.
    // egui stores `data` inside `Memory`, so `ctx.memory()` and `ctx.data_mut()`
    // lock the SAME RwLock — nesting them would deadlock (freeze).
    let focused = ctx.memory(|m| m.focused());
    ctx.data_mut(|d| {
        d.insert_temp(key, focused);
    });
}

/// Restore focus, remove overlay, and clean up state keys.
fn finish_close(
    ctx: &Context,
    popover_id: Id,
    prev_focus_key: Id,
    opened_key: Id,
    closing_key: Id,
) {
    remove_overlay(ctx, popover_id);

    // Read+clear the saved focus from data first (releases the lock), THEN
    // request focus via Memory. Nesting `ctx.memory_mut` inside `ctx.data_mut`
    // would deadlock because data lives inside Memory (same RwLock).
    let saved = ctx.data_mut(|d| {
        let saved = d.get_temp::<Option<Id>>(prev_focus_key).as_ref().and_then(|o| *o);
        d.remove::<bool>(opened_key);
        d.remove::<bool>(closing_key);
        d.remove::<Option<Id>>(prev_focus_key);
        saved
    });
    // Restore focus to the widget that was active before the popover opened.
    // Guard: if nothing had focus, `saved` is None and this is a no-op.
    if let Some(saved) = saved {
        ctx.memory_mut(|m| m.request_focus(saved));
    }
}

// ─── Memory key helpers ─────────────────────────────────────────────────────

fn open_key(id: Id) -> Id {
    id.with(OPEN_KEY)
}
fn prev_focus_key(id: Id) -> Id {
    id.with(PREV_FOCUS_KEY)
}
fn opened_key(id: Id) -> Id {
    id.with(OPENED_KEY)
}
fn closing_key(id: Id) -> Id {
    id.with(CLOSING_KEY)
}

// ─── Position computation ───────────────────────────────────────────────────

/// Compute the initial anchor position, clamped to the viewport so the
/// Area's `fixed_pos` is always within screen bounds.
fn compute_anchor_pos_clamped(
    trigger_response: &Response,
    direction: PopoverDirection,
    gap: f32,
    screen_rect: Rect,
    estimated_width: f32,
) -> Pos2 {
    let rect = trigger_response.rect;
    let mut pos = match direction {
        PopoverDirection::Below => Pos2::new(rect.left(), rect.bottom() + gap),
        PopoverDirection::Above => Pos2::new(rect.left(), rect.top() - gap),
        PopoverDirection::Right => Pos2::new(rect.right() + gap, rect.top()),
        PopoverDirection::Left => Pos2::new(rect.left() - gap, rect.top()),
    };

    // Clamp to viewport so the Area anchor is already in-bounds.
    let est_right = pos.x + estimated_width;
    if est_right > screen_rect.max.x {
        pos.x -= est_right - screen_rect.max.x;
    }
    if pos.x < screen_rect.min.x {
        pos.x = screen_rect.min.x;
    }
    if pos.y < screen_rect.min.y {
        pos.y = screen_rect.min.y;
    }
    pos
}

// ─── Frame styling ──────────────────────────────────────────────────────────

/// The themed frame used for the popover body.
///
/// Colours are sourced from the runtime `Theme`.
/// * fill → `t.surface.floating_card_bg`
/// * stroke → `t.border.default`
/// * corner radius → [`RADIUS_M`]
/// * shadow → `t.elevation.raised`
fn popover_frame(t: &crate::tokens::theme::Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(t.surface.floating_card_bg)
        .stroke(egui::Stroke::new(STROKE_WIDTH, t.border.default))
        .corner_radius(CornerRadius::same(RADIUS_M as u8))
        .inner_margin(egui::Margin::same(12))
        .shadow(t.elevation.raised)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use egui::Sense;

    use super::*;
    use crate::widget::overlay::{OverlayLayer, is_topmost, push_overlay, remove_overlay};

    // ── Builder API ──────────────────────────────────────────────────────

    #[test]
    fn builder_defaults() {
        let p = Popover::new("test");
        assert_eq!(p.direction, PopoverDirection::Below);
        assert!((p.gap - 8.0).abs() < f32::EPSILON);
        assert!(p.max_width.is_none());
        assert!(!p.arrow);
    }

    #[test]
    fn builder_below() {
        let p = Popover::new("test").below();
        assert_eq!(p.direction, PopoverDirection::Below);
    }

    #[test]
    fn builder_above() {
        let p = Popover::new("test").above();
        assert_eq!(p.direction, PopoverDirection::Above);
    }

    #[test]
    fn builder_right() {
        let p = Popover::new("test").right();
        assert_eq!(p.direction, PopoverDirection::Right);
    }

    #[test]
    fn builder_left() {
        let p = Popover::new("test").left();
        assert_eq!(p.direction, PopoverDirection::Left);
    }

    #[test]
    fn builder_gap() {
        let p = Popover::new("test").gap(16.0);
        assert!((p.gap - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn builder_max_width() {
        let p = Popover::new("test").max_width(200.0);
        assert_eq!(p.max_width, Some(200.0));
    }

    #[test]
    fn builder_with_arrow() {
        let p = Popover::new("test").with_arrow(true);
        assert!(p.arrow);
    }

    // ── State round-trips ────────────────────────────────────────────────

    #[test]
    fn is_open_defaults_to_false() {
        let ctx = Context::default();
        assert!(!Popover::is_open(&ctx, "nonexistent"));
    }

    #[test]
    fn is_open_reflects_open_call() {
        let ctx = Context::default();
        let popover = Popover::new("p1");
        assert!(!Popover::is_open(&ctx, "p1"));
        popover.open(&ctx, true);
        assert!(Popover::is_open(&ctx, "p1"));
    }

    #[test]
    fn is_open_reflects_close_call() {
        let ctx = Context::default();
        let popover = Popover::new("p2");
        popover.open(&ctx, true);
        popover.close(&ctx);
        assert!(!Popover::is_open(&ctx, "p2"));
    }

    #[test]
    fn is_open_reflects_toggle() {
        let ctx = Context::default();
        let popover = Popover::new("p3");

        popover.open(&ctx, true);
        assert!(Popover::is_open(&ctx, "p3"));

        popover.open(&ctx, false);
        assert!(!Popover::is_open(&ctx, "p3"));

        popover.open(&ctx, true);
        assert!(Popover::is_open(&ctx, "p3"));
    }

    // ── Overlay coordination ─────────────────────────────────────────────

    #[test]
    fn open_registers_with_overlay() {
        let ctx = Context::default();
        let popover = Popover::new("ol1");

        // `open()` writes the state flag; push_overlay registers with the
        // coordination layer (mirrors what `show()` does on closed→open).
        push_overlay(&ctx, popover.id, OverlayLayer::Popover);
        assert!(is_topmost(&ctx, popover.id), "popover should be topmost after push");

        remove_overlay(&ctx, popover.id);
        assert!(!is_topmost(&ctx, popover.id), "popover should not be topmost after removal");
    }

    #[test]
    fn close_removes_from_overlay() {
        let ctx = Context::default();
        let popover = Popover::new("ol2");
        push_overlay(&ctx, popover.id, OverlayLayer::Popover);
        assert!(is_topmost(&ctx, popover.id), "popover should be topmost after push");

        remove_overlay(&ctx, popover.id);
        assert!(!is_topmost(&ctx, popover.id), "popover should not be topmost after removal");
    }

    // ── Multiple popovers ────────────────────────────────────────────────

    #[test]
    fn multiple_popovers_coordinate() {
        let ctx = Context::default();
        let p1 = Popover::new("mp1");
        let p2 = Popover::new("mp2");

        push_overlay(&ctx, p1.id, OverlayLayer::Popover);
        push_overlay(&ctx, p2.id, OverlayLayer::Popover);

        // p2 was pushed last → it is topmost.
        assert!(is_topmost(&ctx, p2.id));
        assert!(!is_topmost(&ctx, p1.id));

        remove_overlay(&ctx, p2.id);
        assert!(is_topmost(&ctx, p1.id), "p1 should become topmost after removing p2");
    }

    // ── Focus save via show() ────────────────────────────────────────────

    /// Verify that `show()` snapshots the currently focused widget Id into
    /// Memory (mirrors dialog.rs D3 pattern).
    #[test]
    fn show_saves_focus_on_open() {
        let ctx = Context::default();

        let viewport_rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        // `Ui::new` takes ownership of Context; clone to retain `ctx` for assertions.
        let mut ui = Ui::new(
            ctx.clone(),
            Id::new("popover_focus_test"),
            egui::UiBuilder::new().max_rect(viewport_rect),
        );

        // Register a focusable widget so `m.focused()` returns Some(...) after
        // `ui.interact` processes it.
        let dummy_id = Id::new("dummy_focus_target");
        ui.interact(
            Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 130.0)),
            dummy_id,
            Sense::click(),
        );

        let trigger_response = ui.interact(
            Rect::from_min_max(Pos2::new(10.0, 10.0), Pos2::new(60.0, 40.0)),
            Id::new("trigger"),
            Sense::click(),
        );

        let popover = Popover::new("focus_popover").below();
        popover.open(&ctx, true);

        // `show()` should save whatever `m.focused()` was at call time.
        let _resp = popover.show(&mut ui, &trigger_response, |_| {});

        let focused_at_show = ctx.memory(|m| m.focused());
        ctx.data(|d| {
            let saved: Option<Id> = d.get_temp(prev_focus_key(popover.id)).unwrap_or(None);
            assert_eq!(
                saved, focused_at_show,
                "save_focus should snapshot m.focused() at show() time"
            );
        });
    }

    // ── Lifecycle clean-up on close ──────────────────────────────────────

    /// Verify that after closing, the overlay is removed and the open state
    /// flag is cleared (focus restore mirrors dialog.rs D3 pattern).
    #[test]
    fn show_cleans_up_on_close() {
        let ctx = Context::default();
        let viewport_rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        let mut ui = Ui::new(
            ctx.clone(),
            Id::new("popover_cleanup_test"),
            egui::UiBuilder::new().max_rect(viewport_rect),
        );

        let trigger_response = ui.interact(
            Rect::from_min_max(Pos2::new(10.0, 10.0), Pos2::new(60.0, 40.0)),
            Id::new("trigger3"),
            Sense::click(),
        );

        let popover = Popover::new("cleanup_popover").below();

        // Frame 1: open.
        popover.open(&ctx, true);
        let resp1 = popover.show(&mut ui, &trigger_response, |_| {});
        assert!(resp1.is_open, "popover should be open after frame 1");

        // Frame 2: close.
        popover.open(&ctx, false);
        let resp2 = popover.show(&mut ui, &trigger_response, |_| {});
        assert!(!resp2.is_open, "popover should be closed after frame 2");

        // State should be cleared from data after close.
        ctx.data(|d| {
            let still_open: Option<bool> = d.get_temp(open_key(popover.id));
            assert!(
                still_open.is_none() || !still_open.unwrap(),
                "open state flag should be absent or false after close"
            );
        });
    }
}
