//! `Collapsible` — an animated expand/collapse panel (accordion building block).
//!
//! Uses egui `Memory` keyed by `Id` to persist open/closed state across frames
//! (per the state-management contract).  Expand/collapse animation is driven by
//! `anim::animate_bool_eased` so the content crossfades in/out with the
//! standard cubic-bezier easing.
//!
//! A rotating chevron indicates state.  The widget stores nothing retained;
//! open state and animation progress live in egui Memory.
//!
//! ## Usage
//! ```ignore
//! CollapsibleSection::new(ui.id().with("my_section"), "Inspector")
//!     .default_open(true)
//!     .show(ui, |ui| {
//!         ui.label("Hidden content revealed when expanded");
//!     });
//! ```
//! `Accordion` manages a group of `CollapsibleSection` items where only one
//! is open at a time (mutually exclusive).

use egui::{CornerRadius, Id, Response, Sense, Ui, Vec2};

use crate::tokens::motion::Transition;
use crate::tokens::spatial::STROKE_WIDTH;
use crate::tokens::typography::TextRole;
use crate::widget::anim::animate_bool_eased;
use crate::widget::traits::Collapsible as CollapsibleTrait;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Animation duration (seconds) for expand/collapse.
const COLLAPSE_DURATION: f32 = 0.2;

// ── State keys ────────────────────────────────────────────────────────────────

const STATE_KEY: &str = "collapsible_state";
const PROGRESS_KEY: &str = "collapsible_progress";

// ── CollapsibleSection ─────────────────────────────────────────────────────────

/// A section header that expands/collapses its content with animation.
///
/// Open/closed state is persisted in egui Memory keyed by the widget's `Id`.
/// Use `.default_open(bool)` to set the initial state on first use.
#[derive(Clone, Debug)]
pub struct CollapsibleSection {
    id: Id,
    header: String,
    default_open: bool,
}

impl CollapsibleSection {
    /// Create a new collapsible section with the given id and header text.
    pub fn new(id: impl Into<Id>, header: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            header: header.into(),
            default_open: false,
        }
    }

    /// Set the initial open state (used when the section is first encountered).
    pub fn default_open(mut self, yes: bool) -> Self {
        self.default_open = yes;
        self
    }

    /// Render the collapsible section.
    pub fn show(&self, ui: &mut Ui, body_fn: impl FnOnce(&mut Ui)) -> Response {
        let t = crate::tokens::theme::theme(ui);
        let s = crate::spatial(ui);
        let progress_id = self.id.with(PROGRESS_KEY);
        let state_id = self.id.with(STATE_KEY);

        // Read current state (default_open on first use).
        let is_open: bool = ui.data(|d| d.get_temp::<bool>(state_id).unwrap_or(self.default_open));

        // Allocate a clickable header area.
        let header_h = TextRole::Body.size() + s.space_2 * 2.0;
        let avail_w = ui.available_width();
        let (header_rect, header_response) =
            ui.allocate_exact_size(Vec2::new(avail_w, header_h), Sense::click());

        // Hover background.
        if header_response.hovered() {
            ui.painter().rect_filled(header_rect, CornerRadius::same(0), t.surface.hover);
        }

        // Chevron.
        ui.painter().text(
            egui::pos2(header_rect.min.x + s.space_3, header_rect.center().y),
            egui::Align2::LEFT_CENTER,
            "\u{25B6}",
            TextRole::BodyS.font_id(),
            t.text.muted,
        );

        // Title text.
        ui.painter().text(
            egui::pos2(header_rect.min.x + s.space_3 + s.space_2, header_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &self.header,
            TextRole::Body.font_id(),
            t.text.primary,
        );

        // Toggle on click.
        let new_open = if header_response.clicked() {
            !is_open
        } else {
            is_open
        };
        ui.data_mut(|d| {
            d.insert_temp(state_id, new_open);
        });

        // Animate progress toward the target.
        let transition = Transition {
            duration: COLLAPSE_DURATION,
            easing: crate::tokens::motion::DECELERATE,
        };
        let ctx = ui.ctx().clone();
        let progress = animate_bool_eased(&ctx, progress_id, new_open, transition);

        // Persist progress.
        ui.data_mut(|d| {
            d.insert_temp(progress_id, progress);
        });

        let effective_open = progress > 0.01;

        // Separator below header.
        let sep_y = header_rect.max.y;
        ui.painter().line_segment(
            [
                egui::pos2(header_rect.min.x, sep_y),
                egui::pos2(header_rect.max.x, sep_y),
            ],
            egui::Stroke::new(STROKE_WIDTH, t.border.default),
        );

        // ── Body ────────────────────────────────────────────────────────────
        if effective_open {
            ui.vertical(|ui| {
                ui.add_space(s.space_2);
                let full_h = ui.available_height();
                let body_h = full_h * progress;
                if body_h > 0.5 {
                    ui.allocate_ui(Vec2::new(ui.available_width(), body_h), |ui| {
                        body_fn(ui);
                    });
                }
            });
        }

        header_response
    }
}

// ── Convenience alias ─────────────────────────────────────────────────────────

/// Alias for [`CollapsibleSection`].
pub use CollapsibleSection as Collapsible;

// ── Accordion ─────────────────────────────────────────────────────────────────

/// Manages a group of `CollapsibleSection` items where only one is open at a time.
///
/// The open-section index is stored in egui Memory keyed by `id`.
/// Sections are rendered one at a time via `section()`.
#[derive(Clone, Debug)]
pub struct Accordion {
    id: Id,
    default_open: Option<usize>,
}

impl Accordion {
    /// Create a new accordion.
    ///
    /// `default_open` sets which index starts open (`None` = all collapsed).
    pub fn new(id: impl Into<Id>, default_open: Option<usize>) -> Self {
        Self {
            id: id.into(),
            default_open,
        }
    }

    /// Render one section of the accordion.
    ///
    /// Call this once per section in your UI loop.  Only one section can be
    /// open at a time; clicking a section closes the previously open one.
    pub fn section(
        &self,
        ui: &mut Ui,
        index: usize,
        title: impl AsRef<str>,
        body_fn: impl FnOnce(&mut Ui),
    ) -> Response {
        let section_id = self.id.with(("section", index));

        // Check if this section should be open (either explicitly set, or if it's the
        // default-open index and no section has been clicked yet).
        let is_open: bool = ui.data(|d| {
            d.get_temp::<bool>(section_id.with(STATE_KEY))
                .unwrap_or(index == self.default_open.unwrap_or(0))
        });

        let section = CollapsibleSection::new(section_id, title.as_ref()).default_open(is_open);

        let response = section.show(ui, body_fn);

        // On click, close all other sections by recording the open index.
        if response.clicked() {
            ui.data_mut(|d| {
                d.insert_temp(self.id.with("open_idx"), index);
            });
        }

        response
    }

    /// Return the index of the currently open section, if any.
    pub fn open_index(&self, ui: &Ui) -> Option<usize> {
        ui.data(|d| d.get_temp::<usize>(self.id.with("open_idx")))
    }
}

// ── Collapsible trait impl ────────────────────────────────────────────────────

impl CollapsibleTrait for CollapsibleSection {
    fn collapsed(mut self, yes: bool) -> Self {
        self.default_open = !yes;
        self
    }

    fn is_collapsed(&self) -> bool {
        !self.default_open
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsible_section_new_has_correct_header() {
        let c = CollapsibleSection::new("test_id", "Inspector");
        assert_eq!(c.header, "Inspector");
        assert!(!c.default_open);
    }

    #[test]
    fn collapsible_section_default_open() {
        let c = CollapsibleSection::new("test_id", "Section").default_open(true);
        assert!(c.default_open);
    }

    #[test]
    fn collapsible_section_builder_chaining() {
        let c = CollapsibleSection::new("test_id", "Section")
            .default_open(true)
            .default_open(false);
        assert!(!c.default_open);
    }

    #[test]
    fn accordion_new_defaults() {
        let a = Accordion::new("test_accordion", None);
        assert_eq!(a.default_open, None);
    }

    #[test]
    fn collapsible_trait_collapsed() {
        let c = CollapsibleSection::new("test_id", "Section");
        let c2 = c.collapsed(true);
        assert!(c2.is_collapsed());
    }
}
