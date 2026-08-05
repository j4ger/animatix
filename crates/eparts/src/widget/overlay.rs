//! Lightweight managed overlay COORDINATION layer.
//!
//! # What this is
//!
//! This module provides **priority ordering** and **uniform dismissal helpers**
//! for floating UI overlays (dialogs, popovers, tooltips, etc.).  It is *not* a
//! compositor or mandatory root view — egui's [`Area`]/[`Order`] system does
//! the actual compositing and painting.  This layer only answers two questions:
//!
//! 1. *Which overlay is topmost?* — so that Escape and outside-click are consumed by exactly one
//!    overlay (the highest-priority, most-recently-opened one).
//! 2. *Did a dismissal event fire this frame?* — uniform helpers so every overlay type handles
//!    Escape and click-outside identically.
//!
//! # Overlay priority (low → high)
//!
//! | Layer      | egui `Order`         | Typical use           |
//! |------------|----------------------|-----------------------|
//! | `Dialog`   | `Order::Foreground`  | Modal dialogs         |
//! | `Popover`  | `Order::Foreground`  | Dropdown menus, etc.  |
//! | `Tooltip`  | `Order::Tooltip`     | Hover tooltips        |
//!
//! Within the same `Order`, callers should use a monotonically increasing
//! relative z (see [`OverlayLayer::to_order`]) so that newer overlays paint
//! above older ones.
//!
//! # State model
//!
//! The open-overlay registry lives in [`egui::Memory`] (`ctx.data`) as a
//! `Vec<(Id, OverlayLayer)>`.  No rendering is performed here.
//!
//! # Usage pattern
//!
//! ```ignore
//! // When opening:
//! overlay::push_overlay(ctx, egui::Id::new("my_dialog"), overlay::OverlayLayer::Dialog);
//!
//! // In your widget's update loop:
//! if overlay::is_topmost(ctx, my_id) && overlay::escape_pressed(ctx) {
//!     request_close();
//! }
//! let dismissed = overlay::clicked_outside(ui, content_rect, &response);
//!
//! // When closing (frame after):
//! overlay::remove_overlay(ctx, egui::Id::new("my_dialog"));
//! ```
//!
//! [`Area`]: egui::Area
//! [`Order`]: egui::Order

#[allow(unused_imports)]
use egui::{Context, Id, Order, Pos2, Rect, Response, Sense, Ui};

// ─── Overlay layer priority ────────────────────────────────────────────────

/// An overlay type with an implicit priority ordering.
///
/// Priority (low → high): `Dialog` < `Popover` < `Tooltip`.
///
/// This ordering determines which overlay is considered "topmost" when
/// multiple overlays are open simultaneously, and therefore which one
/// receives Escape-key / outside-click dismissal events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OverlayLayer {
    /// Full-viewport modal dialogs (lowest priority — never steal from popovers/tooltips).
    Dialog = 0,
    /// Anchored popovers / dropdowns / menus (mid priority).
    Popover = 1,
    /// Transient tooltip popups (highest priority).
    Tooltip = 2,
}

impl OverlayLayer {
    /// Map this layer to an [`egui::Order`].
    ///
    /// `Dialog` and `Popover` use `Order::Foreground` (they share the same
    /// z-plane; use [`egui::Area::order`] with a relative z offset to
    /// separate them within that plane). `Tooltip` uses `Order::Tooltip`,
    /// which egui paints above `Foreground`.
    pub fn to_order(self) -> Order {
        match self {
            Self::Dialog | Self::Popover => Order::Foreground,
            Self::Tooltip => Order::Tooltip,
        }
    }
}

// ─── Memory key ───────────────────────────────────────────────────────────

/// Memory key for the overlay registry stored in `ctx.data`.
///
/// We use a plain `Id::new(...)` at each call site (not a `const`) because
/// `Id::new` is not a const fn in egui 0.34.
fn overlay_registry_key() -> Id {
    Id::new("eparts_overlay_registry")
}

// ─── Registry API ─────────────────────────────────────────────────────────

/// Open an overlay: register `id` with `layer` in the coordination registry.
///
/// Call this at the start of the frame where the overlay becomes visible.
/// If `id` is already registered it will be re-registered (its insertion
/// position is moved to the end).
pub fn push_overlay(ctx: &Context, id: Id, layer: OverlayLayer) {
    ctx.data_mut(|data| {
        let mut registry: Vec<(Id, OverlayLayer)> =
            data.get_temp(overlay_registry_key()).unwrap_or_default();

        // Remove stale entry if present (moves it to end on re-push).
        registry.retain(|(existing_id, _)| *existing_id != id);
        registry.push((id, layer));
        data.insert_temp(overlay_registry_key(), registry);
    });
}

/// Close an overlay: remove `id` from the coordination registry.
///
/// Call this when the overlay has fully dismissed (including after any
/// closing animation).
pub fn remove_overlay(ctx: &Context, id: Id) {
    ctx.data_mut(|data| {
        let Some(mut registry): Option<Vec<(Id, OverlayLayer)>> =
            data.get_temp(overlay_registry_key())
        else {
            return;
        };
        registry.retain(|(existing_id, _)| *existing_id != id);
        data.insert_temp(overlay_registry_key(), registry);
    });
}

/// Return `true` if `id` is the currently topmost overlay.
///
/// "Topmost" = highest [`OverlayLayer`] priority, and (for ties) most
/// recently pushed via [`push_overlay`].
pub fn is_topmost(ctx: &Context, id: Id) -> bool {
    ctx.data(|data| {
        let registry: Vec<(Id, OverlayLayer)> =
            data.get_temp(overlay_registry_key()).unwrap_or_default();

        let topmost = registry.iter().max_by(|a, b| {
            // Primary: layer priority (highest wins)
            a.1.cmp(&b.1)
                // Secondary: insertion order (most recent wins)
                .then_with(|| {
                    registry
                        .iter()
                        .position(|(i, _)| i == &a.0)
                        .cmp(&registry.iter().position(|(i, _)| i == &b.0))
                })
        });

        topmost.map(|(top_id, _)| *top_id == id).unwrap_or(false)
    })
}

// ─── Dismissal helpers ─────────────────────────────────────────────────────

/// Returns `true` on the exact frame the Escape key is pressed.
///
/// The Escape event is only consumed when the calling overlay `id` **is**
/// the current topmost overlay (per [`is_topmost`]).  Lower-priority
/// overlays that call this function get `false` and the Escape event remains
/// available for the actual topmost overlay to consume.
///
/// # Call pattern
///
/// ```ignore
/// if overlay::is_topmost(ctx, my_id) && overlay::escape_pressed(ctx, my_id) {
///     request_close();
/// }
/// ```
pub fn escape_pressed(ctx: &Context, id: Id) -> bool {
    // Only consume if this overlay is currently the topmost one.
    if !is_topmost(ctx, id) {
        return false;
    }
    ctx.input(|input| input.key_pressed(egui::Key::Escape))
}

/// Returns `true` if a primary pointer click happened **outside**
/// `content_rect` this frame — the standard click-outside-to-dismiss event.
///
/// The detection requires that a primary click (press + release) completed
/// this frame and the release position is outside `content_rect`.
///
/// The widget's [`Response`] (`response`) is accepted for API symmetry; in
/// a future iteration it may gate the check behind `response.interact()`.
pub fn clicked_outside(_ui: &Ui, content_rect: Rect, _response: &Response) -> bool {
    // Cloning pointer state — PointerState is not Copy.
    let Some(pointer) = _ui.ctx().input(|i| Some(i.pointer.clone())) else {
        return false;
    };

    if !pointer.primary_clicked() {
        return false;
    }

    let click_pos = match pointer.interact_pos() {
        Some(pos) => pos,
        None => return false,
    };

    !content_rect.contains(click_pos)
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- Layer ordering ---

    #[test]
    fn layer_ordering_is_correct() {
        assert!(OverlayLayer::Tooltip > OverlayLayer::Popover);
        assert!(OverlayLayer::Popover > OverlayLayer::Dialog);
    }

    #[test]
    fn layer_to_order_maps_correctly() {
        assert_eq!(OverlayLayer::Dialog.to_order(), Order::Foreground);
        assert_eq!(OverlayLayer::Popover.to_order(), Order::Foreground);
        assert_eq!(OverlayLayer::Tooltip.to_order(), Order::Tooltip);
    }

    // --- Registry ---

    #[test]
    fn empty_registry_no_topmost() {
        let ctx = Context::default();
        let id = Id::new("nonexistent");
        assert!(!is_topmost(&ctx, id));
    }

    #[test]
    fn single_overlay_is_topmost() {
        let ctx = Context::default();
        let id = Id::new("d1");
        push_overlay(&ctx, id, OverlayLayer::Dialog);
        assert!(is_topmost(&ctx, id));
    }

    #[test]
    fn higher_priority_layer_is_topmost() {
        let ctx = Context::default();
        let dialog = Id::new("dialog");
        let tooltip = Id::new("tooltip");

        // Push dialog first, then tooltip.
        push_overlay(&ctx, dialog, OverlayLayer::Dialog);
        push_overlay(&ctx, tooltip, OverlayLayer::Tooltip);

        assert!(!is_topmost(&ctx, dialog));
        assert!(is_topmost(&ctx, tooltip));
    }

    #[test]
    fn higher_priority_wins_even_if_pushed_first() {
        let ctx = Context::default();
        let tooltip = Id::new("tooltip");
        let dialog = Id::new("dialog");

        // Push tooltip first, then dialog — priority still wins.
        push_overlay(&ctx, tooltip, OverlayLayer::Tooltip);
        push_overlay(&ctx, dialog, OverlayLayer::Dialog);

        assert!(is_topmost(&ctx, tooltip));
        assert!(!is_topmost(&ctx, dialog));
    }

    #[test]
    fn popover_above_dialog() {
        let ctx = Context::default();
        let dialog = Id::new("dialog");
        let popover = Id::new("popover");

        push_overlay(&ctx, dialog, OverlayLayer::Dialog);
        push_overlay(&ctx, popover, OverlayLayer::Popover);

        assert!(!is_topmost(&ctx, dialog));
        assert!(is_topmost(&ctx, popover));
    }

    #[test]
    fn remove_overlay_updates_topmost() {
        let ctx = Context::default();
        let dialog = Id::new("dialog");
        let tooltip = Id::new("tooltip");

        push_overlay(&ctx, dialog, OverlayLayer::Dialog);
        push_overlay(&ctx, tooltip, OverlayLayer::Tooltip);
        assert!(is_topmost(&ctx, tooltip));

        remove_overlay(&ctx, tooltip);
        // After removing tooltip, dialog should become topmost.
        assert!(is_topmost(&ctx, dialog));
        assert!(!is_topmost(&ctx, tooltip));
    }

    #[test]
    fn remove_overlay_unknown_id_is_noop() {
        let ctx = Context::default();
        let id = Id::new("exists");
        push_overlay(&ctx, id, OverlayLayer::Dialog);
        remove_overlay(&ctx, Id::new("unknown"));
        assert!(is_topmost(&ctx, id));
    }

    #[test]
    fn re_push_overlay_moves_to_end() {
        let ctx = Context::default();
        let d1 = Id::new("d1");
        let d2 = Id::new("d2");
        push_overlay(&ctx, d1, OverlayLayer::Dialog);
        push_overlay(&ctx, d2, OverlayLayer::Dialog);
        assert!(is_topmost(&ctx, d2));

        // Re-push d1 — it becomes topmost because it's newer.
        push_overlay(&ctx, d1, OverlayLayer::Dialog);
        assert!(is_topmost(&ctx, d1));
        assert!(!is_topmost(&ctx, d2));
    }

    // --- Escape key ---

    #[test]
    fn escape_pressed_topmost_consumes() {
        let ctx = Context::default();
        let id = Id::new("top");
        push_overlay(&ctx, id, OverlayLayer::Dialog);

        // Simulate an Escape key press (egui 0.34 Event::Key requires `repeat`).
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        });

        let triggered = escape_pressed(&ctx, id);
        assert!(triggered, "topmost overlay should see Escape press");
    }

    #[test]
    fn escape_pressed_non_topmost_does_not_consume() {
        let ctx = Context::default();
        let low = Id::new("low");
        let high = Id::new("high");

        push_overlay(&ctx, low, OverlayLayer::Dialog);
        push_overlay(&ctx, high, OverlayLayer::Tooltip);

        // Inject Escape press.
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        });

        // The lower-priority overlay must not consume; returns false.
        let low_triggered = escape_pressed(&ctx, low);
        assert!(!low_triggered, "non-topmost overlay must not consume Escape");
    }

    #[test]
    fn escape_pressed_no_overlay_noop() {
        let ctx = Context::default();
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        });
        let none_id = Id::new("none");
        assert!(!escape_pressed(&ctx, none_id));
    }

    // --- click_outside ---
    //
    // `clicked_outside` detects a primary pointer click outside `content_rect`.
    // With a fresh Context (no pointer activity) it must return false.

    #[test]
    fn clicked_outside_no_click_returns_false() {
        let ctx = Context::default();
        let viewport_rect = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 200.0));
        // egui 0.34: Ui::new(ctx, id, UiBuilder::new(rect))
        let ui = egui::Ui::new(
            ctx,
            Id::new("overlay_test"),
            egui::UiBuilder::new().max_rect(viewport_rect),
        );
        let content_rect = Rect::from_min_max(Pos2::new(50.0, 50.0), Pos2::new(150.0, 150.0));
        let response = ui.interact(content_rect, Id::new("content"), Sense::hover());

        assert!(!clicked_outside(&ui, content_rect, &response));
    }
}
