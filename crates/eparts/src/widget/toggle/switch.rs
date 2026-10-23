use super::*;

pub struct Switch<'a> {
    pub(super) value: &'a mut bool,
    pub(super) label: Option<&'a str>,
    pub(super) label_side: Side,
    pub(super) tooltip: &'a str,
}

impl<'a> Switch<'a> {
    /// Create a switch bound to the given boolean.
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

impl<'a> egui::Widget for Switch<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        // Namespace the animation id under the parent `Ui` (see `Checkbox`).
        let id = animation_id(ui, self.value as *const bool);

        // Dimensions
        let s = crate::spatial(ui);
        let track_height = s.toggle.switch_track_height;
        let track_width = s.toggle.switch_track_width;
        let thumb_radius = s.toggle.switch_thumb_radius;
        let spacing = s.space_3;

        let font = TextRole::Body.font_id();
        let label_galley = self.label.map(|l| {
            ui.painter().layout_no_wrap(l.to_string(), font.clone(), t.palette.text.primary)
        });
        let label_size = label_galley.as_ref().map(|g| g.size()).unwrap_or(Vec2::ZERO);

        let total_size =
            Vec2::new(track_width + spacing + label_size.x, track_height.max(label_size.y));
        let (rect, response) = ui.allocate_exact_size(total_size, Sense::click());

        // Position track and label
        let (track_rect, label_rect) = match self.label_side {
            Side::Right => {
                let track = egui::Rect::from_center_size(
                    rect.center() - Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    Vec2::new(track_width, track_height),
                );
                let label_pos =
                    egui::pos2(track.right() + spacing, rect.center().y - label_size.y / 2.0);
                (track, egui::Rect::from_min_size(label_pos, label_size))
            },
            Side::Left => {
                let track = egui::Rect::from_center_size(
                    rect.center() + Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    Vec2::new(track_width, track_height),
                );
                let label_pos = egui::pos2(rect.min.x, rect.center().y - label_size.y / 2.0);
                (track, egui::Rect::from_min_size(label_pos, label_size))
            },
        };

        let track_center_y = track_rect.center().y;
        let left_x = track_rect.left() + thumb_radius;
        let right_x = track_rect.right() - thumb_radius;

        let transition = Transition {
            duration: NORMAL,
            easing: STANDARD,
        };

        // Animated thumb position
        let thumb_x =
            animate_lerp(ui.ctx(), id.with("thumb"), left_x, right_x, *self.value, transition);

        // Track crossfades between the off/on slot fills. Hover brightens the off
        // fill; disabled collapses both endpoints onto the disabled fill.
        let enabled = ui.is_enabled();
        let slot = toggle_slot(&t, *self.value, enabled, response.hovered());
        let off_bg = if !enabled {
            t.components.toggle.disabled.bg
        } else if response.hovered() {
            t.components.toggle.hover.bg
        } else {
            t.components.toggle.unchecked.bg
        };
        let on_bg = if enabled {
            t.components.toggle.checked.bg
        } else {
            t.components.toggle.disabled.bg
        };

        // Animated track color crossfade
        let track_color =
            animate_lerp(ui.ctx(), id.with("track"), off_bg, on_bg, *self.value, transition);

        // Draw track
        ui.painter().rect_filled(track_rect, track_height / 2.0, track_color);
        ui.painter().rect_stroke(
            track_rect,
            track_height / 2.0,
            Stroke::new(STROKE_WIDTH, slot.border),
            StrokeKind::Inside,
        );

        // Draw thumb
        let thumb_color = if enabled {
            t.components.toggle.thumb
        } else {
            t.components.toggle.disabled.fg
        };
        let thumb_center = Pos2::new(thumb_x, track_center_y);
        ui.painter().circle_filled(thumb_center, thumb_radius, thumb_color);
        ui.painter().circle_stroke(
            thumb_center,
            thumb_radius,
            Stroke::new(STROKE_WIDTH, slot.border),
        );

        // Draw label
        if let Some(galley) = label_galley {
            ui.painter().galley(label_rect.min, galley, t.palette.text.primary);
        }

        // Toggle on click
        if response.clicked() {
            *self.value = !*self.value;
        }

        let accessible_label = non_empty_or(self.label, "Switch");

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

// ── Tests ────────────────────────────────────────────────────────────
