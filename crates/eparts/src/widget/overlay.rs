//! Overlay coordination: priority ordering and uniform dismissal helpers for
//! floating UI (dialogs, popovers, tooltips). egui's [`Area`]/[`Order`] does the
//! compositing; this layer answers which overlay is topmost (so Escape and
//! click-outside are consumed by exactly one) and whether a dismissal event
//! fired this frame.
//!
//! Priority (low → high): `Dialog` < `Popover` < `Tooltip`. `Dialog` and
//! `Popover` share `Order::Foreground`; `Tooltip` uses `Order::Tooltip`. The
//! registry lives in [`egui::Memory`] (`ctx.data`) as `Vec<(Id, OverlayLayer)>`.
//!
//! ```ignore
//! overlay::push_overlay(ctx, id, OverlayLayer::Dialog);
//! if overlay::is_topmost(ctx, id) && overlay::escape_pressed(ctx, id) { close(); }
//! overlay::remove_overlay(ctx, id);
//! ```
//!
//! [`Area`]: egui::Area
//! [`Order`]: egui::Order

#[allow(unused_imports)]
use egui::{Context, Id, Order, Pos2, Rect, Response, Sense, Ui};

/// An overlay type with an implicit priority ordering: `Dialog` < `Popover`
/// < `Tooltip`. It decides which open overlay is "topmost", and therefore
/// which one receives Escape / click-outside dismissal.
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
    /// Map to an [`egui::Order`]. `Dialog` and `Popover` share
    /// `Order::Foreground` (separate them with a relative z offset); `Tooltip`
    /// uses `Order::Tooltip`, which egui paints above `Foreground`.
    pub fn to_order(self) -> Order {
        match self {
            Self::Dialog | Self::Popover => Order::Foreground,
            Self::Tooltip => Order::Tooltip,
        }
    }
}

/// Registry key in `ctx.data`; `Id::unique` is computed per call because it is not const.
fn overlay_registry_key() -> Id {
    Id::unique("eparts_overlay_registry")
}

/// Register `id` at `layer`; a re-push moves it to the end (newest wins).
pub fn push_overlay(ctx: &Context, id: Id, layer: OverlayLayer) {
    ctx.data_mut(|data| {
        let mut registry: Vec<(Id, OverlayLayer)> =
            data.get_temp(overlay_registry_key()).unwrap_or_default();

        registry.retain(|(existing_id, _)| *existing_id != id);
        registry.push((id, layer));
        data.insert_temp(overlay_registry_key(), registry);
    });
}

/// Remove `id`; call once the overlay has fully dismissed (after any closing animation).
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

/// True if `id` is topmost: highest [`OverlayLayer`] priority, ties broken by
/// the most recent [`push_overlay`].
pub fn is_topmost(ctx: &Context, id: Id) -> bool {
    ctx.data(|data| {
        let registry: Vec<(Id, OverlayLayer)> =
            data.get_temp(overlay_registry_key()).unwrap_or_default();

        let topmost = registry.iter().max_by(|a, b| {
            a.1.cmp(&b.1)
                // Ties break by insertion order (most recent wins).
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

/// True on the frame Escape is pressed and `id` is the topmost overlay;
/// lower overlays get `false`, leaving the event for the topmost one.
///
/// ```ignore
/// if overlay::is_topmost(ctx, my_id) && overlay::escape_pressed(ctx, my_id) { close(); }
/// ```
pub fn escape_pressed(ctx: &Context, id: Id) -> bool {
    if !is_topmost(ctx, id) {
        return false;
    }
    ctx.input(|input| input.key_pressed(egui::Key::Escape))
}

/// True if a primary click (press + release) completed this frame outside
/// `content_rect`. `response` is accepted for API symmetry only; it does not
/// gate the check.
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn empty_registry_no_topmost() {
        let ctx = Context::default();
        let id = Id::unique("nonexistent");
        assert!(!is_topmost(&ctx, id));
    }

    #[test]
    fn single_overlay_is_topmost() {
        let ctx = Context::default();
        let id = Id::unique("d1");
        push_overlay(&ctx, id, OverlayLayer::Dialog);
        assert!(is_topmost(&ctx, id));
    }

    #[test]
    fn higher_priority_layer_is_topmost() {
        let ctx = Context::default();
        let dialog = Id::unique("dialog");
        let tooltip = Id::unique("tooltip");

        push_overlay(&ctx, dialog, OverlayLayer::Dialog);
        push_overlay(&ctx, tooltip, OverlayLayer::Tooltip);

        assert!(!is_topmost(&ctx, dialog));
        assert!(is_topmost(&ctx, tooltip));
    }

    #[test]
    fn higher_priority_wins_even_if_pushed_first() {
        let ctx = Context::default();
        let tooltip = Id::unique("tooltip");
        let dialog = Id::unique("dialog");

        push_overlay(&ctx, tooltip, OverlayLayer::Tooltip);
        push_overlay(&ctx, dialog, OverlayLayer::Dialog);

        assert!(is_topmost(&ctx, tooltip));
        assert!(!is_topmost(&ctx, dialog));
    }

    #[test]
    fn popover_above_dialog() {
        let ctx = Context::default();
        let dialog = Id::unique("dialog");
        let popover = Id::unique("popover");

        push_overlay(&ctx, dialog, OverlayLayer::Dialog);
        push_overlay(&ctx, popover, OverlayLayer::Popover);

        assert!(!is_topmost(&ctx, dialog));
        assert!(is_topmost(&ctx, popover));
    }

    #[test]
    fn remove_overlay_updates_topmost() {
        let ctx = Context::default();
        let dialog = Id::unique("dialog");
        let tooltip = Id::unique("tooltip");

        push_overlay(&ctx, dialog, OverlayLayer::Dialog);
        push_overlay(&ctx, tooltip, OverlayLayer::Tooltip);
        assert!(is_topmost(&ctx, tooltip));

        remove_overlay(&ctx, tooltip);
        assert!(is_topmost(&ctx, dialog));
        assert!(!is_topmost(&ctx, tooltip));
    }

    #[test]
    fn remove_overlay_unknown_id_is_noop() {
        let ctx = Context::default();
        let id = Id::unique("exists");
        push_overlay(&ctx, id, OverlayLayer::Dialog);
        remove_overlay(&ctx, Id::unique("unknown"));
        assert!(is_topmost(&ctx, id));
    }

    #[test]
    fn re_push_overlay_moves_to_end() {
        let ctx = Context::default();
        let d1 = Id::unique("d1");
        let d2 = Id::unique("d2");
        push_overlay(&ctx, d1, OverlayLayer::Dialog);
        push_overlay(&ctx, d2, OverlayLayer::Dialog);
        assert!(is_topmost(&ctx, d2));

        push_overlay(&ctx, d1, OverlayLayer::Dialog);
        assert!(is_topmost(&ctx, d1));
        assert!(!is_topmost(&ctx, d2));
    }

    #[test]
    fn escape_pressed_topmost_consumes() {
        let ctx = Context::default();
        let id = Id::unique("top");
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
        let low = Id::unique("low");
        let high = Id::unique("high");

        push_overlay(&ctx, low, OverlayLayer::Dialog);
        push_overlay(&ctx, high, OverlayLayer::Tooltip);

        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        });

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
        let none_id = Id::unique("none");
        assert!(!escape_pressed(&ctx, none_id));
    }

    #[test]
    fn clicked_outside_no_click_returns_false() {
        let ctx = Context::default();
        let viewport_rect = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 200.0));
        let ui = egui::Ui::new(
            ctx,
            Id::unique("overlay_test"),
            egui::UiBuilder::new().max_rect(viewport_rect),
        );
        let content_rect = Rect::from_min_max(Pos2::new(50.0, 50.0), Pos2::new(150.0, 150.0));
        let response = ui.interact(content_rect, Id::unique("content"), Sense::hover());

        assert!(!clicked_outside(&ui, content_rect, &response));
    }
}
