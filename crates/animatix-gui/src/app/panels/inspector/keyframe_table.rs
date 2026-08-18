use animatix::timeline::{
    ActorField, AnimationTrack, PROPERTY_REGISTRY, PropertyValue, ShapeType, Timeline,
    allowed_property_indices, property_has_keyframes, property_keyframe_easing,
    property_keyframe_times, read_property_plan_slot, read_property_value,
};
use animatix_syntax::easing::Easing;
use egui::{Rect, Vec2};
use eparts::widget::UiExt;

use crate::app::commands::{ActionQueue, KeyframeCommand, PlaybackCommand};
use crate::app::components::{Badge, Tooltip};
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;

// ─── Data Structures ──────────────────────────────────────────────────────

struct PropertyTrackInfo {
    name: String,
    keyframes: Vec<(u64, String, animatix_syntax::easing::Easing)>, /* time_ms, formatted_value,
                                                                     * easing */
}

struct TrackGroup {
    icon: &'static str,
    tracks: Vec<PropertyTrackInfo>,
}

// ─── Public Entry Point ───────────────────────────────────────────────────

pub(super) fn count_keyframes(track: &AnimationTrack) -> usize {
    let indices = allowed_property_indices(track.kind);
    let builtin = indices
        .iter()
        .filter_map(|&idx| {
            let schema = &PROPERTY_REGISTRY[idx];
            if property_has_keyframes(track, schema.field) {
                Some(property_keyframe_times(track, schema.field).len())
            } else {
                None
            }
        })
        .sum::<usize>();
    let extension = track
        .property_plan
        .iter()
        .filter(|slot| slot.id.0 >= 1_000_000)
        .map(|slot| slot.track.keyframe_count())
        .sum::<usize>();
    builtin + extension
}

pub(super) fn render_dope_sheet(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    track: &AnimationTrack,
    current_time_ms: u64,
    actor_label: &str,
    commands: &mut ActionQueue,
    active_scene: Option<&str>,
) {
    let sp = spatial(ui);
    let groups = collect_track_groups(timeline, track);

    if groups.is_empty() {
        crate::app::components::layout::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No keyframes",
            "Edit properties with keyframe mode enabled",
        );
        return;
    }

    // Compact list of animated properties with keyframe counts
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
    for group in &groups {
        for track_info in &group.tracks {
            render_compact_track_row(
                ui,
                group,
                track_info,
                current_time_ms,
                timeline,
                actor_label,
                commands,
                active_scene,
            );
        }
    }
    ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_2);
}

// ─── Compact Track Row ────────────────────────────────────────────────────

fn easing_display_name(easing: Easing) -> String {
    match easing {
        Easing::Linear => "Linear".into(),
        Easing::EaseIn => "Ease In".into(),
        Easing::EaseOut => "Ease Out".into(),
        Easing::EaseInOut => "Ease In Out".into(),
        Easing::Bounce => "Bounce".into(),
        Easing::Elastic => "Elastic".into(),
        Easing::Back => "Back".into(),
        Easing::Expo => "Expo".into(),
        Easing::CubicBezier(cp) => animatix_syntax::easing::format_cubic_bezier(cp),
    }
}

fn render_compact_track_row(
    ui: &mut egui::Ui,
    group: &TrackGroup,
    track: &PropertyTrackInfo,
    current_time_ms: u64,
    timeline: &Timeline,
    actor_label: &str,
    commands: &mut ActionQueue,
    active_scene: Option<&str>,
) {
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
    let row_height = sp.base.row_s;
    let available = ui.available_width();
    let (row_rect, response) =
        ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

    if response.hovered() {
        ui.painter().rect_filled(row_rect, 0.0, theme.surface.hover);
    }

    let baseline_y = row_rect.center().y;
    let duration_s = timeline.duration_seconds().max(0.1);

    // Icon
    let mut cursor_x = row_rect.min.x + sp.base.space_2;
    ui.painter().text(
        egui::pos2(cursor_x + 7.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        group.icon,
        TextRole::Micro.font_id(),
        theme.text.muted,
    );
    cursor_x += 18.0;

    // Property name
    ui.painter().text(
        egui::pos2(cursor_x, baseline_y),
        egui::Align2::LEFT_CENTER,
        track.name.as_str(),
        TextRole::BodyS.font_id(),
        theme.text.secondary,
    );

    // Keyframe count badge (right-aligned)
    let count = track.keyframes.len();
    let count_rect = Rect::from_center_size(
        egui::pos2(row_rect.max.x - sp.base.space_2, baseline_y),
        Vec2::new(40.0, row_height),
    );
    ui.put(count_rect, Badge::new(format!("{} {}", egui_phosphor::regular::DIAMOND, count)));

    // Mini timeline strip (subtle, behind everything)
    let strip_left = cursor_x + 70.0_f32.min(available * 0.3);
    let strip_right = row_rect.max.x - sp.base.space_2 - 50.0;
    if strip_right > strip_left + 20.0 {
        let strip_rect = egui::Rect::from_min_max(
            egui::pos2(strip_left, row_rect.min.y + 5.0),
            egui::pos2(strip_right, row_rect.max.y - 5.0),
        );
        ui.painter().rect_filled(strip_rect, RADIUS_S, theme.surface.widget);

        // Keyframe dots on the strip
        for (time_ms, value, easing) in &track.keyframes {
            let fraction = ((*time_ms as f64 / 1000.0) / duration_s).clamp(0.0, 1.0);
            let x = egui::lerp(strip_rect.left()..=strip_rect.right(), fraction as f32);
            let is_current = *time_ms == current_time_ms;
            let color = if is_current {
                theme.status.warning
            } else {
                theme.text.muted
            };
            let size = if is_current { 3.5 } else { 2.5 };
            let dot_pos = egui::pos2(x, strip_rect.center().y);
            let dot_rect = egui::Rect::from_center_size(dot_pos, egui::vec2(8.0, 8.0));
            let dot_id = ui.id().with(("kf_dot", track.name.as_str(), *time_ms));
            let dot_response = ui.interact(dot_rect, dot_id, egui::Sense::click());
            ui.painter().circle_filled(dot_pos, size, color);

            // Right-click: show easing context menu
            dot_response.context_menu(|ui| {
                ui.set_min_width(120.0);
                ui.strong("Easing");
                ui.separator();
                let current_easing = *easing;
                for &(id_str, display_name) in animatix_syntax::easing::EASING_REGISTRY {
                    let variant = animatix_syntax::easing::parse_easing_name(id_str)
                        .unwrap_or(Easing::Linear);
                    let is_selected = variant == current_easing;
                    if ui.stable_selectable_label(is_selected, display_name).clicked() {
                        commands.push_back(
                            KeyframeCommand::SetKeyframeEasing {
                                scene: active_scene.map(ToOwned::to_owned),
                                actor: actor_label.to_string(),
                                property: track.name.to_string(),
                                time_s: *time_ms as f64 / 1000.0,
                                easing: variant,
                            }
                            .into(),
                        );
                        ui.close();
                    }
                }
            });

            // Per-dot hover tooltip with value and easing info
            Tooltip::new(ui.id().with(("kf_dot_tooltip", track.name.as_str(), *time_ms))).show(
                ui,
                &dot_response,
                |ui| {
                    ui.label(format!("{:.2}s", *time_ms as f64 / 1000.0));
                    ui.label(
                        egui::RichText::new(value)
                            .size(TextRole::Micro.size())
                            .color(theme.text.secondary),
                    );
                    ui.label(
                        egui::RichText::new(format!("ease: {}", easing_display_name(*easing)))
                            .size(TextRole::Micro.size())
                            .color(theme.text.muted),
                    );
                },
            );
        }

        // Current time indicator
        let current_fraction = ((current_time_ms as f64 / 1000.0) / duration_s).clamp(0.0, 1.0);
        let playhead_x =
            egui::lerp(strip_rect.left()..=strip_rect.right(), current_fraction as f32);
        if playhead_x >= strip_rect.left() && playhead_x <= strip_rect.right() {
            ui.painter().line_segment(
                [
                    egui::pos2(playhead_x, strip_rect.top()),
                    egui::pos2(playhead_x, strip_rect.bottom()),
                ],
                egui::Stroke::new(STROKE_WIDTH, theme.status.warning),
            );
        }

        // Click to scrub
        let strip_response = ui.interact(
            strip_rect,
            ui.id().with(("compact_strip", track.name.as_str())),
            egui::Sense::click_and_drag(),
        );
        if strip_response.clicked() || strip_response.dragged() {
            if let Some(pos) = strip_response.interact_pointer_pos() {
                let fraction =
                    ((pos.x - strip_rect.left()) / strip_rect.width()).clamp(0.0, 1.0) as f64;
                commands.push_back(PlaybackCommand::ScrubTo(fraction * duration_s).into());
            }
        }
    }

    // Hover tooltip showing keyframe values
    Tooltip::new(ui.id().with(("compact_track_tooltip", track.name.as_str()))).show(
        ui,
        &response,
        |ui| {
            ui.horizontal(|ui| {
                ui.strong(track.name.as_str());
                ui.label(
                    egui::RichText::new(format!("{} keyframes", track.keyframes.len()))
                        .size(TextRole::Micro.size())
                        .color(theme.text.muted),
                );
            });
            ui.add_space(sp.base.space_1);
            for (time_ms, value, easing) in &track.keyframes {
                let is_current = *time_ms == current_time_ms;
                let color = if is_current {
                    theme.status.warning
                } else {
                    theme.text.secondary
                };
                ui.horizontal(|ui| {
                    let icon = egui_phosphor::regular::DIAMOND;
                    ui.label(egui::RichText::new(icon).size(TextRole::Micro.size()).color(color));
                    ui.label(
                        egui::RichText::new(format!("{:.2}s", *time_ms as f64 / 1000.0))
                            .monospace()
                            .size(TextRole::Micro.size())
                            .color(color),
                    );
                    ui.label(
                        egui::RichText::new(value)
                            .size(TextRole::Micro.size())
                            .color(theme.text.secondary),
                    );
                    ui.label(
                        egui::RichText::new(easing_display_name(*easing))
                            .size(TextRole::Micro.size())
                            .color(theme.text.muted),
                    );
                });
            }
        },
    );
}

// ─── Collection (generic via registry) ────────────────────────────────────

fn collect_track_groups(timeline: &Timeline, track: &AnimationTrack) -> Vec<TrackGroup> {
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
                    .unwrap_or(animatix_syntax::easing::Easing::Linear);
                keyframes.push((time_ms, format_value(&value, schema.name), easing));
            }
        }
        if keyframes.is_empty() {
            continue;
        }

        let info = PropertyTrackInfo {
            name: schema.name.to_string(),
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
            _ => {},
        }
    }

    let mut groups = Vec::new();
    if !transform.is_empty() {
        groups.push(TrackGroup {
            icon: egui_phosphor::regular::ARROWS_OUT_CARDINAL,
            tracks: transform,
        });
    }
    if !style.is_empty() {
        groups.push(TrackGroup {
            icon: egui_phosphor::regular::PAINT_BRUSH,
            tracks: style,
        });
    }
    if !shape.is_empty() {
        groups.push(TrackGroup {
            icon: egui_phosphor::regular::SHAPES,
            tracks: shape,
        });
    }
    if !text.is_empty() {
        groups.push(TrackGroup {
            icon: egui_phosphor::regular::TEXT_T,
            tracks: text,
        });
    }
    if !media.is_empty() {
        groups.push(TrackGroup {
            icon: egui_phosphor::regular::FILM_STRIP,
            tracks: media,
        });
    }

    let actor_type = track.actor_type.as_deref();
    let mut extensions = Vec::new();
    for descriptor in timeline.extension_property_descriptors() {
        if !descriptor.actor_types.iter().any(|ty| Some(ty.as_str()) == actor_type) {
            continue;
        }
        if track.property_plan.keyframe_count(descriptor.id) == 0 {
            continue;
        }
        let mut keyframes = Vec::new();
        for time_ms in track.property_plan.keyframe_times(descriptor.id) {
            if let Some(value) = read_property_plan_slot(track, descriptor.id, time_ms) {
                let easing = track
                    .property_plan
                    .keyframe_easing(descriptor.id, time_ms)
                    .unwrap_or(animatix_syntax::easing::Easing::Linear);
                keyframes.push((time_ms, format_value(&value, &descriptor.name), easing));
            }
        }
        if keyframes.is_empty() {
            continue;
        }
        extensions.push(PropertyTrackInfo {
            name: descriptor.name.clone(),
            keyframes,
        });
    }
    if !extensions.is_empty() {
        groups.push(TrackGroup {
            icon: egui_phosphor::regular::PLUG,
            tracks: extensions,
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
        },
        PropertyValue::U32(v) => {
            if name == "shape_type" {
                ShapeType::from(*v).to_string()
            } else {
                v.to_string()
            }
        },
        PropertyValue::Vec2(v) => format!("({:.1}, {:.1})", v[0], v[1]),
        PropertyValue::Vec4(v) => {
            let r = (v[0] * 255.0).round() as u8;
            let g = (v[1] * 255.0).round() as u8;
            let b = (v[2] * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}", r, g, b)
        },
        PropertyValue::Color(v) => {
            let r = (v[0] * 255.0).round() as u8;
            let g = (v[1] * 255.0).round() as u8;
            let b = (v[2] * 255.0).round() as u8;
            if v[3] >= 0.999 {
                format!("#{:02x}{:02x}{:02x}", r, g, b)
            } else {
                format!("rgba({},{},{},{:.2})", r, g, b, v[3])
            }
        },
        PropertyValue::String(v) => {
            if v.len() > 24 {
                format!("{}…", &v[..24])
            } else {
                v.clone()
            }
        },
        PropertyValue::Bool(v) => v.to_string(),
        PropertyValue::Enum(v) => v.clone(),
        PropertyValue::Variant { value, .. } => match value.as_ref() {
            PropertyValue::Bool(v) => v.to_string(),
            PropertyValue::String(v) => v.clone(),
            other => format!("{other:?}"),
        },
        PropertyValue::StringList(v) => format!("[{} items]", v.len()),
        PropertyValue::PointList(v) => format!("[{} pts]", v.len()),
        PropertyValue::CommandList(v) => {
            if v.len() > 24 {
                format!("{}…", &v[..24])
            } else {
                v.clone()
            }
        },
        PropertyValue::Transform(v) => format!(
            "[{:.2}, {:.2}, {:.2}, {:.2}, {:.2}, {:.2}]",
            v[0], v[1], v[2], v[3], v[4], v[5]
        ),
    }
}

#[cfg(test)]
mod tests {
    use animatix::timeline::ActorKindId;
    use animatix::timeline::property_track::PropertyTrack;
    use animatix_syntax::easing::Easing;

    use super::*;

    fn make_track(kind: ActorKindId) -> AnimationTrack {
        let mut track = AnimationTrack::new("test".to_string());
        track.kind = kind;
        track
    }

    #[test]
    fn test_count_keyframes_empty_track() {
        let track = make_track(ActorKindId::Shape(animatix::timeline::ShapeKind::Rect));
        assert_eq!(count_keyframes(&track), 0);
    }

    #[test]
    fn test_count_keyframes_one_keyframe() {
        let mut track = make_track(ActorKindId::Shape(animatix::timeline::ShapeKind::Rect));
        let mut pt = PropertyTrack::new([0.0, 0.0]);
        pt.add_keyframe(0, [100.0, 200.0], Easing::Linear);
        track.geometry.position = Some(pt);
        assert_eq!(count_keyframes(&track), 1);
    }

    #[test]
    fn test_count_keyframes_multiple_properties() {
        let mut track = make_track(ActorKindId::Shape(animatix::timeline::ShapeKind::Rect));

        // Two keyframes on position
        let mut pos = PropertyTrack::new([0.0, 0.0]);
        pos.add_keyframe(0, [100.0, 0.0], Easing::Linear);
        pos.add_keyframe(1000, [200.0, 0.0], Easing::EaseInOut);
        track.geometry.position = Some(pos);

        // One keyframe on rotation
        let mut rot = PropertyTrack::new(0.0);
        rot.add_keyframe(500, 1.57, Easing::EaseOut);
        track.geometry.rotation = Some(rot);

        assert_eq!(count_keyframes(&track), 3);
    }

    #[test]
    fn test_count_keyframes_skips_non_applicable_properties() {
        // Text kinds do not have shape-specific properties like stroke_width applicable
        let mut track = make_track(ActorKindId::Text);

        // position is applicable to Everything
        let mut pos = PropertyTrack::new([0.0, 0.0]);
        pos.add_keyframe(0, [100.0, 0.0], Easing::Linear);
        track.geometry.position = Some(pos);

        // stroke_width is only applicable to AllShapes — not Text
        let mut sw = PropertyTrack::new(1.0);
        sw.add_keyframe(0, 2.0, Easing::Linear);
        track.style.stroke_width = Some(sw);

        // text is applicable to Text
        let mut txt = PropertyTrack::new(String::new());
        txt.add_keyframe(0, "hello".to_string(), Easing::Linear);
        track.text.text_content = Some(txt);

        // Only position and text_content should count (stroke_width is not applicable)
        assert_eq!(count_keyframes(&track), 2);
    }
}
