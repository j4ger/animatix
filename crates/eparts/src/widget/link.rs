use egui::{Response, Sense};

use crate::tokens::typography::TextRole;

/// A themed hyperlink-style widget.
///
/// This is the **only** widget that uses `CursorIcon::PointingHand`.
///
/// # Example
/// ```ignore
/// ui.add(Link::new("Docs").url(Some("https://example.com".into())));
/// ```
pub struct Link {
    text: String,
    url: Option<String>,
}

impl Link {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            url: None,
        }
    }

    /// Set the URL to open when the link is clicked.
    pub fn url(mut self, url: Option<String>) -> Self {
        self.url = url;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> Response {
        let t = crate::theme(ui);
        let color = t.accent.primary;
        let hover_color = t.accent.primary_hover;

        let label = egui::Label::new(
            egui::RichText::new(self.text.clone())
                .color(color)
                .font(TextRole::BodyS.font_id()),
        )
        .sense(Sense::click());

        let response = ui.add(label);
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

        if response.hovered() {
            ui.painter().hline(
                response.rect.x_range(),
                response.rect.bottom(),
                egui::Stroke::new(1.0, hover_color),
            );
        }

        if response.clicked() {
            if let Some(url) = self.url {
                ui.ctx().open_url(egui::OpenUrl::same_tab(url));
            }
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sets_text_and_url() {
        let link = Link::new("hello").url(Some("https://example.com".into()));
        assert_eq!(link.text, "hello");
        assert_eq!(link.url, Some("https://example.com".into()));
    }

    #[test]
    fn builder_without_url() {
        let link = Link::new("no url");
        assert!(link.url.is_none());
    }
}
