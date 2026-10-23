use super::*;

pub struct Radio<'a, T> {
    pub(super) value: &'a mut T,
    pub(super) this_value: T,
    pub(super) label: Option<&'a str>,
    pub(super) label_side: Side,
    pub(super) tooltip: &'a str,
}

impl<'a, T: PartialEq + Clone + Hash> Radio<'a, T> {
    /// Create a radio button. It is selected when `*value == this_value`.
    pub fn new(value: &'a mut T, this_value: T) -> Self {
        Self {
            value,
            this_value,
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

impl<'a, T: PartialEq + Clone + Hash> egui::Widget for Radio<'a, T> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        let selected = *self.value == self.this_value;

        // Stable id derived from the value pointer + option identity, namespaced
        // under the parent `Ui` so it can't collide across panels.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (self.value as *const T as usize).hash(&mut hasher);
        self.this_value.hash(&mut hasher);
        let id = animation_id(ui, hasher.finish());

        // Layout
        let s = crate::spatial(ui);
        let outer_size = Vec2::splat(s.toggle.radio_size);
        let spacing = s.space_3;
        let font = TextRole::Body.font_id();
        let label_galley = self.label.map(|l| {
            ui.painter().layout_no_wrap(l.to_string(), font.clone(), t.palette.text.primary)
        });
        let label_size = label_galley.as_ref().map(|g| g.size()).unwrap_or(Vec2::ZERO);

        let total_size =
            Vec2::new(outer_size.x + spacing + label_size.x, outer_size.y.max(label_size.y));
        let (rect, response) = ui.allocate_exact_size(total_size, Sense::click());

        let (outer_rect, label_rect) = match self.label_side {
            Side::Right => {
                let outer = egui::Rect::from_center_size(
                    rect.center() - Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    outer_size,
                );
                let label_pos =
                    egui::pos2(outer.right() + spacing, rect.center().y - label_size.y / 2.0);
                (outer, egui::Rect::from_min_size(label_pos, label_size))
            },
            Side::Left => {
                let outer = egui::Rect::from_center_size(
                    rect.center() + Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    outer_size,
                );
                let label_pos = egui::pos2(rect.min.x, rect.center().y - label_size.y / 2.0);
                (outer, egui::Rect::from_min_size(label_pos, label_size))
            },
        };

        // Animated dot progress
        let transition = Transition {
            duration: NORMAL,
            easing: STANDARD,
        };
        let dot_t = animate_bool_eased(ui.ctx(), id.with("dot"), selected, transition);

        // Radio has no fill: the state slot's `border` carries the ring colour.
        let enabled = ui.is_enabled();
        let slot = toggle_slot(&t, selected, enabled, response.hovered());
        ui.painter().circle_stroke(
            outer_rect.center(),
            outer_size.x / 2.0,
            Stroke::new(STROKE_WIDTH, slot.border),
        );

        // Draw animated inner dot
        if dot_t > 0.01 {
            let dot_radius = (outer_size.x / 2.0 - STROKE_WIDTH) * 0.6 * dot_t;
            let mark = toggle_mark_color(&t, enabled);
            let dot_color = Color32::from_rgba_unmultiplied(
                mark.r(),
                mark.g(),
                mark.b(),
                (dot_t * 255.0).round() as u8,
            );
            ui.painter().circle_filled(outer_rect.center(), dot_radius, dot_color);
        }

        // Draw label
        if let Some(galley) = label_galley {
            ui.painter().galley(label_rect.min, galley, t.palette.text.primary);
        }

        // Set value on click
        if response.clicked() {
            *self.value = self.this_value.clone();
        }

        let accessible_label = non_empty_or(self.label, "Radio");

        // Tooltip + principle 3 cursor
        let response = finish_response(ui, response, self.tooltip);
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::RadioButton,
                ui.is_enabled(),
                selected,
                accessible_label.clone(),
            )
        });
        response
    }
}

// ── Switch ──────────────────────────────────────────────────────────
