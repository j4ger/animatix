//! Shared F-curve sampling, mapping, and painting.
//!
//! Both the Inspector's read-only graph editor and the bottom Curves editor
//! draw the same property curves, so the sampling math (per-property keyframe
//! times, easing-aware segment interpolation) and the time/value axis mapping
//! live here once. Callers own the surrounding frame: the Inspector simply
//! paints, while the Curves panel adds hit-testing, drag handles, and a
//! scrub ruler on top of the returned geometry.

use animatix::timeline::{
    ActorField, AnimationTrack, PROPERTY_REGISTRY, ValueType, allowed_property_indices,
    property_has_keyframes, property_keyframe_easing, property_keyframe_times, read_property_value,
};
use animatix_syntax::easing::Easing;
use egui::{Color32, Painter, Pos2, Rect, Stroke};
use eparts::Theme;

use crate::app::design_tokens::semantic::{canvas, curve};
use crate::app::design_tokens::spatial::STROKE_WIDTH;

/// Number of polyline segments used to approximate one eased keyframe span.
const SEGMENTS_PER_SPAN: usize = 20;

/// One graphed channel of one property.
///
/// A `Vec2` property yields two channels (`.X`, `.Y`); a `Color`/`Vec4`
/// property yields four (`.R`, `.G`, `.B`, `.A`). `channel` is the component
/// index within the property value, used when an edit writes a single
/// component back through [`PropertyValue`](animatix::timeline::PropertyValue).
#[derive(Debug, Clone)]
pub(crate) struct CurveChannel {
    pub label: String,
    pub color: Color32,
    /// Sampled keyframes as `(time_s, value)`.
    pub points: Vec<(f64, f32)>,
    /// Easing for the segment ending at the matching point index.
    pub segment_easing: Vec<Easing>,
    /// Storage field, for easing/keyframe lookups.
    pub field: ActorField,
    /// Canonical property name (e.g. `position`), the identity used by
    /// [`KeyframeId`](crate::app::document::timeline_diff::KeyframeId).
    pub property: &'static str,
    /// Component index within the property value.
    pub channel: usize,
}

/// Sample every animated channel of a track.
///
/// Properties with fewer than two keyframes are skipped: a single keyframe has
/// no curve to draw.
pub(crate) fn collect_curves(track: &AnimationTrack, theme: Theme) -> Vec<CurveChannel> {
    let mut curves: Vec<CurveChannel> = Vec::new();

    for &idx in &allowed_property_indices(track.kind) {
        let schema = &PROPERTY_REGISTRY[idx];
        if !property_has_keyframes(track, schema.field) {
            continue;
        }
        let kf_times = property_keyframe_times(track, schema.field);
        if kf_times.len() < 2 {
            continue;
        }

        let value_type = schema.value_type;
        let mut push = |label: String, color: Color32, channel: usize, points: Vec<(f64, f32)>| {
            if points.len() < 2 {
                return;
            }
            let segment_easing = kf_times
                .iter()
                .map(|time_ms| {
                    property_keyframe_easing(track, schema.field, *time_ms)
                        .unwrap_or(Easing::Linear)
                })
                .collect();
            curves.push(CurveChannel {
                label,
                color,
                points,
                segment_easing,
                field: schema.field,
                property: schema.name,
                channel,
            });
        };

        match schema.value_type {
            ValueType::F32 => {
                let mut points = Vec::new();
                for time_ms in &kf_times {
                    if let Some(animatix::timeline::PropertyValue::F32(v)) =
                        read_property_value(track, schema.field, *time_ms)
                    {
                        points.push((*time_ms as f64 / 1000.0, v));
                    }
                }
                push(schema.name.to_string(), channel_color(theme, value_type, 0), 0, points);
            },
            ValueType::Vec2 => {
                let mut x_points = Vec::new();
                let mut y_points = Vec::new();
                for time_ms in &kf_times {
                    if let Some(animatix::timeline::PropertyValue::Vec2(v)) =
                        read_property_value(track, schema.field, *time_ms)
                    {
                        x_points.push((*time_ms as f64 / 1000.0, v[0]));
                        y_points.push((*time_ms as f64 / 1000.0, v[1]));
                    }
                }
                push(
                    format!("{}.X", schema.name),
                    channel_color(theme, value_type, 0),
                    0,
                    x_points,
                );
                push(
                    format!("{}.Y", schema.name),
                    channel_color(theme, value_type, 1),
                    1,
                    y_points,
                );
            },
            ValueType::Vec4 | ValueType::Color => {
                let mut ch: [Vec<(f64, f32)>; 4] = Default::default();
                for time_ms in &kf_times {
                    let value = match read_property_value(track, schema.field, *time_ms) {
                        Some(animatix::timeline::PropertyValue::Vec4(v)) => Some(v),
                        Some(animatix::timeline::PropertyValue::Color(v)) => Some(v),
                        _ => None,
                    };
                    if let Some(v) = value {
                        for (i, slot) in ch.iter_mut().enumerate() {
                            slot.push((*time_ms as f64 / 1000.0, v[i]));
                        }
                    }
                }
                for (i, suffix) in ["R", "G", "B", "A"].iter().enumerate() {
                    push(
                        format!("{}.{}", schema.name, suffix),
                        channel_color(theme, value_type, i),
                        i,
                        std::mem::take(&mut ch[i]),
                    );
                }
            },
            _ => {},
        }
    }

    curves
}

/// Stable component color: index 0/1 use the canonical accent/error pairing,
/// remaining channels use the shared curve palette.
fn channel_color(theme: Theme, value_type: ValueType, channel: usize) -> Color32 {
    match channel {
        0 => {
            if matches!(value_type, ValueType::Vec2 | ValueType::Vec4 | ValueType::Color) {
                theme.status.error
            } else {
                theme.accent.primary
            }
        },
        1 => curve::GREEN,
        2 => curve::BLUE,
        _ => curve::GRAY,
    }
}

/// Read the active legend visibility for `curves`, defaulting every new label
/// to visible. Missing labels are inserted so toggles survive frames.
pub(crate) fn legend_visibility(
    ui: &egui::Ui,
    id: egui::Id,
    curves: &[CurveChannel],
) -> std::collections::HashMap<String, bool> {
    let mut visibility: std::collections::HashMap<String, bool> =
        ui.data(|d| d.get_temp(id).unwrap_or_default());
    for curve in curves {
        visibility.entry(curve.label.clone()).or_insert(true);
    }
    visibility
}

/// Draw the legend row of clickable channel toggles and persist the state.
///
/// Returns the rect the legend occupied.
pub(crate) fn draw_legend(
    ui: &egui::Ui,
    rect: Rect,
    curves: &[CurveChannel],
    visibility: &mut std::collections::HashMap<String, bool>,
    theme: Theme,
) {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let legend_height = 18.0f32;
    let mut legend_x = rect.min.x;
    for curve in curves {
        let is_visible = *visibility.get(&curve.label).unwrap_or(&true);
        let item_width = 50.0f32;
        let item_rect = Rect::from_min_size(
            egui::pos2(legend_x, rect.min.y),
            egui::Vec2::new(item_width, legend_height),
        );
        if ui.rect_contains_pointer(item_rect) {
            ui.painter().rect_filled(
                item_rect,
                crate::app::design_tokens::spatial::RADIUS_S,
                theme.surface.hover,
            );
        }
        let color_dot = if is_visible {
            curve.color
        } else {
            theme.text.disabled
        };
        ui.painter().circle_filled(
            egui::pos2(item_rect.min.x + 6.0, item_rect.center().y),
            3.0,
            color_dot,
        );
        ui.painter().text(
            egui::pos2(item_rect.min.x + 14.0, item_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &curve.label,
            crate::app::design_tokens::typography::TextRole::Micro.font_id(),
            if is_visible {
                theme.text.secondary
            } else {
                theme.text.disabled
            },
        );
        let item_response =
            ui.interact(item_rect, ui.id().with(("legend", &curve.label)), egui::Sense::click());
        if item_response.clicked() {
            visibility.insert(curve.label.clone(), !is_visible);
        }
        legend_x += item_width + sp.base.space_2;
    }
}

/// Time/value window used to map data onto a plot rect.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Axis {
    pub plot_rect: Rect,
    pub duration_s: f64,
    pub min_val: f32,
    pub max_val: f32,
}

impl Axis {
    /// Value window padded to a non-degenerate range.
    pub fn new(plot_rect: Rect, duration_s: f64, min_val: f32, max_val: f32) -> Self {
        let (min_val, max_val) = pad_range(min_val, max_val);
        Self {
            plot_rect,
            duration_s: duration_s.max(0.1),
            min_val,
            max_val,
        }
    }

    fn val_range(&self) -> f32 {
        (self.max_val - self.min_val).max(0.001)
    }

    /// X pixel for a time in seconds.
    pub fn x(&self, time_s: f64) -> f32 {
        let tx = (time_s / self.duration_s) as f32;
        egui::lerp(self.plot_rect.left()..=self.plot_rect.right(), tx.clamp(0.0, 1.0))
    }

    /// Y pixel for a value.
    pub fn y(&self, val: f32) -> f32 {
        let ty = 1.0 - ((val - self.min_val) / self.val_range()).clamp(0.0, 1.0);
        egui::lerp(self.plot_rect.top()..=self.plot_rect.bottom(), ty)
    }

    /// Screen position of a data point.
    pub fn point(&self, time_s: f64, val: f32) -> Pos2 {
        Pos2::new(self.x(time_s), self.y(val))
    }

    /// Value at a Y pixel, for vertical drags.
    ///
    /// Deliberately unclamped: dragging past the top of the plot must be able
    /// to raise a keyframe above the currently graphed maximum, otherwise the
    /// value window (which is derived from the data) would pin the edit.
    /// Callers that paint should clamp the resulting position for display.
    pub fn value_at_y(&self, y: f32) -> f32 {
        let t = (y - self.plot_rect.top()) / self.plot_rect.height().max(0.001);
        self.max_val - t * self.val_range()
    }

    /// Time in seconds at an X pixel, for scrub/drag hit-testing.
    pub fn time_at_x(&self, x: f32) -> f64 {
        let frac = ((x - self.plot_rect.left()) / self.plot_rect.width().max(0.001)).clamp(0.0, 1.0)
            as f64;
        frac * self.duration_s
    }
}

fn pad_range(min_val: f32, max_val: f32) -> (f32, f32) {
    if !min_val.is_finite() || !max_val.is_finite() {
        return (0.0, 1.0);
    }
    if (max_val - min_val).abs() < 0.001 {
        let mid = (min_val + max_val) / 2.0;
        let half = mid.abs().max(0.5);
        (mid - half, mid + half)
    } else {
        (min_val, max_val)
    }
}

/// Value window spanning every visible channel.
pub(crate) fn value_window(curves: &[&CurveChannel]) -> (f32, f32) {
    let mut min_val = f32::INFINITY;
    let mut max_val = f32::NEG_INFINITY;
    for curve in curves {
        for (_, v) in &curve.points {
            min_val = min_val.min(*v);
            max_val = max_val.max(*v);
        }
    }
    pad_range(min_val, max_val)
}

/// Draw the background grid and value labels.
pub(crate) fn draw_value_grid(painter: &Painter, axis: &Axis, theme: Theme) {
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        let y = egui::lerp(axis.plot_rect.top()..=axis.plot_rect.bottom(), t);
        painter.line_segment(
            [
                Pos2::new(axis.plot_rect.left(), y),
                Pos2::new(axis.plot_rect.right(), y),
            ],
            Stroke::new(STROKE_WIDTH, canvas::grid_line()),
        );
        let val_label = format!("{:.1}", axis.max_val - t * axis.val_range());
        painter.text(
            Pos2::new(axis.plot_rect.left() + 2.0, y),
            egui::Align2::LEFT_CENTER,
            val_label,
            crate::app::design_tokens::typography::TextRole::Micro.font_id(),
            theme.text.muted,
        );
    }
}

/// Paint one channel: eased polyline plus keyframe dots.
///
/// `highlight_time_s` draws the current-playhead ring; callers that need extra
/// affordances (selection, drag ghosts) draw them afterwards using
/// [`Axis::point`].
pub(crate) fn draw_curve(
    painter: &Painter,
    axis: &Axis,
    curve: &CurveChannel,
    current_time_s: f64,
    theme: Theme,
) {
    for i in 0..curve.points.len().saturating_sub(1) {
        let (t0, v0) = curve.points[i];
        let (t1, v1) = curve.points[i + 1];
        let easing = curve.segment_easing.get(i + 1).copied().unwrap_or(Easing::Linear);

        let mut prev = axis.point(t0, v0);
        for s in 1..=SEGMENTS_PER_SPAN {
            let progress = s as f32 / SEGMENTS_PER_SPAN as f32;
            let eased = animatix_syntax::easing::apply_easing(progress, easing);
            let time_s = t0 + (t1 - t0) * eased as f64;
            let val = v0 + (v1 - v0) * eased;
            let curr = axis.point(time_s, val);
            painter.line_segment([prev, curr], Stroke::new(2.0, curve.color));
            prev = curr;
        }
    }

    for (time_s, val) in &curve.points {
        let p = axis.point(*time_s, *val);
        let is_current = (*time_s - current_time_s).abs() < 0.05;
        let size = if is_current { 4.0 } else { 2.5 };
        painter.circle_filled(p, size, curve.color);
        if is_current {
            painter.circle_stroke(p, size + 2.0, Stroke::new(STROKE_WIDTH, theme.status.warning));
        }
    }
}

/// Draw the vertical playhead line across the plot.
pub(crate) fn draw_playhead(painter: &Painter, axis: &Axis, current_time_s: f64, theme: Theme) {
    let x = axis.x(current_time_s);
    if x >= axis.plot_rect.left() && x <= axis.plot_rect.right() {
        painter.line_segment(
            [
                Pos2::new(x, axis.plot_rect.top()),
                Pos2::new(x, axis.plot_rect.bottom()),
            ],
            Stroke::new(1.5, theme.status.warning),
        );
    }
}

/// Write `new_value` into component `channel` of a property value.
///
/// Returns `false` when the value's type or arity does not match, leaving it
/// untouched so the caller can report the mismatch instead of silently
/// dropping the edit.
pub(crate) fn set_channel(
    value: &mut animatix::timeline::PropertyValue,
    channel: usize,
    new_value: f32,
) -> bool {
    use animatix::timeline::PropertyValue;
    match (value, channel) {
        (PropertyValue::F32(v), 0) => {
            *v = new_value;
            true
        },
        (PropertyValue::Vec2(v), c) if c < 2 => {
            v[c] = new_value;
            true
        },
        (PropertyValue::Vec4(v), c) if c < 4 => {
            v[c] = new_value;
            true
        },
        (PropertyValue::Color(v), c) if c < 4 => {
            v[c] = new_value;
            true
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 50.0))
    }

    #[test]
    fn axis_maps_time_to_full_width() {
        let axis = Axis::new(rect(), 10.0, 0.0, 1.0);
        assert!((axis.x(0.0) - 0.0).abs() < 0.001);
        assert!((axis.x(10.0) - 100.0).abs() < 0.001);
        assert!((axis.x(5.0) - 50.0).abs() < 0.001);
    }

    #[test]
    fn axis_time_at_x_round_trips() {
        let axis = Axis::new(rect(), 4.0, 0.0, 1.0);
        assert!((axis.time_at_x(25.0) - 1.0).abs() < 0.001);
        assert!((axis.time_at_x(100.0) - 4.0).abs() < 0.001);
    }

    #[test]
    fn axis_value_at_y_is_inverted() {
        let axis = Axis::new(rect(), 1.0, 0.0, 10.0);
        // Top of the plot is the maximum, bottom the minimum.
        assert!((axis.value_at_y(0.0) - 10.0).abs() < 0.001);
        assert!((axis.value_at_y(50.0) - 0.0).abs() < 0.001);
        assert!((axis.value_at_y(25.0) - 5.0).abs() < 0.001);
        // Dragging above the plot top must extrapolate past the graphed max so
        // a keyframe can be raised beyond the current value window.
        assert!(axis.value_at_y(-25.0) > 10.0);
    }

    #[test]
    fn degenerate_value_window_is_padded() {
        let (min, max) = pad_range(3.0, 3.0);
        assert!(max > min);
    }

    #[test]
    fn set_channel_targets_the_right_component() {
        use animatix::timeline::PropertyValue;
        let mut v = PropertyValue::Vec2([1.0, 2.0]);
        assert!(set_channel(&mut v, 1, 9.0));
        assert_eq!(v, PropertyValue::Vec2([1.0, 9.0]));

        let mut c = PropertyValue::Color([0.0, 0.0, 0.0, 1.0]);
        assert!(set_channel(&mut c, 3, 0.5));
        assert_eq!(c, PropertyValue::Color([0.0, 0.0, 0.0, 0.5]));

        // Out-of-range channel is rejected rather than silently writing.
        assert!(!set_channel(&mut v, 5, 1.0));
    }
}
