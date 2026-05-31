use egui::{CornerRadius, Margin, Rect, Response, RichText, Vec2};

use crate::app::design_tokens::*;

/// A styled container with our surface background, rounded corners,
/// and layered shadow for depth.
pub fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(BG_SURFACE)
        .corner_radius(CornerRadius::same(RADIUS_M as u8))
        .inner_margin(Margin::same(SPACE_L as i8))
        .shadow(egui::Shadow {
            offset: [0, 2],
            blur: 6,
            spread: 0,
            color: shadow_ambient(),
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

/// A section header that sticks to the top of the visible scroll area.
pub fn section_header(ui: &mut egui::Ui, icon: &str, title: &str, count: Option<usize>) {
    let clip_rect = ui.clip_rect();
    let available = ui.available_width();

    let line_h = 2.0;
    let row_h = ROW_S;
    let header_height = SPACE_M + line_h + SPACE_M + row_h + SPACE_M;

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
        let bg_rect = Rect::from_min_size(
            egui::pos2(paint_x, paint_y),
            Vec2::new(available, header_height),
        );
        ui.painter().rect_filled(bg_rect, RADIUS_M, BG_SURFACE);
        ui.painter().line_segment(
            [
                egui::pos2(paint_x, paint_y + header_height),
                egui::pos2(paint_x + available, paint_y + header_height),
            ],
            egui::Stroke::new(STROKE_WIDTH, BORDER),
        );
    }

    let line_rect = Rect::from_min_size(
        egui::pos2(paint_x, paint_y + SPACE_M),
        Vec2::new(24.0, line_h),
    );
    ui.painter().rect_filled(line_rect, RADIUS_S, ACCENT_BLUE);

    let row_rect = Rect::from_min_size(
        egui::pos2(paint_x, paint_y + SPACE_M + line_h + SPACE_M),
        Vec2::new(available, row_h),
    );
    let baseline_y = row_rect.center().y;
    let mut cursor_x = row_rect.min.x;

    ui.painter().text(
        egui::pos2(cursor_x + 7.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        TEXT_MUTED,
    );
    cursor_x += 18.0;

    ui.painter().text(
        egui::pos2(cursor_x, baseline_y),
        egui::Align2::LEFT_CENTER,
        title.to_uppercase(),
        egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
        TEXT_MUTED,
    );

    if let Some(n) = count {
        ui.painter().text(
            egui::pos2(row_rect.max.x - SPACE_S, baseline_y),
            egui::Align2::RIGHT_CENTER,
            n.to_string(),
            egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
    }
}

/// Standard icon size for empty-state placeholders (px).
pub const EMPTY_STATE_ICON_SIZE: f32 = 28.0;

/// Centered empty-state placeholder with icon, title, and subtitle.
pub fn empty_state(ui: &mut egui::Ui, icon: &str, title: &str, subtitle: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(SPACE_XL * 3.0);
        ui.add(
            egui::Label::new(egui::RichText::new(icon).size(EMPTY_STATE_ICON_SIZE).color(TEXT_MUTED))
                .selectable(false),
        );
        ui.add_space(SPACE_M);
        ui.add(
            egui::Label::new(
                egui::RichText::new(title).size(FONT_SIZE_L).color(TEXT_SECONDARY),
            )
            .selectable(false),
        );
        ui.add_space(SPACE_S);
        ui.add(
            egui::Label::new(
                egui::RichText::new(subtitle).size(FONT_SIZE_M).color(TEXT_MUTED),
            )
            .selectable(false),
        );
    });
}

/// Wraps native egui widgets in our themed input frame.
#[allow(dead_code)]
pub fn field(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) -> Response {
    field_sized(ui, None, add_contents)
}

/// Same as [`field`], but with an explicit desired width.
pub fn field_sized(
    ui: &mut egui::Ui,
    desired_width: Option<f32>,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> Response {
    let frame = egui::Frame::new()
        .fill(BG_WIDGET)
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
        egui::Stroke::new(STROKE_WIDTH, BORDER_HOVER)
    } else {
        egui::Stroke::new(STROKE_WIDTH, BORDER)
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
    let available = ui.available_width();
    let tab_h = PILL_TAB_HEIGHT;
    let gap = PILL_TAB_GAP;
    let tab_w = (available - gap * (tabs.len().saturating_sub(1)) as f32) / tabs.len() as f32;

    let bar_rect = ui
        .allocate_exact_size(Vec2::new(available, tab_h), egui::Sense::hover())
        .0;
    ui.painter().rect_filled(bar_rect, RADIUS_M, BG_WIDGET);

    let mut clicked_tab = None;

    for (idx, (tab, icon, label)) in tabs.iter().enumerate() {
        let is_active = active_tab == *tab;
        let x = bar_rect.min.x + idx as f32 * (tab_w + gap);
        let tab_rect =
            egui::Rect::from_min_size(egui::pos2(x, bar_rect.min.y), Vec2::new(tab_w, tab_h));

        let response = ui.interact(tab_rect, ui.id().with(("pill_tab", idx)), egui::Sense::click());

        if is_active {
            let pill = tab_rect.shrink2(Vec2::new(2.0, 2.0));
            ui.painter().rect_filled(pill, RADIUS_M, BG_SURFACE);
        } else if response.hovered() {
            let hover_bg = BG_HOVER;
            let pill = tab_rect.shrink2(Vec2::new(2.0, 2.0));
            ui.painter().rect_filled(pill, RADIUS_M, hover_bg);
        }

        let text_color = if is_active { TEXT_PRIMARY } else { TEXT_MUTED };
        let font_id = egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional);
        let full_text = format!("{}  {}", icon, label);
        let galley = ui.painter().layout_no_wrap(full_text.clone(), font_id.clone(), text_color);
        let show_label = galley.size().x + SPACE_XL <= tab_w;
        let display_text = if show_label { full_text } else { icon.to_string() };
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
