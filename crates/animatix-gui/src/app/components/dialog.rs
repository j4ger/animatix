//! Reusable modal dialog widget — centered `egui::Window` with backdrop,
//! consistent Pattern B styling, and a standard title-row helper.
//!
//! # Constraint
//!
//! Only one dialog with a given `id` may be open at a time (egui uses the
//! id string as the window identifier). This is safe in the current UX since
//! at most one modal is open at once.

use egui::{Align2, Margin, Stroke, Ui};

use crate::app::design_tokens::semantic::{border, overlay, surface, text};
use crate::app::design_tokens::spatial::{RADIUS_XL, SPACE_XL, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;

/// Configuration for a centered modal dialog.
pub struct DialogSpec<'a> {
    /// Used as the `egui::Window` id seed — must be unique per open dialog.
    pub id: &'a str,
    pub default_size: [f32; 2],
    pub min_size: [f32; 2],
    pub max_size: Option<[f32; 2]>,
    pub resizable: bool,
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
            anchor_offset: [0.0, 0.0],
        }
    }

    /// Set a smaller minimum size than the default.
    pub fn with_min_size(mut self, min_size: [f32; 2]) -> Self {
        self.min_size = min_size;
        self
    }

    /// Allow the dialog to be resized by the user.
    #[allow(dead_code)] // Reserved for migrating Settings and Export dialogs
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Cap the maximum size of the dialog.
    #[allow(dead_code)] // Reserved for migrating Settings and Export dialogs
    pub fn with_max_size(mut self, max_size: [f32; 2]) -> Self {
        self.max_size = Some(max_size);
        self
    }

    /// Offset from screen center (e.g. `[0.0, -80.0]` for command palette).
    #[allow(dead_code)] // Reserved for migrating Command Palette dialog
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
/// `body` receives the window's inner `&mut Ui` and must render the title row
/// (including the close button) plus content.
pub fn modal(ui: &mut Ui, spec: &DialogSpec, body: impl FnOnce(&mut Ui)) -> bool {
    let ctx = ui.ctx();
    let screen_rect = ctx.viewport_rect();

    // Dark semi-transparent backdrop
    ui.painter().rect_filled(screen_rect, 0.0, overlay::backdrop());

    // Close on Escape
    let mut should_close = ctx.input(|i| i.key_pressed(egui::Key::Escape));

    // Close on backdrop click
    let backdrop_id = egui::Id::new(spec.id).with("backdrop");
    let backdrop = ui.interact(screen_rect, backdrop_id, egui::Sense::click());
    if backdrop.clicked() {
        should_close = true;
    }

    // Centered dialog using egui window for proper layout
    let mut window = egui::Window::new(spec.id)
        .anchor(Align2::CENTER_CENTER, spec.anchor_offset)
        .default_size(spec.default_size)
        .min_size(spec.min_size)
        .resizable(spec.resizable)
        .collapsible(false)
        .title_bar(false)
        .frame(
            egui::Frame::new()
                .fill(surface::BASE)
                .stroke(Stroke::new(STROKE_WIDTH, border::DEFAULT))
                .corner_radius(RADIUS_XL)
                .inner_margin(Margin::same(SPACE_XL as i8)),
        );
    if let Some(max) = spec.max_size {
        window = window.max_size(max);
    }

    let resp = window.show(ctx, |window_ui| {
        window_ui.set_min_width(spec.min_size[0] - 2.0 * SPACE_XL);
        body(window_ui);
    });

    if resp.is_none() {
        // Window was closed externally (e.g., collapsed away by the area system).
        should_close = true;
    }

    !should_close // returns `true` while the dialog should stay open
}

/// Renders the standard title row: heading on the left, X close button on the right.
///
/// Returns `true` if the close button was clicked.
pub fn title_row(ui: &mut Ui, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(TextRole::Heading.size())
                .color(text::PRIMARY),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(egui_phosphor::regular::X)
                .on_hover_text("Close (Esc)")
                .clicked()
            {
                close = true;
            }
        });
    });
    close
}
