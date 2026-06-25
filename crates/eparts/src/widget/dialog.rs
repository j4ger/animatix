//! Reusable modal dialog widget — centered `egui::Window` with backdrop,
//! consistent Pattern B styling, and a standard title-row helper.
//!
//! # Constraint
//!
//! Only one dialog with a given `id` may be open at a time (egui uses the
//! id string as the window identifier). This is safe in the current UX since
//! at most one modal is open at once.

use egui::{Align2, Margin, Stroke, Ui};

use crate::widget::anim;
use crate::tokens::motion;
use crate::tokens::semantic::{border, overlay, surface, text};
use crate::tokens::spatial::{self, RADIUS_XL, STROKE_WIDTH};
use crate::tokens::spatial::dialog as dialog_token;
use crate::tokens::typography::TextRole;

/// Context passed to the dialog body on each frame.
pub struct DialogCtx {
    /// Set to `true` on the very first frame the dialog is rendered.
    /// Useful for requesting initial focus on a widget.
    #[allow(dead_code)] // Reserved for CommandPalette/FindReplace focus-on-open (Phase 5)
    pub first_frame: bool,
}

/// Configuration for a centered modal dialog.
pub struct DialogSpec<'a> {
    /// Used as the `egui::Window` id seed — must be unique per open dialog.
    pub id: &'a str,
    pub default_size: [f32; 2],
    pub min_size: [f32; 2],
    pub max_size: Option<[f32; 2]>,
    pub resizable: bool,
    pub max_viewport_frac: [f32; 2],
    /// Anchor offset from `Align2::CENTER_CENTER`; default `[0.0, 0.0]`.
    pub anchor_offset: [f32; 2],
}

impl<'a> DialogSpec<'a> {
    pub fn new(id: &'a str, default_size: [f32; 2]) -> Self {
        Self {
            id,
            default_size,
            min_size: default_size,
            max_size: None,
            resizable: false,
            max_viewport_frac: crate::tokens::spatial::dialog::MAX_VIEWPORT_FRAC,
            anchor_offset: [0.0, 0.0],
        }
    }

    /// Set a smaller minimum size than the default.
    pub fn with_min_size(mut self, min_size: [f32; 2]) -> Self {
        self.min_size = min_size;
        self
    }

    /// Allow the dialog to be resized by the user.
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Cap the maximum size of the dialog.
    pub fn with_max_size(mut self, max_size: [f32; 2]) -> Self {
        self.max_size = Some(max_size);
        self
    }

    /// Set custom viewport-relative sizing fractions (width, height).
    #[allow(dead_code)] // Reserved for future Export dialog with viewport-relative sizing
    pub fn with_max_viewport_frac(mut self, frac: [f32; 2]) -> Self {
        self.max_viewport_frac = frac;
        self
    }

    /// Offset from screen center (e.g. `[0.0, -80.0]` for command palette).
    pub fn with_anchor_offset(mut self, offset: [f32; 2]) -> Self {
        self.anchor_offset = offset;
        self
    }
}

/// Draws the modal backdrop (full-viewport dim), intercepts Escape + backdrop
/// click to request close, then shows a centered `egui::Window` with the
/// standard Pattern B frame.
///
/// Returns `true` while the dialog is open, `false` when it should close.
///
/// `body` receives the window's inner `&mut Ui` plus a [`DialogCtx`] and
/// returns `true` if the body requests to close the dialog (e.g., via the
/// title close button).
pub fn modal(
    ui: &mut Ui,
    spec: &DialogSpec,
    body: impl FnOnce(&mut Ui, &DialogCtx) -> bool,
) -> bool {
    let ctx = ui.ctx();
    let screen_rect = ctx.viewport_rect();

    // ── Animation state ──
    let anim_id = egui::Id::new(spec.id).with("anim");
    let closing_id = egui::Id::new(spec.id).with("closing");
    let opened_id = egui::Id::new(spec.id).with("opened");

    // Read current closing state (persists across frames)
    let is_closing = ctx.data(|d| d.get_temp::<bool>(closing_id).unwrap_or(false));

    // First-ever-frame detection: seed animation value at 0.0 so the
    // entrance transition doesn't snap to 1.0 on the very first summon
    // (egui's animate_value_with_time returns the target immediately
    // for a brand-new id that has no previous value).
    let first_frame = !ctx.data(|d| d.get_temp::<bool>(opened_id).unwrap_or(false));
    if first_frame && !is_closing {
        ctx.animate_value_with_time(anim_id, 0.0, 0.0);
        ctx.data_mut(|d| d.insert_temp(opened_id, true));
    }

    // Use separate transitions for open vs close:
    //   - Open:  MODAL (DECELERATE, 0.40s) — fast rise, gentle settle
    //   - Close: MODAL_EXIT (STANDARD, 0.20s) — shorter symmetric exit;
    //            avoids the front-loaded ghost-tail stall of DECELERATE
    //            applied to the closing direction.
    let transition = if is_closing { motion::MODAL_EXIT } else { motion::MODAL };
    let anim_target = if is_closing { 0.0 } else { 1.0 };
    let raw_progress = anim::animate_toward(ctx, anim_id, anim_target, transition);

    // Apply easing based on direction:
    let progress = if is_closing {
        let close_t = 1.0 - raw_progress; // 0→1 over close duration
        // 1 - STANDARD(close_t): openness drops uniformly; STANDARD's
        // symmetric round-trip avoids the long phantom tail of DECELERATE.
        1.0 - transition.easing.sample(close_t)
    } else {
        // DECELERATE on 0→1: fast initial rise, gentle settle
        transition.easing.sample(raw_progress)
    };

    // ── Animated backdrop (painted before window, layered behind it) ──
    let bg = overlay::backdrop();
    let alpha = (bg.a() as f32 * progress).round() as u8;
    let backdrop_color =
        egui::Color32::from_rgba_premultiplied(bg.r(), bg.g(), bg.b(), alpha);
    ui.painter().rect_filled(screen_rect, 0.0, backdrop_color);

    // Close on backdrop click (gated until the dialog is visually established)
    let backdrop_id = egui::Id::new(spec.id).with("backdrop");
    let backdrop = ui.interact(screen_rect, backdrop_id, egui::Sense::click());
    let backdrop_clicked = backdrop.clicked() && progress > 0.05;

    // ── Window fill and border opacity — scales with animation progress ──
    let border_color = egui::Color32::from_rgba_premultiplied(
        border::DEFAULT.r(),
        border::DEFAULT.g(),
        border::DEFAULT.b(),
        (border::DEFAULT.a() as f32 * progress).round() as u8,
    );
    let window_bg = egui::Color32::from_rgba_premultiplied(
        surface::BASE.r(),
        surface::BASE.g(),
        surface::BASE.b(),
        (surface::BASE.a() as f32 * progress).round() as u8,
    );

    // ── Slide offset for window ──
    let slide_offset = dialog_token::SLIDE_PX * (1.0 - progress);

    // ── Compute viewport-relative effective size ──
    let viewport = ctx.viewport_rect().size();
    let effective_size = [
        spec.default_size[0]
            .min((viewport.x * spec.max_viewport_frac[0]).max(spec.min_size[0]))
            .max(spec.min_size[0]),
        spec.default_size[1]
            .min((viewport.y * spec.max_viewport_frac[1]).max(spec.min_size[1]))
            .max(spec.min_size[1]),
    ];

    // ── Centered dialog using egui window for proper layout ──
    let window = egui::Window::new(spec.id)
        .anchor(
            Align2::CENTER_CENTER,
            [spec.anchor_offset[0], spec.anchor_offset[1] + slide_offset],
        )
        .default_size(effective_size)
        .min_size(if spec.resizable {
            spec.min_size
        } else {
            effective_size
        })
        .resizable(spec.resizable)
        .collapsible(false)
        .title_bar(false)
        .frame(
            egui::Frame::new()
                .fill(window_bg)
                .stroke(Stroke::new(STROKE_WIDTH, border_color))
                .corner_radius(RADIUS_XL)
                .inner_margin(Margin::same(spatial::dialog::INNER_MARGIN as i8)),
        );

    let window = if spec.resizable {
        if let Some(max) = spec.max_size {
            window.max_size(max)
        } else {
            window
        }
    } else {
        window.max_size(effective_size)
    };

    let resp = window.show(ctx, |window_ui| {
        // Fade window content (widgets, text, etc.) with animation progress
        window_ui.set_opacity(progress);
        window_ui.set_min_width(spec.min_size[0] - 2.0 * spatial::dialog::INNER_MARGIN);
        let dc = DialogCtx { first_frame };
        body(window_ui, &dc)
    });

    // Inner is None if the window was not shown (e.g., collapsed by area system)
    let body_close = resp.map(|r| r.inner.unwrap_or(true)).unwrap_or(true);

    // ── Close request detection ──
    let close_requested = ctx.input(|i| i.key_pressed(egui::Key::Escape))
        || backdrop_clicked
        || body_close;

    // Start closing animation (only once, on first close request)
    if close_requested && !is_closing {
        ctx.data_mut(|d| d.insert_temp(closing_id, true));
    }

    // Request repaint during animation for smooth transitions
    if progress > 0.01 && progress < 0.99 {
        ctx.request_repaint();
    }

    // When fully closed (egui reached the target value via animate_value_with_time),
    // clean up and hide. Keys off `raw_progress` (the actual animation value) rather
    // than the eased progress, so the dialog hides exactly when the animation
    // completes with no threshold-magic ghost tail.
    let fully_closed = is_closing && raw_progress <= 0.0;
    if fully_closed {
        ctx.data_mut(|d| {
            d.remove::<bool>(closing_id);
            d.remove::<bool>(opened_id);
        });
    }

    !fully_closed // returns `true` while the dialog should stay visible (open or animating closed)
}

/// Renders the standard title row: heading on the left, X close button on the right.
///
/// Returns `true` if the close button was clicked.
pub fn title_row(ui: &mut Ui, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(TextRole::Heading.size()).color(text::PRIMARY));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(egui_phosphor::regular::X).on_hover_text("Close (Esc)").clicked() {
                close = true;
            }
        });
    });
    close
}
