//! TextRole helpers for concise RichText construction.
//!
//! Usage:
//! ```ignore
//! use eparts::widget::text::{rich, RichTextExt};
//! ui.label(rich(TextRole::Body, "Hello"));
//! ui.label("Hello".role(TextRole::Body));
//! ```

use crate::tokens::typography::TextRole;

/// Construct a RichText with a given TextRole and string content.
#[allow(dead_code)] // Available for future TextRole migration call sites
pub fn rich(role: TextRole, text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).font(role.font_id())
}

/// Extension trait for egui::RichText to set font via TextRole.
#[allow(dead_code)] // Available for future TextRole migration call sites
pub trait RichTextExt {
    fn role(self, role: TextRole) -> Self;
}

impl RichTextExt for egui::RichText {
    fn role(self, role: TextRole) -> Self {
        self.font(role.font_id())
    }
}
