use super::*;

pub struct Checkbox<'a> {
    pub(super) value: &'a mut bool,
    pub(super) label: Option<&'a str>,
    pub(super) label_side: Side,
    pub(super) tooltip: &'a str,
}

impl<'a> Checkbox<'a> {
    /// Create a checkbox bound to the given boolean.
    pub fn new(value: &'a mut bool) -> Self {
        Self {
            value,
            label: None,
            label_side: Side::Right,
            tooltip: "",
        }
    }

    /// Set the label text.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Set which side the label appears on.
    pub fn label_side(mut self, side: Side) -> Self {
        self.label_side = side;
        self
    }

    /// Set a tooltip shown on hover.
    pub fn tooltip(mut self, tip: &'a str) -> Self {
        self.tooltip = tip;
        self
    }
}

impl<'a> egui::Widget for Checkbox<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        // Namespace the animation id under the parent `Ui` so two checkboxes in
        // different panels (or frames) can't collide on a recycled address.
        let id = animation_id(ui, self.value as *const bool);

        // Layout calculations
        let s = crate::spatial(ui);
        let box_size = Vec2::splat(s.row_xs);
        let spacing = s.space_3;
        let font = TextRole::Body.font_id();
        let label_galley = self.label.map(|l| {
            ui.painter().layout_no_wrap(l.to_string(), font.clone(), t.palette.text.primary)
        });
        let label_size = label_galley.as_ref().map(|g| g.size()).unwrap_or(Vec2::ZERO);

        let total_size =
            Vec2::new(box_size.x + spacing + label_size.x, box_size.y.max(label_size.y));
        let (rect, response) = ui.allocate_exact_size(total_size, Sense::click());

        // Position checkbox and label
        let (checkbox_rect, label_rect) = match self.label_side {
            Side::Right => {
                let cb = egui::Rect::from_center_size(
                    rect.center() - Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    box_size,
                );
                let label_pos =
                    egui::pos2(cb.right() + spacing, rect.center().y - label_size.y / 2.0);
                (cb, egui::Rect::from_min_size(label_pos, label_size))
            },
            Side::Left => {
                let cb = egui::Rect::from_center_size(
                    rect.center() + Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    box_size,
                );
                let label_pos = egui::pos2(rect.min.x, rect.center().y - label_size.y / 2.0);
                (cb, egui::Rect::from_min_size(label_pos, label_size))
            },
        };

        // Animated checkmark crossfade
        let transition = Transition {
            duration: NORMAL,
            easing: STANDARD,
        };
        let check_t = animate_bool_eased(ui.ctx(), id.with("check"), *self.value, transition);

        // Read the control's state slot from `theme.toggle`. Hover and enabled
        // state come from the response; a disabled control overrides everything.
        let enabled = ui.is_enabled();
        let slot = toggle_slot(&t, *self.value, enabled, response.hovered());

        // Draw checkbox box
        ui.painter().rect_filled(checkbox_rect, RADIUS_M, slot.bg);
        ui.painter().rect_stroke(
            checkbox_rect,
            RADIUS_M,
            Stroke::new(STROKE_WIDTH, slot.border),
            StrokeKind::Inside,
        );

        // Draw animated checkmark
        if check_t > 0.01 {
            let alpha = (check_t * 255.0).round() as u8;
            let scale = 0.5 + 0.5 * check_t;
            let center = checkbox_rect.center();
            let arm = checkbox_rect.width() * 0.28;

            let mark = toggle_mark_color(&t, enabled);
            let check_color = Color32::from_rgba_unmultiplied(mark.r(), mark.g(), mark.b(), alpha);

            let p1 = center + Vec2::new(-arm * 0.9 * scale, arm * 0.1 * scale);
            let p2 = center + Vec2::new(-arm * 0.1 * scale, arm * 0.8 * scale);
            let p3 = center + Vec2::new(arm * 0.9 * scale, -arm * 0.9 * scale);

            ui.painter()
                .line_segment([p1, p2], Stroke::new(STROKE_WIDTH_THICK, check_color));
            ui.painter()
                .line_segment([p2, p3], Stroke::new(STROKE_WIDTH_THICK, check_color));
        }

        // Draw label
        if let Some(galley) = label_galley {
            ui.painter().galley(label_rect.min, galley, t.palette.text.primary);
        }

        // Toggle on click
        if response.clicked() {
            *self.value = !*self.value;
        }

        let accessible_label = non_empty_or(self.label, "Checkbox");

        // Tooltip + principle 3 cursor
        let response = finish_response(ui, response, self.tooltip);
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::Checkbox,
                ui.is_enabled(),
                *self.value,
                accessible_label.clone(),
            )
        });
        response
    }
}

// ── Radio ───────────────────────────────────────────────────────────
