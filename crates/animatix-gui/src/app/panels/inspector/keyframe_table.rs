use animatix::easing::Easing;
use animatix::timeline::{
    ActorField, AnimationTrack, PropertyValue, ShapeType, Timeline,
    property_has_keyframes, property_keyframe_times, property_keyframe_easing,
    read_property_value, allowed_property_indices, PROPERTY_REGISTRY,
};
use egui::Vec2;

use crate::app::theme::*;
use crate::app::commands::{Command, CommandQueue};

// ─── Data Structures ──────────────────────────────────────────────────────

struct PropertyTrackInfo {
    name: &'static str,
    keyframes: Vec<(u64, String, animatix::easing::Easing)>, // time_ms, formatted_value, easing
}

struct TrackGroup {
    name: &'static str,
    icon: &'static str,
    tracks: Vec<PropertyTrackInfo>,
}

// ─── Public Entry Point ───────────────────────────────────────────────────

pub(super) fn count_keyframes(track: &AnimationTrack) -> usize {
    let indices = allowed_property_indices(track.kind);
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
    actor_label: &str,
    commands: &mut CommandQueue,
) {
    let groups = collect_track_groups(track);

    if groups.is_empty() {
        render_empty_state(ui);
        return;
    }

    // Compact list of animated properties with keyframe counts
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
    for group in &groups {
        for track_info in &group.tracks {
            render_compact_track_row(ui, group, track_info, current_time_ms, timeline, actor_label, commands);
        }
    }
    ui.spacing_mut().item_spacing = Vec2::new(0.0, SPACE_S);
}

/// Collect all keyframe times across all property tracks (for mini timeline).
pub(super) fn collect_all_keyframe_times(track: &AnimationTrack) -> Vec<f64> {
    let indices = allowed_property_indices(track.kind);
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

// ─── Compact Track Row ────────────────────────────────────────────────────

fn easing_display_name(easing: Easing) -> &'static str {
    match easing {
        Easing::Linear => "Linear",
        Easing::EaseIn => "Ease In",
        Easing::EaseOut => "Ease Out",
        Easing::EaseInOut => "Ease In Out",
        Easing::Bounce => "Bounce",
        Easing::Elastic => "Elastic",
        Easing::Back => "Back",
        Easing::Expo => "Expo",
    }
}

fn render_compact_track_row(
    ui: &mut egui::Ui,
    group: &TrackGroup,
    track: &PropertyTrackInfo,
    current_time_ms: u64,
    timeline: &Timeline,
    actor_label: &str,
    commands: &mut CommandQueue,
) {
    let row_height = ROW_S;
    let available = ui.available_width();
    let (row_rect, response) =
        ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

    if response.hovered() {
        ui.painter().rect_filled(row_rect, 0.0, BG_HOVER);
    }

    let baseline_y = row_rect.center().y;
    let duration_s = timeline.duration_seconds().max(0.1);

    // Icon
    let mut cursor_x = row_rect.min.x + SPACE_S;
    ui.painter().text(
        egui::pos2(cursor_x + 7.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        group.icon,
        egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
        TEXT_MUTED,
    );
    cursor_x += 18.0;

    // Property name
    ui.painter().text(
        egui::pos2(cursor_x, baseline_y),
        egui::Align2::LEFT_CENTER,
        track.name,
        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        TEXT_SECONDARY,
    );

    // Keyframe count badge (right-aligned)
    let count = track.keyframes.len();
    let count_label = format!("{} {}", egui_phosphor::regular::DIAMOND, count);
    ui.painter().text(
        egui::pos2(row_rect.max.x - SPACE_S, baseline_y),
        egui::Align2::RIGHT_CENTER,
        count_label,
        egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
        TEXT_MUTED,
    );

    // Mini timeline strip (subtle, behind everything)
    let strip_left = cursor_x + 70.0_f32.min(available * 0.3);
    let strip_right = row_rect.max.x - SPACE_S - 50.0;
    if strip_right > strip_left + 20.0 {
        let strip_rect = egui::Rect::from_min_max(
            egui::pos2(strip_left, row_rect.min.y + 5.0),
            egui::pos2(strip_right, row_rect.max.y - 5.0),
        );
        ui.painter().rect_filled(strip_rect, RADIUS_S, BG_WIDGET);

        // Keyframe dots on the strip
        for (time_ms, value, easing) in &track.keyframes {
            let fraction = ((*time_ms as f64 / 1000.0) / duration_s).clamp(0.0, 1.0);
            let x = egui::lerp(strip_rect.left()..=strip_rect.right(), fraction as f32);
            let is_current = *time_ms == current_time_ms;
            let color = if is_current { AMBER } else { TEXT_MUTED };
            let size = if is_current { 3.5 } else { 2.5 };
            let dot_pos = egui::pos2(x, strip_rect.center().y);
            let dot_rect = egui::Rect::from_center_size(dot_pos, egui::vec2(8.0, 8.0));
            let dot_id = ui.id().with(("kf_dot", track.name, *time_ms));
            let dot_response = ui.interact(dot_rect, dot_id, egui::Sense::click());
            ui.painter().circle_filled(dot_pos, size, color);

            // Right-click: show easing context menu
            dot_response.context_menu(|ui| {
                ui.set_min_width(120.0);
                ui.strong("Easing");
                ui.separator();
                let current_easing = *easing;
                for &(id_str, display_name) in animatix::easing::EASING_REGISTRY {
                    let variant = match id_str {
                        "linear" => Easing::Linear,
                        "easein" => Easing::EaseIn,
                        "easeout" => Easing::EaseOut,
                        "easeinout" => Easing::EaseInOut,
                        "bounce" => Easing::Bounce,
                        "elastic" => Easing::Elastic,
                        "back" => Easing::Back,
                        "expo" => Easing::Expo,
                        _ => Easing::Linear,
                    };
                    let is_selected = variant == current_easing;
                    if ui.selectable_label(is_selected, display_name).clicked() {
                        commands.push_back(Command::SetKeyframeEasing {
                            actor: actor_label.to_string(),
                            property: track.name.to_string(),
                            time_s: *time_ms as f64 / 1000.0,
                            easing: variant,
                        });
                        ui.close();
                    }
                }
            });

            // Per-dot hover tooltip with value and easing info
            dot_response.on_hover_ui(|ui| {
                ui.label(format!("{:.2}s", *time_ms as f64 / 1000.0));
                ui.label(egui::RichText::new(value).size(FONT_SIZE_XS).color(TEXT_SECONDARY));
                ui.label(
                    egui::RichText::new(format!("ease: {}", easing_display_name(*easing)))
                        .size(FONT_SIZE_XS)
                        .color(TEXT_MUTED),
                );
            });
        }

        // Current time indicator
        let current_fraction = ((current_time_ms as f64 / 1000.0) / duration_s).clamp(0.0, 1.0);
        let playhead_x = egui::lerp(strip_rect.left()..=strip_rect.right(), current_fraction as f32);
        if playhead_x >= strip_rect.left() && playhead_x <= strip_rect.right() {
            ui.painter().line_segment(
                [egui::pos2(playhead_x, strip_rect.top()), egui::pos2(playhead_x, strip_rect.bottom())],
                egui::Stroke::new(1.0, AMBER),
            );
        }

        // Click to scrub
        let strip_response = ui.interact(
            strip_rect,
            ui.id().with(("compact_strip", track.name)),
            egui::Sense::click_and_drag(),
        );
        if strip_response.clicked() || strip_response.dragged() {
            if let Some(pos) = strip_response.interact_pointer_pos() {
                let fraction = ((pos.x - strip_rect.left()) / strip_rect.width()).clamp(0.0, 1.0) as f64;
                commands.push_back(Command::ScrubTo(fraction * duration_s));
            }
        }
    }

    // Hover tooltip showing keyframe values
    response.on_hover_ui(|ui| {
        ui.horizontal(|ui| {
            ui.strong(track.name);
            ui.label(
                egui::RichText::new(format!("{} keyframes", track.keyframes.len()))
                    .size(FONT_SIZE_XS)
                    .color(TEXT_MUTED),
            );
        });
        ui.add_space(SPACE_XS);
        for (time_ms, value, easing) in &track.keyframes {
            let is_current = *time_ms == current_time_ms;
            let color = if is_current { AMBER } else { TEXT_SECONDARY };
            ui.horizontal(|ui| {
                let icon = egui_phosphor::regular::DIAMOND;
                ui.label(egui::RichText::new(icon).size(FONT_SIZE_XS).color(color));
                ui.label(
                    egui::RichText::new(format!("{:.2}s", *time_ms as f64 / 1000.0))
                        .monospace()
                        .size(FONT_SIZE_XS)
                        .color(color),
                );
                ui.label(
                    egui::RichText::new(value).size(FONT_SIZE_XS).color(TEXT_SECONDARY),
                );
                ui.label(
                    egui::RichText::new(easing_display_name(*easing))
                        .size(FONT_SIZE_XS)
                        .color(TEXT_MUTED),
                );
            });
        }
    });
}

// ─── Collection (generic via registry) ────────────────────────────────────

fn collect_track_groups(track: &AnimationTrack) -> Vec<TrackGroup> {
    let indices = allowed_property_indices(track.kind);

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
                let easing = property_keyframe_easing(track, schema.field, time_ms)
                    .unwrap_or(animatix::easing::Easing::Linear);
                keyframes.push((time_ms, format_value(&value, schema.name), easing));
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
                ShapeType::from(*v).to_string()
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
        PropertyValue::PointList(v) => format!("[{} pts]", v.len()),
        PropertyValue::CommandList(v) => {
            if v.len() > 24 {
                format!("{}…", &v[..24])
            } else {
                v.clone()
            }
        }
        PropertyValue::PlacementMode(v) => format!("{:?}", v),
        PropertyValue::MorphOptions(v) => format!("{:?}", v),
        PropertyValue::Transform(v) => format!("[{:.2}, {:.2}, {:.2}, {:.2}, {:.2}, {:.2}]", v[0], v[1], v[2], v[3], v[4], v[5]),
    }
}
