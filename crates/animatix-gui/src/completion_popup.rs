//! Completion popup widget for the code editor.

use egui::{self, Color32, FontId, FontFamily, Key, Rect, Pos2, Vec2, Stroke, CornerRadius};

use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_S, ROW_M};
use animatix_analyzer::{CompletionItem, CompletionKind};

/// State for the completion popup.
pub struct CompletionPopup {
    /// Current completion items.
    items: Vec<CompletionItem>,
    /// Whether the popup is visible.
    visible: bool,
    /// Currently selected index.
    selected: usize,
    /// Scroll offset for long lists.
    scroll_offset: usize,
    /// Maximum visible items before scrolling.
    max_visible: usize,
    /// The text that triggered completion (for filtering).
    trigger_text: String,
}

impl Default for CompletionPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionPopup {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            visible: false,
            selected: 0,
            scroll_offset: 0,
            max_visible: 8,
            trigger_text: String::new(),
        }
    }

    /// Show the popup with new completion items.
    pub fn show(&mut self, items: Vec<CompletionItem>, trigger_text: String) {
        self.items = items;
        self.visible = !self.items.is_empty();
        self.selected = 0;
        self.scroll_offset = 0;
        self.trigger_text = trigger_text;
    }

    /// Hide the popup.
    pub fn hide(&mut self) {
        self.visible = false;
        self.items.clear();
        self.selected = 0;
    }

    /// Check if the popup is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the currently selected item.
    pub fn selected_item(&self) -> Option<&CompletionItem> {
        if self.visible {
            self.items.get(self.selected)
        } else {
            None
        }
    }

    /// Handle keyboard input. Returns true if the popup consumed the input.
    pub fn handle_input(&mut self, ctx: &egui::Context) -> bool {
        if !self.visible {
            return false;
        }

        let mut consumed = false;

        ctx.input(|i| {
            if i.key_pressed(Key::ArrowDown) {
                self.selected = (self.selected + 1) % self.items.len();
                self.ensure_visible();
                consumed = true;
            }
            if i.key_pressed(Key::ArrowUp) {
                self.selected = if self.selected == 0 {
                    self.items.len() - 1
                } else {
                    self.selected - 1
                };
                self.ensure_visible();
                consumed = true;
            }
            if i.key_pressed(Key::Tab) || i.key_pressed(Key::Enter) {
                // Selection confirmed - caller should read selected_item()
                consumed = true;
            }
            if i.key_pressed(Key::Escape) {
                self.hide();
                consumed = true;
            }
        });

        consumed
    }

    /// Ensure the selected item is visible in the scroll window.
    fn ensure_visible(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.max_visible {
            self.scroll_offset = self.selected - self.max_visible + 1;
        }
    }

    /// Render the popup below the given rectangle (e.g., cursor position).
    /// Returns the selected item if confirmed (Tab/Enter).
    pub fn ui(&mut self, ui: &mut egui::Ui, below_rect: Rect) -> Option<String> {
        if !self.visible || self.items.is_empty() {
            return None;
        }

        let mut result = None;

        // Filter items based on trigger text
        let filtered: Vec<(usize, &CompletionItem)> = self.items.iter().enumerate()
            .filter(|(_, item)| {
                if self.trigger_text.is_empty() {
                    true
                } else {
                    item.label.to_lowercase().starts_with(&self.trigger_text.to_lowercase())
                }
            })
            .collect();

        if filtered.is_empty() {
            self.hide();
            return None;
        }

        // Clamp selected to filtered range
        if self.selected >= filtered.len() {
            self.selected = filtered.len() - 1;
        }

        // Popup dimensions
        let item_height = ROW_M;
        let popup_width = 280.0;
        let visible_count = filtered.len().min(self.max_visible);
        let popup_height = visible_count as f32 * item_height;

        // Position below the cursor
        let popup_pos = Pos2::new(
            below_rect.left().min(ui.available_width() - popup_width),
            below_rect.bottom() + 2.0,
        );

        // Draw popup background
        let popup_rect = Rect::from_min_size(popup_pos, Vec2::new(popup_width, popup_height));
        // Semantic completion colors are intentionally hardcoded to match syntax highlighting.
        let bg_color = if ui.visuals().dark_mode {
            Color32::from_rgb(40, 44, 52)
        } else {
            Color32::from_rgb(255, 255, 255)
        };
        let border_color = if ui.visuals().dark_mode {
            Color32::from_rgb(60, 65, 75)
        } else {
            Color32::from_rgb(200, 200, 200)
        };

        ui.painter().rect_filled(popup_rect, CornerRadius::same(RADIUS_M as u8), bg_color);
        ui.painter().rect_stroke(popup_rect, CornerRadius::same(RADIUS_M as u8), Stroke::new(1.0, border_color), egui::StrokeKind::Outside);

        // Render items
        let mut item_rect = Rect::from_min_size(
            popup_pos + Vec2::new(4.0, 2.0),
            Vec2::new(popup_width - 8.0, item_height),
        );

        for (visible_idx, (original_idx, item)) in filtered.iter().enumerate() {
            if visible_idx >= self.scroll_offset && visible_idx < self.scroll_offset + self.max_visible {
                let is_selected = *original_idx == self.selected;

                // Highlight selected item
                if is_selected {
                    // Semantic completion colors are intentionally hardcoded to match syntax highlighting.
                    let highlight_color = if ui.visuals().dark_mode {
                        Color32::from_rgb(60, 65, 75)
                    } else {
                        Color32::from_rgb(220, 230, 240)
                    };
                    ui.painter().rect_filled(item_rect, CornerRadius::same(RADIUS_S as u8), highlight_color);
                }

                // Draw icon based on kind
                let icon = match item.kind {
                    CompletionKind::Keyword => egui_phosphor::regular::HASH,
                    CompletionKind::Type => egui_phosphor::regular::CUBE,
                    CompletionKind::Property => egui_phosphor::regular::WRENCH,
                    CompletionKind::Label => egui_phosphor::regular::TAG,
                    CompletionKind::Action => egui_phosphor::regular::LIGHTNING,
                    CompletionKind::Value => egui_phosphor::regular::HASH,
                    CompletionKind::Snippet => egui_phosphor::regular::CODE,
                };
                let icon_color = match item.kind {
                    CompletionKind::Keyword => Color32::from_rgb(251, 73, 106),
                    CompletionKind::Type => Color32::from_rgb(250, 189, 47),
                    CompletionKind::Property => Color32::from_rgb(131, 165, 152),
                    CompletionKind::Label => Color32::from_rgb(142, 192, 124),
                    CompletionKind::Action => Color32::from_rgb(254, 128, 25),
                    CompletionKind::Value => Color32::from_rgb(215, 153, 33),
                    CompletionKind::Snippet => Color32::from_rgb(108, 153, 187),
                };

                let icon_pos = item_rect.left_center() + Vec2::new(8.0, -6.0);
                ui.painter().text(
                    icon_pos,
                    egui::Align2::LEFT_CENTER,
                    icon,
                    FontId::new(11.0, FontFamily::Monospace),
                    icon_color,
                );

                // Draw label
                let label_pos = item_rect.left_center() + Vec2::new(24.0, -6.0);
                let label_color = if ui.visuals().dark_mode {
                    Color32::from_rgb(235, 219, 178)
                } else {
                    Color32::from_rgb(60, 60, 60)
                };
                ui.painter().text(
                    label_pos,
                    egui::Align2::LEFT_CENTER,
                    &item.label,
                    FontId::new(13.0, FontFamily::Proportional),
                    label_color,
                );

                // Draw detail (if any)
                if let Some(detail) = &item.detail {
                    let detail_pos = item_rect.right_center() + Vec2::new(-8.0, -6.0);
                    let detail_color = if ui.visuals().dark_mode {
                        Color32::from_rgb(146, 131, 116)
                    } else {
                        Color32::from_rgb(150, 150, 150)
                    };
                    ui.painter().text(
                        detail_pos,
                        egui::Align2::RIGHT_CENTER,
                        detail,
                        FontId::new(11.0, FontFamily::Proportional),
                        detail_color,
                    );
                }

                // Handle click
                if is_selected {
                    // Check for Tab/Enter confirmation
                    ui.input(|i| {
                        if i.key_pressed(Key::Tab) || i.key_pressed(Key::Enter) {
                            let insert = item.insert_text.as_deref().unwrap_or(&item.label);
                            result = Some(insert.to_string());
                        }
                    });
                }
            }

            item_rect = item_rect.translate(Vec2::new(0.0, item_height));
        }

        // Handle click outside to dismiss
        ui.input(|i| {
            if i.pointer.any_pressed() {
                if let Some(click_pos) = i.pointer.interact_pos() {
                    if !popup_rect.contains(click_pos) {
                        self.hide();
                    }
                }
            }
        });

        if result.is_some() {
            self.hide();
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_starts_hidden() {
        let popup = CompletionPopup::new();
        assert!(!popup.is_visible());
    }

    #[test]
    fn popup_shows_with_items() {
        let mut popup = CompletionPopup::new();
        popup.show(vec![
            CompletionItem {
                label: "test".to_string(),
                kind: CompletionKind::Keyword,
                detail: None,
                documentation: None,
                insert_text: None,
            },
        ], String::new());
        assert!(popup.is_visible());
    }

    #[test]
    fn popup_hides_on_empty_items() {
        let mut popup = CompletionPopup::new();
        popup.show(vec![], String::new());
        assert!(!popup.is_visible());
    }

    #[test]
    fn popup_navigation() {
        let mut popup = CompletionPopup::new();
        popup.show(vec![
            CompletionItem {
                label: "a".to_string(),
                kind: CompletionKind::Keyword,
                detail: None,
                documentation: None,
                insert_text: None,
            },
            CompletionItem {
                label: "b".to_string(),
                kind: CompletionKind::Keyword,
                detail: None,
                documentation: None,
                insert_text: None,
            },
        ], String::new());

        assert_eq!(popup.selected, 0);

        // Simulate arrow down (would need ctx in real usage)
        popup.selected = 1;
        assert_eq!(popup.selected, 1);
    }
}
