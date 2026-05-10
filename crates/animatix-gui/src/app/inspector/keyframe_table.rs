use animatix::timeline::{
    ActorField, AnimationTrack, PropertyValue, ShapeType, Timeline,
    property_has_keyframes, property_has_keyframe_at, property_keyframe_times,
    read_property_value, allowed_property_indices, PROPERTY_REGISTRY,
};
use egui::{Color32, Vec2};

use crate::app::components;
use crate::app::theme::*;
use crate::app::workspace::UiActions;

// ─── Data Structures ──────────────────────────────────────────────────────

struct PropertyTrackInfo {
    name: &'static str,
    keyframes: Vec<(u64, String)>, // time_ms, formatted_value
}

struct TrackGroup {
    name: &'static str,
    icon: &'static str,
    tracks: Vec<PropertyTrackInfo>,
}

// ─── Public Entry Point ───────────────────────────────────────────────────

pub(super) fn count_keyframes(track: &AnimationTrack) -> usize {
    let sub_kind = match track.kind {
        animatix::timeline::ActorKindId::Shape(sk) => Some(sk),
        _ => None,
    };
    let indices = allowed_property_indices(track.kind, sub_kind);
    indices
        .iter()
        .filter_map(|&idx| {
            let schema = &PROPERTY_REGISTRY[idx];
            if property_has_keyframes(track, schema.field) {
                Some(property_keyframe_times(track, schema.field).len())
            } else {
                None
            }
        })
        .sum()
}

pub(super) fn render_dope_sheet(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    track: &AnimationTrack,
    current_time_ms: u64,
    actions: &mut UiActions,
) {
    let duration_s = timeline.duration_seconds().max(0.1);
    let groups = collect_track_groups(track);

    if groups.is_empty() {
        render_empty_state(ui);
        return;
    }

    for group in &groups {
        render_track_group(ui, group, current_time_ms, duration_s, actions);
    }
}

/// Collect all keyframe times across all property tracks (for mini timeline).
pub(super) fn collect_all_keyframe_times(track: &AnimationTrack) -> Vec<f64> {
    let sub_kind = match track.kind {
        animatix::timeline::ActorKindId::Shape(sk) => Some(sk),
        _ => None,
    };
    let indices = allowed_property_indices(track.kind, sub_kind);
    let mut times = std::collections::BTreeSet::new();

    for &idx in &indices {
        let schema = &PROPERTY_REGISTRY[idx];
        for t in property_keyframe_times(track, schema.field) {
            times.insert(t);
        }
    }

    times.into_iter().map(|ms| ms as f64 / 1000.0).collect()
}

// ─── Empty State ──────────────────────────────────────────────────────────

fn render_empty_state(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(SPACE_M * 3.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(egui_phosphor::regular::FILM_STRIP)
                    .size(22.0)
                    .color(TEXT_MUTED),
            )
            .selectable(false),
        );
        ui.add_space(SPACE_S);
        ui.add(
            egui::Label::new(
                egui::RichText::new("No keyframes")
                    .size(FONT_SIZE_S)
                    .color(TEXT_MUTED),
            )
            .selectable(false),
        );
        ui.add_space(SPACE_XS);
        ui.add(
            egui::Label::new(
                egui::RichText::new("Edit properties with keyframe mode enabled")
                    .size(FONT_SIZE_XS)
                    .color(TEXT_MUTED),
            )
            .selectable(false),
        );
    });
}

// ─── Track Group ──────────────────────────────────────────────────────────

fn render_track_group(
    ui: &mut egui::Ui,
    group: &TrackGroup,
    current_time_ms: u64,
    duration_s: f64,
    actions: &mut UiActions,
) {
    let group_id = ui.id().with(("kf_group", group.name));
    let mut expanded = ui.data(|d| d.get_temp::<bool>(group_id)).unwrap_or(true);

    let kf_count: usize = group.tracks.iter().map(|t| t.keyframes.len()).sum();
    let header = components::Row::new(group.name)
        .height(ROW_M)
        .icon(Some(group.icon))
        .has_children(true)
        .expanded(expanded)
        .right(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(kf_count.to_string())
                        .size(FONT_SIZE_XS)
                        .color(TEXT_MUTED),
                )
                .selectable(false),
            );
        });
    let response = header.show(ui, group_id.with("header"));

    if response.row_clicked || response.chevron_clicked {
        expanded = !expanded;
        ui.data_mut(|d| d.insert_temp(group_id, expanded));
    }

    if expanded {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
        for track in &group.tracks {
            render_track_row(ui, track, current_time_ms, duration_s, actions);
        }
        ui.spacing_mut().item_spacing = Vec2::new(0.0, SPACE_S);
    }
}

// ─── Track Row ────────────────────────────────────────────────────────────

fn render_track_row(
    ui: &mut egui::Ui,
    track: &PropertyTrackInfo,
    current_time_ms: u64,
    duration_s: f64,
    actions: &mut UiActions,
) {
    let row_height = ROW_S;
    let available = ui.available_width();
    let (row_rect, response) =
        ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

    if response.hovered() {
        ui.painter().rect_filled(row_rect, 0.0, BG_HOVER);
    }

    let label_width = 90.0_f32.min(available * 0.35);
    let timeline_left = row_rect.min.x + label_width;
    let timeline_right = row_rect.max.x - SPACE_S;

    // Property label
    ui.painter().text(
        egui::pos2(row_rect.min.x + SPACE_L, row_rect.center().y),
        egui::Align2::LEFT_CENTER,
        track.name,
        egui::TextStyle::Small.resolve(ui.style()),
        TEXT_SECONDARY,
    );

    // Timeline strip
    let strip_rect = egui::Rect::from_min_max(
        egui::pos2(timeline_left, row_rect.min.y + 3.0),
        egui::pos2(timeline_right, row_rect.max.y - 3.0),
    );
    ui.painter().rect_filled(strip_rect, RADIUS_M, BG_WIDGET);
    ui.painter().rect_stroke(
        strip_rect,
        RADIUS_M,
        egui::Stroke::new(1.0, BORDER),
        egui::StrokeKind::Outside,
    );

    // Second tick marks (subtle)
    let sec_step = if duration_s > 20.0 { 5.0 } else { 1.0 };
    let mut sec = sec_step;
    while sec < duration_s {
        let fraction = sec / duration_s;
        let x = egui::lerp(strip_rect.left()..=strip_rect.right(), fraction as f32);
        ui.painter().line_segment(
            [
                egui::pos2(x, strip_rect.top() + 2.0),
                egui::pos2(x, strip_rect.bottom() - 2.0),
            ],
            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 15)),
        );
        sec += sec_step;
    }

    // Click / drag on strip to scrub
    let strip_response = ui.interact(
        strip_rect,
        ui.id().with(("strip", track.name)),
        egui::Sense::click_and_drag(),
    );
    if (strip_response.clicked() || strip_response.dragged())
        && strip_response.interact_pointer_pos().is_some()
    {
        let pos = strip_response.interact_pointer_pos().unwrap();
        let fraction =
            ((pos.x - strip_rect.left()) / strip_rect.width()).clamp(0.0, 1.0) as f64;
        let time_s = fraction * duration_s;
        actions.scrub_to = Some(time_s);
    }

    // Current time indicator
    let current_fraction =
        ((current_time_ms as f64 / 1000.0) / duration_s).clamp(0.0, 1.0);
    let playhead_x = egui::lerp(
        strip_rect.left()..=strip_rect.right(),
        current_fraction as f32,
    );

    // Keyframe diamonds
    let diamond_size = 5.0;
    for (time_ms, value) in &track.keyframes {
        let fraction = ((*time_ms as f64 / 1000.0) / duration_s).clamp(0.0, 1.0);
        let x = egui::lerp(strip_rect.left()..=strip_rect.right(), fraction as f32);
        let center = egui::pos2(x, strip_rect.center().y);
        let is_current = *time_ms == current_time_ms;

        let size = if is_current { diamond_size + 1.5 } else { diamond_size };
        components::keyframe_dot(&ui.painter(), center, size, is_current);

        // Hover tooltip
        let hover_rect =
            egui::Rect::from_center_size(center, Vec2::splat(size + 6.0));
        let diamond_response = ui.interact(
            hover_rect,
            ui.id().with(("kf", track.name, *time_ms)),
            egui::Sense::hover(),
        );
        diamond_response.on_hover_ui(|ui| {
            ui.horizontal(|ui| {
                ui.strong(track.name);
                ui.label(
                    egui::RichText::new(format!("{:.2}s", *time_ms as f64 / 1000.0))
                        .monospace()
                        .color(AMBER),
                );
            });
            ui.add_space(SPACE_XS);
            ui.label(egui::RichText::new(format!("Value: {}", value)).size(FONT_SIZE_M));
        });
    }

    // Playhead line (drawn on top of diamonds)
    if playhead_x >= strip_rect.left() && playhead_x <= strip_rect.right() {
        components::playhead(
            &ui.painter(),
            playhead_x,
            strip_rect.top() - 1.0..strip_rect.bottom() + 1.0,
        );
    }
}

// ─── Collection (generic via registry) ────────────────────────────────────

fn collect_track_groups(track: &AnimationTrack) -> Vec<TrackGroup> {
    let sub_kind = match track.kind {
        animatix::timeline::ActorKindId::Shape(sk) => Some(sk),
        _ => None,
    };
    let indices = allowed_property_indices(track.kind, sub_kind);

    let mut transform = Vec::new();
    let mut style = Vec::new();
    let mut shape = Vec::new();
    let mut text = Vec::new();
    let mut media = Vec::new();

    for &idx in &indices {
        let schema = &PROPERTY_REGISTRY[idx];
        if !property_has_keyframes(track, schema.field) {
            continue;
        }

        let mut keyframes = Vec::new();
        for time_ms in property_keyframe_times(track, schema.field) {
            if let Some(value) = read_property_value(track, schema.field, time_ms) {
                keyframes.push((time_ms, format_value(&value, schema.name)));
            }
        }
        if keyframes.is_empty() {
            continue;
        }

        let info = PropertyTrackInfo {
            name: schema.name,
            keyframes,
        };

        match schema.field {
            ActorField::Position
            | ActorField::MotionOffset
            | ActorField::Size
            | ActorField::LayoutSize
            | ActorField::Rotation
            | ActorField::Scale
            | ActorField::PlacementMode
            | ActorField::PositionBinding => transform.push(info),
            ActorField::Color
            | ActorField::Opacity
            | ActorField::StrokeWidth
            | ActorField::StrokeColor
            | ActorField::StrokeProgress
            | ActorField::FillOpacity
            | ActorField::MorphOptions => style.push(info),
            ActorField::ShapeType
            | ActorField::LineFrom
            | ActorField::LineTo
            | ActorField::ArcAngles
            | ActorField::Points
            | ActorField::VectorPaths => shape.push(info),
            ActorField::TextContent
            | ActorField::FontFamily
            | ActorField::FontSize
            | ActorField::TextPaths => text.push(info),
            ActorField::ImageData | ActorField::SvgPaths => media.push(info),
            _ => {}
        }
    }

    let mut groups = Vec::new();
    if !transform.is_empty() {
        groups.push(TrackGroup {
            name: "Transform",
            icon: egui_phosphor::regular::ARROWS_OUT_CARDINAL,
            tracks: transform,
        });
    }
    if !style.is_empty() {
        groups.push(TrackGroup {
            name: "Style",
            icon: egui_phosphor::regular::PAINT_BRUSH,
            tracks: style,
        });
    }
    if !shape.is_empty() {
        groups.push(TrackGroup {
            name: "Shape",
            icon: egui_phosphor::regular::SHAPES,
            tracks: shape,
        });
    }
    if !text.is_empty() {
        groups.push(TrackGroup {
            name: "Text",
            icon: egui_phosphor::regular::TEXT_T,
            tracks: text,
        });
    }
    if !media.is_empty() {
        groups.push(TrackGroup {
            name: "Media",
            icon: egui_phosphor::regular::FILM_STRIP,
            tracks: media,
        });
    }

    groups
}

fn format_value(value: &PropertyValue, name: &str) -> String {
    match value {
        PropertyValue::F32(v) => {
            if name == "rotation" {
                format!("{:.1}°", v.to_degrees())
            } else if *v == v.floor() && v.abs() < 10000.0 {
                format!("{:.0}", v)
            } else {
                format!("{:.2}", v)
            }
        }
        PropertyValue::U32(v) => {
            if name == "shape_type" {
                let st = match *v {
                    0 => ShapeType::Rect,
                    1 => ShapeType::Circle,
                    2 => ShapeType::Line,
                    3 => ShapeType::Ellipse,
                    4 => ShapeType::Arc,
                    5 => ShapeType::Polygon,
                    6 => ShapeType::Path,
                    7 => ShapeType::Arrow,
                    _ => ShapeType::Rect,
                };
                format!("{:?}", st)
            } else {
                v.to_string()
            }
        }
        PropertyValue::Vec2(v) => format!("({:.1}, {:.1})", v[0], v[1]),
        PropertyValue::Vec4(v) => {
            let r = (v[0] * 255.0).round() as u8;
            let g = (v[1] * 255.0).round() as u8;
            let b = (v[2] * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}", r, g, b)
        }
        PropertyValue::Color(v) => {
            let r = (v[0] * 255.0).round() as u8;
            let g = (v[1] * 255.0).round() as u8;
            let b = (v[2] * 255.0).round() as u8;
            if v[3] >= 0.999 {
                format!("#{:02x}{:02x}{:02x}", r, g, b)
            } else {
                format!("rgba({},{},{},{:.2})", r, g, b, v[3])
            }
        }
        PropertyValue::String(v) => {
            if v.len() > 24 {
                format!("{}…", &v[..24])
            } else {
                v.clone()
            }
        }
    }
}
