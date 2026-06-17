//! Unified context menu component for the Animatix GUI.
//!
//! Provides consistent theming and behavior for all right-click / floating menus.
//!
//! # Usage
//!
//! ## Inside egui's `context_menu()`:
//! ```ignore
//! response.context_menu(|ui| {
//!     let entries = vec![
//!         MenuEntry::item_with_icon(egui_phosphor::regular::COPY, "Copy"),
//!         MenuEntry::item_with_icon(egui_phosphor::regular::SCISSORS, "Cut"),
//!         MenuEntry::Separator,
//!     ];
//!     if let Some(idx) = render_menu(ui, &entries) {
//!         match idx { ... }
//!     }
//! });
//! ```
//!
//! ## As a floating menu (e.g. preview canvas):
//! ```ignore
//! let entries = vec![MenuEntry::item_with_icon(egui_phosphor::regular::EYE, "Actor 1")];
//! let (clicked, menu_rect) = render_floating_menu(ctx, id, pos, &entries);
//! ```

use egui::{Align2, Color32, CornerRadius, Id, Margin, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::design_tokens::semantic::{accent, border, overlay, surface, text};
use crate::app::design_tokens::spatial::{
    menu as menu_spatial, ROW_M, ROW_S, SPACE_L, SPACE_M, SPACE_S, STROKE_WIDTH,
};
use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_S};
use crate::app::design_tokens::typography::{TextRole};

// ─── Constants ──────────────────────────────────────────────────────────────

const MENU_ITEM_HEIGHT: f32 = ROW_M; // 24.0
const MENU_ICON_GAP: f32 = SPACE_S;  // 4.0
const MENU_SHORTCUT_GAP: f32 = SPACE_L; // 8.0

// ─── Data Types ─────────────────────────────────────────────────────────────

/// A single entry in a context menu.
#[derive(Clone, Debug)]
pub enum MenuEntry {
    /// A clickable menu item.
    Item {
        icon: Option<&'static str>,
        label: String,
        shortcut: Option<String>,
        /// When true, the item is rendered with a checkmark and accent bg.
        checked: bool,
        /// When false, the item is grayed out and non-interactive.
        enabled: bool,
    },
    /// A non-interactive section header (muted, small).
    Header(String),
    /// A horizontal separator line.
    Separator,
}

impl MenuEntry {
    /// Create a menu item with an icon.
    pub fn item_with_icon(icon: &'static str, label: impl Into<String>) -> Self {
        Self::Item {
            icon: Some(icon),
            label: label.into(),
            shortcut: None,
            checked: false,
            enabled: true,
        }
    }

    /// Create a section header.
    pub fn header(label: impl Into<String>) -> Self {
        Self::Header(label.into())
    }

    /// Create a separator.
    pub fn separator() -> Self {
        Self::Separator
    }
}

/// Response from rendering a single menu item.
pub struct MenuItemResponse {
    pub clicked: bool,
}

// ─── Layout analysis ────────────────────────────────────────────────────────

/// How much left padding a menu needs based on which columns are used.
struct MenuLayout {
    check_col: bool,
    icon_col: bool,
    text_left: f32,
}

impl MenuLayout {
    fn from_entries(entries: &[MenuEntry]) -> Self {
        let mut check_col = false;
        let mut icon_col = false;
        for entry in entries {
            if let MenuEntry::Item { checked, icon, .. } = entry {
                if *checked && icon.is_none() {
                    check_col = true;
                }
                if icon.is_some() {
                    icon_col = true;
                }
            }
        }
        let mut text_left = SPACE_M;
        if check_col {
            text_left += menu_spatial::CHECK_WIDTH + MENU_ICON_GAP;
        }
        if icon_col {
            text_left += menu_spatial::ICON_WIDTH + MENU_ICON_GAP;
        }
        Self {
            check_col,
            icon_col,
            text_left,
        }
    }
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Render a context menu inside an existing UI container (e.g. within
/// `response.context_menu(|ui| { ... })`).
///
/// Returns the index of the clicked item, or `None` if nothing was clicked.
pub fn render_menu(ui: &mut Ui, entries: &[MenuEntry]) -> Option<usize> {
    ui.set_min_width(menu_spatial::MIN_WIDTH);

    let layout = MenuLayout::from_entries(entries);
    let mut clicked_index = None;
    let mut content_width = menu_spatial::MIN_WIDTH;

    // First pass: measure content width
    for entry in entries {
        if let MenuEntry::Item { label, shortcut, .. } = entry {
            let mut needed = layout.text_left;
            // Label
            let label_galley = ui.painter().layout(
                label.to_string(),
                TextRole::Body.font_id(),
                Color32::PLACEHOLDER,
                f32::INFINITY,
            );
            needed += label_galley.size().x;
            // Shortcut
            if let Some(sc) = shortcut {
                let sc_galley = ui.painter().layout(
                    sc.to_string(),
                    TextRole::BodyS.font_id(),
                    Color32::PLACEHOLDER,
                    f32::INFINITY,
                );
                needed += MENU_SHORTCUT_GAP + sc_galley.size().x;
            }
            content_width = content_width.max(needed + SPACE_M * 2.0);
        }
    }

    for (i, entry) in entries.iter().enumerate() {
        match entry {
            MenuEntry::Item {
                icon,
                label,
                shortcut,
                checked,
                enabled,
            } => {
                let response = render_menu_item(
                    ui,
                    *icon,
                    label,
                    shortcut.as_deref(),
                    *checked,
                    *enabled,
                    content_width,
                    &layout,
                );
                if response.clicked && clicked_index.is_none() {
                    clicked_index = Some(i);
                }
            }
            MenuEntry::Header(text) => {
                render_menu_header(ui, text, content_width);
            }
            MenuEntry::Separator => {
                render_menu_separator(ui, content_width);
            }
        }
    }

    clicked_index
}

/// Render a floating context menu at a screen position.
///
/// Returns `(clicked_index, menu_rect)`. The caller is responsible for
/// managing open/close state and outside-click detection.
pub fn render_floating_menu(
    ctx: &egui::Context,
    id: Id,
    pos: Pos2,
    entries: &[MenuEntry],
) -> (Option<usize>, Rect) {
    let area = egui::Area::new(id)
        .fixed_pos(pos)
        .order(egui::Order::Foreground);

    let inner = area.show(ctx, |ui| {
        menu_frame().show(ui, |ui| {
            render_menu(ui, entries)
        }).inner
    });

    (inner.inner, inner.response.rect)
}

// ─── Internals ──────────────────────────────────────────────────────────────

fn menu_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(surface::SURFACE)
        .stroke(Stroke::new(STROKE_WIDTH, border::DEFAULT))
        .corner_radius(CornerRadius::same(RADIUS_M as u8))
        .inner_margin(Margin::same(SPACE_S as i8))
        .shadow(egui::Shadow {
            offset: [0, menu_spatial::SHADOW_OFFSET_Y],
            blur: menu_spatial::SHADOW_BLUR as u8,
            spread: 0,
            color: overlay::shadow_direct(),
        })
}

fn render_menu_item(
    ui: &mut Ui,
    icon: Option<&'static str>,
    label: &str,
    shortcut: Option<&str>,
    checked: bool,
    enabled: bool,
    content_width: f32,
    layout: &MenuLayout,
) -> MenuItemResponse {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(content_width, MENU_ITEM_HEIGHT),
        if enabled { Sense::click() } else { Sense::hover() },
    );

    // ── Background ──
    let bg = if !enabled {
        Color32::TRANSPARENT
    } else if checked {
        accent::PRIMARY
    } else if response.hovered() {
        surface::HOVER
    } else {
        Color32::TRANSPARENT
    };

    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, RADIUS_S, bg);
    }

    let baseline_y = rect.center().y;
    let mut cursor_x = rect.min.x + SPACE_M;

    // ── Checkmark column ──
    if layout.check_col {
        if checked && icon.is_none() {
            ui.painter().text(
                egui::pos2(cursor_x + menu_spatial::CHECK_WIDTH / 2.0, baseline_y),
                Align2::CENTER_CENTER,
                egui_phosphor::regular::CHECK,
                TextRole::BodyS.font_id(),
                text::PRIMARY,
            );
        }
        cursor_x += menu_spatial::CHECK_WIDTH + MENU_ICON_GAP;
    }

    // ── Icon column ──
    if layout.icon_col {
        if let Some(icon_str) = icon {
            let icon_color = if enabled {
                if checked || response.hovered() {
                    text::PRIMARY
                } else {
                    text::SECONDARY
                }
            } else {
                text::DISABLED
            };
            ui.painter().text(
                egui::pos2(cursor_x + menu_spatial::ICON_WIDTH / 2.0, baseline_y),
                Align2::CENTER_CENTER,
                icon_str,
                TextRole::BodyS.font_id(),
                icon_color,
            );
        }
        cursor_x += menu_spatial::ICON_WIDTH + MENU_ICON_GAP;
    }

    // ── Label ──
    let label_color = if !enabled {
        text::DISABLED
    } else if checked || response.hovered() {
        text::PRIMARY
    } else {
        text::SECONDARY
    };

    ui.painter().text(
        egui::pos2(cursor_x, baseline_y),
        Align2::LEFT_CENTER,
        label,
        TextRole::Body.font_id(),
        label_color,
    );

    // ── Shortcut (right-aligned) ──
    if let Some(sc) = shortcut {
        let shortcut_color = if !enabled {
            text::DISABLED
        } else {
            text::MUTED
        };
        ui.painter().text(
            egui::pos2(rect.max.x - SPACE_M, baseline_y),
            Align2::RIGHT_CENTER,
            sc,
            TextRole::BodyS.font_id(),
            shortcut_color,
        );
    }

    MenuItemResponse {
        clicked: enabled && response.clicked(),
    }
}

fn render_menu_header(ui: &mut Ui, text: &str, content_width: f32) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(content_width, ROW_S),
        Sense::hover(),
    );

    ui.painter().text(
        egui::pos2(rect.min.x + SPACE_M, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        TextRole::Micro.font_id(),
        text::MUTED,
    );
}

fn render_menu_separator(ui: &mut Ui, content_width: f32) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(content_width, SPACE_M + 1.0),
        Sense::hover(),
    );

    let y = rect.center().y;
    ui.painter().line_segment(
        [
            egui::pos2(rect.min.x + SPACE_M, y),
            egui::pos2(rect.max.x - SPACE_M, y),
        ],
        Stroke::new(STROKE_WIDTH, border::DEFAULT),
    );
}
