use egui::{CornerRadius, Margin, Rect, Response, Stroke, Ui, Vec2, WidgetText};

use crate::tokens::theme::theme;
use crate::tokens::spatial::component::{PILL_TAB_GAP, PILL_TAB_HEIGHT};
use crate::tokens::spatial::{
    RADIUS_M, RADIUS_S, ROW_S, SPACE_M, SPACE_S, SPACE_XL, STROKE_WIDTH,
};
use crate::tokens::typography::TextRole;

/// A styled container with our surface background, rounded corners,
/// and layered shadow for depth.
pub fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let t = theme(ui);
    egui::Frame::new()
        .fill(t.surface.surface)
        .corner_radius(CornerRadius::same(RADIUS_M as u8))
        .inner_margin(Margin::same(SPACE_M as i8))
        .shadow(egui::Shadow {
            offset: [0, 2],
            blur: 6,
            spread: 0,
            color: t.overlay.shadow_ambient,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

/// A section header that sticks to the top of the visible scroll area.
pub fn section_header(ui: &mut egui::Ui, icon: &str, title: &str, count: Option<usize>) {
    let t = theme(ui);
    let clip_rect = ui.clip_rect();
    let available = ui.available_width();

    let line_h = 2.0;
    let row_h = ROW_S;
    let header_height = SPACE_S + line_h + SPACE_S + row_h + SPACE_S;

    let (alloc_rect, _) =
        ui.allocate_exact_size(Vec2::new(available, header_height), egui::Sense::hover());

    let natural_y = alloc_rect.min.y;
    let paint_y = natural_y.max(clip_rect.min.y);
    let paint_x = alloc_rect.min.x;

    if paint_y > clip_rect.max.y {
        return;
    }

    let is_sticky = paint_y > natural_y;

    if is_sticky {
        let bg_rect =
            Rect::from_min_size(egui::pos2(paint_x, paint_y), Vec2::new(available, header_height));
        ui.painter().rect_filled(bg_rect, RADIUS_M, t.surface.surface);
        ui.painter().line_segment(
            [
                egui::pos2(paint_x, paint_y + header_height),
                egui::pos2(paint_x + available, paint_y + header_height),
            ],
            egui::Stroke::new(STROKE_WIDTH, t.border.default),
        );
    }

    let line_rect =
        Rect::from_min_size(egui::pos2(paint_x, paint_y + SPACE_S), Vec2::new(24.0, line_h));
    ui.painter().rect_filled(line_rect, RADIUS_S, t.accent.primary);

    let row_rect = Rect::from_min_size(
        egui::pos2(paint_x, paint_y + SPACE_S + line_h + SPACE_S),
        Vec2::new(available, row_h),
    );
    let baseline_y = row_rect.center().y;
    let mut cursor_x = row_rect.min.x;

    ui.painter().text(
        egui::pos2(cursor_x + 7.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        icon,
        TextRole::BodyS.font_id(),
        t.text.muted,
    );
    cursor_x += 18.0;

    ui.painter().text(
        egui::pos2(cursor_x, baseline_y),
        egui::Align2::LEFT_CENTER,
        title,
        TextRole::Micro.font_id(),
        t.text.muted,
    );

    if let Some(n) = count {
        ui.painter().text(
            egui::pos2(row_rect.max.x - SPACE_S, baseline_y),
            egui::Align2::RIGHT_CENTER,
            n.to_string(),
            TextRole::Micro.font_id(),
            t.text.muted,
        );
    }
}

/// Standard icon size for empty-state placeholders (px).
pub const EMPTY_STATE_ICON_SIZE: f32 = 28.0;

/// Centered empty-state placeholder with icon, title, and subtitle.
pub fn empty_state(ui: &mut egui::Ui, icon: &str, title: &str, subtitle: &str) {
    let t = theme(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(SPACE_XL * 3.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(icon).size(EMPTY_STATE_ICON_SIZE).color(t.text.muted),
            )
            .selectable(false),
        );
        ui.add_space(SPACE_M);
        ui.add(
            egui::Label::new(
                egui::RichText::new(title).size(TextRole::Title.size()).color(t.text.secondary),
            )
            .selectable(false),
        );
        ui.add_space(SPACE_S);
        ui.add(
            egui::Label::new(
                egui::RichText::new(subtitle).size(TextRole::Body.size()).color(t.text.muted),
            )
            .selectable(false),
        );
    });
}

/// Wraps native egui widgets in our themed input frame.
#[allow(dead_code)] // Reserved for use by future component widgets that don't need sized fields
pub fn field(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) -> Response {
    field_sized(ui, None, add_contents)
}

/// Same as [`field`], but with an explicit desired width.
pub fn field_sized(
    ui: &mut egui::Ui,
    desired_width: Option<f32>,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> Response {
    let t = theme(ui);
    let frame = egui::Frame::new()
        .fill(t.surface.widget)
        .corner_radius(CornerRadius::same(RADIUS_M as u8))
        .inner_margin(Margin::symmetric(SPACE_S as i8, SPACE_S as i8));

    let response = frame.show(ui, |ui| {
        if let Some(w) = desired_width {
            ui.set_width(w);
            ui.set_min_width(w);
        } else {
            ui.set_width(ui.available_width());
        }
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
            |ui| {
                add_contents(ui);
                let remaining = ui.available_width();
                if remaining > 0.0 {
                    ui.add_space(remaining);
                }
            },
        )
    });

    let is_hovered = ui.rect_contains_pointer(response.response.rect);
    let stroke = if is_hovered {
        egui::Stroke::new(STROKE_WIDTH, t.border.strong)
    } else {
        egui::Stroke::new(STROKE_WIDTH, t.border.default)
    };
    ui.painter().rect_stroke(
        response.response.rect,
        CornerRadius::same(RADIUS_M as u8),
        stroke,
        egui::StrokeKind::Inside,
    );

    response.response
}

/// Renders a label-left / input-right row with consistent alignment.
pub fn labeled_row(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    input_width: f32,
    add_input: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let remaining = ui.available_width();
        let frame_width = input_width + 2.0 * SPACE_S;
        if remaining > frame_width {
            ui.add_space(remaining - frame_width);
        }
        field_sized(ui, Some(input_width), add_input);
    });
}

/// Reusable pill-style segmented tab bar.
pub fn pill_tab_bar<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    active_tab: T,
    tabs: &[(T, &'static str, &'static str)],
) -> Option<T> {
    let t = theme(ui);
    let available = ui.available_width();
    let tab_h = PILL_TAB_HEIGHT;
    let gap = PILL_TAB_GAP;
    let tab_w = (available - gap * (tabs.len().saturating_sub(1)) as f32) / tabs.len() as f32;

    let bar_rect = ui.allocate_exact_size(Vec2::new(available, tab_h), egui::Sense::hover()).0;
    ui.painter().rect_filled(bar_rect, RADIUS_M, t.surface.widget);

    let mut clicked_tab = None;

    for (idx, (tab, icon, label)) in tabs.iter().enumerate() {
        let is_active = active_tab == *tab;
        let x = bar_rect.min.x + idx as f32 * (tab_w + gap);
        let tab_rect =
            egui::Rect::from_min_size(egui::pos2(x, bar_rect.min.y), Vec2::new(tab_w, tab_h));

        let response = ui.interact(tab_rect, ui.id().with(("pill_tab", idx)), egui::Sense::click());

        // Draw pill background
        let pill = tab_rect.shrink2(Vec2::new(2.0, 2.0));
        if is_active {
            ui.painter().rect_filled(pill, RADIUS_M, t.surface.surface);
            ui.painter().rect_stroke(
                pill,
                RADIUS_M,
                Stroke::new(STROKE_WIDTH, t.border.strong),
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() {
            ui.painter().rect_filled(pill, RADIUS_M, t.surface.hover);
        }

        let text_color = if is_active {
            t.text.primary
        } else if response.hovered() {
            t.text.secondary
        } else {
            t.text.muted
        };
        let font_id = TextRole::BodyS.font_id();
        let full_text = format!("{}  {}", icon, label);
        let galley = ui.painter().layout_no_wrap(full_text.clone(), font_id.clone(), text_color);
        let show_label = galley.size().x + SPACE_XL <= tab_w;
        let display_text = if show_label {
            full_text
        } else {
            icon.to_string()
        };
        ui.painter().text(
            tab_rect.center(),
            egui::Align2::CENTER_CENTER,
            display_text,
            font_id,
            text_color,
        );

    if response.clicked() {
        clicked_tab = Some(*tab);
    }
}

    clicked_tab
}

// ═══════════════════════════════════════════════════════════════════════════
// Separator  (F1)
// ═══════════════════════════════════════════════════════════════════════════

/// Horizontal rule drawn with `border::DEFAULT` and standard stroke width.
pub fn separator(ui: &mut egui::Ui) {
    let t = theme(ui);
    let avail = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail, STROKE_WIDTH), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        egui::Stroke::new(STROKE_WIDTH, t.border.default),
    );
}

/// Vertical rule drawn with `border::DEFAULT` and standard stroke width.
pub fn separator_v(ui: &mut egui::Ui) {
    let t = theme(ui);
    let avail = ui.available_height();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(STROKE_WIDTH, avail), egui::Sense::hover());
    ui.painter().line_segment(
        [
            egui::pos2(rect.center().x, rect.min.y),
            egui::pos2(rect.center().x, rect.max.y),
        ],
        egui::Stroke::new(STROKE_WIDTH, t.border.default),
    );
}

/// A labeled separator: thin line — text — thin line, all themed.
pub fn separator_labeled(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) {
    let t = theme(ui);
    let label = label.into();
    let label_str = label.text().to_string();
    ui.horizontal(|ui| {
        let galley = ui.painter().layout_no_wrap(
            label_str.clone(),
            TextRole::BodyS.font_id(),
            t.text.muted,
        );
        let label_w = galley.size().x + SPACE_S * 2.0;
        let avail = ui.available_width();
        let line_h = STROKE_WIDTH;
        let line_fraction = (avail - label_w) / 2.0;

        // Left line
        let (left_rect, _) = ui.allocate_exact_size(
            Vec2::new(line_fraction.max(0.0), line_h),
            egui::Sense::hover(),
        );
        ui.painter().line_segment(
            [
                egui::pos2(left_rect.min.x, left_rect.center().y),
                egui::pos2(left_rect.max.x, left_rect.center().y),
            ],
            egui::Stroke::new(STROKE_WIDTH, t.border.default),
        );

        // Label
        let (label_rect, _) = ui.allocate_exact_size(
            Vec2::new(label_w, line_h + SPACE_S * 2.0),
            egui::Sense::hover(),
        );
        ui.painter().galley(
            egui::pos2(
                label_rect.center().x - galley.size().x / 2.0,
                label_rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            t.text.muted,
        );

        // Right line
        let (right_rect, _) = ui.allocate_exact_size(
            Vec2::new(line_fraction.max(0.0), line_h),
            egui::Sense::hover(),
        );
        ui.painter().line_segment(
            [
                egui::pos2(right_rect.min.x, right_rect.center().y),
                egui::pos2(right_rect.max.x, right_rect.center().y),
            ],
            egui::Stroke::new(STROKE_WIDTH, t.border.default),
        );
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// GroupBox  (F2)
// ═══════════════════════════════════════════════════════════════════════════

/// A titled, bordered container for grouping related inspector sections.
///
/// Paints a `border::DEFAULT` stroke around the whole container and places
/// the `title` in `text::SECONDARY` at the top-left, inside the border.
pub fn group_box(
    ui: &mut egui::Ui,
    title: impl Into<WidgetText>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let t = theme(ui);
    let title_text = title.into();
    let title_str = title_text.text().to_string();

    ui.vertical(|ui| {
        // Title row — sits inside the border area.
        ui.horizontal(|ui| {
            ui.add_space(SPACE_M);
            ui.label(
                egui::RichText::new(title_str)
                    .size(TextRole::Body.size())
                    .color(t.text.secondary),
            );
        });

        // Body container with a full border.
        egui::Frame::new()
            .fill(t.surface.surface)
            .corner_radius(CornerRadius::same(RADIUS_M as u8))
            .inner_margin(Margin::same(SPACE_M as i8))
            .stroke(egui::Stroke::new(STROKE_WIDTH, t.border.default))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add_contents(ui);
            });
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// StatusBar  (F5)
// ═══════════════════════════════════════════════════════════════════════════

/// A bottom status bar: themed `surface::PANEL` background with a top
/// `border::DEFAULT` stroke.  Content is split into a left-aligned and a
/// right-aligned segment.
///
/// ```ignore
/// status_bar(ui, |ui| {
///     ui.horizontal(|ui| {
///         ui.label("Ready");
///         ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
///             ui.label("Ln 12, Col 4");
///         });
///     });
/// });
/// ```
pub fn status_bar(
    ui: &mut Ui,
    build: impl FnOnce(&mut Ui),
) {
    let t = theme(ui);
    let h = ROW_S;
    let avail_w = ui.available_width();

    // Allocate and paint the bar background + top border.
    let (_bar_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, h), egui::Sense::hover());

    egui::Frame::new()
        .fill(t.surface.panel)
        .corner_radius(CornerRadius::same(0))
        .inner_margin(Margin::symmetric(SPACE_M as i8, 0))
        .stroke(egui::Stroke::new(STROKE_WIDTH, t.border.default))
        .show(ui, |ui| {
            build(ui);
        });
}

/// Convenience builder for a status bar with left- and right-aligned text items.
///
/// ```ignore
/// StatusBar::new()
///     .left("Ready")
///     .right("Ln 12, Col 4")
///     .show(ui);
/// ```
#[derive(Default)]
pub struct StatusBar {
    left_items: Vec<String>,
    right_items: Vec<String>,
}

impl StatusBar {
    /// Create an empty status bar builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a left-aligned text item.
    pub fn left(mut self, text: impl Into<String>) -> Self {
        self.left_items.push(text.into());
        self
    }

    /// Add a right-aligned text item.
    pub fn right(mut self, text: impl Into<String>) -> Self {
        self.right_items.push(text.into());
        self
    }

    /// Render the status bar into `ui`.
    pub fn show(self, ui: &mut Ui) {
        let t = theme(ui);
        let h = ROW_S;
        let avail_w = ui.available_width();

        // Allocate and paint the bar background + top border.
        let (_bar_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, h), egui::Sense::hover());

        egui::Frame::new()
            .fill(t.surface.panel)
            .corner_radius(CornerRadius::same(0))
            .inner_margin(Margin::symmetric(SPACE_M as i8, 0))
            .stroke(egui::Stroke::new(STROKE_WIDTH, t.border.default))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Left items
                    for item in &self.left_items {
                        ui.label(item.as_str());
                    }
                    // Spacer pushes right items to the far right.
                    ui.add_space(ui.available_width());
                    // Right items in a right-to-left sub-layout.
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            for item in &self.right_items {
                                ui.label(item.as_str());
                            }
                        });
                    });
                });
            });
    }
}
