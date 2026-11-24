//! Read-only keyframe track inspection and selection.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use super::TimelineCoords;
use super::track_lanes::{PropertyGroup, property_group_color};
use crate::app::PreviewPaneState;
use crate::app::components::text_tooltip;
use crate::app::design_tokens::spatial::STROKE_WIDTH;
use crate::app::design_tokens::spatial::timeline::KF_HALF as KF_DIAMOND_HALF;
use crate::app::document::timeline_diff::{KeyframeId, collect_per_property_keyframes};

/// Group an actor's keyframes by time, preserving every property keyed at that
/// time.
pub(crate) fn collect_actor_keyframe_groups(
    geometry: &animatix::timeline::animation_track::GeometryTracks,
    style: &animatix::timeline::animation_track::StyleTracks,
    shape: &animatix::timeline::animation_track::ShapeTracks,
    text: &animatix::timeline::animation_track::TextTracks,
) -> Vec<(u64, Vec<String>)> {
    let mut by_time: std::collections::BTreeMap<u64, Vec<String>> =
        std::collections::BTreeMap::new();
    for (property, times) in collect_per_property_keyframes(geometry, style, shape, text, None) {
        for time_ms in times {
            by_time.entry(time_ms).or_default().push(property.clone());
        }
    }
    by_time.into_iter().collect()
}

/// Is `time_ms`'s aggregate diamond (any property) selected?
pub(crate) fn aggregate_selected(
    selected: &[KeyframeId],
    active_scene: Option<&str>,
    actor: &str,
    time_ms: u64,
) -> bool {
    selected
        .iter()
        .any(|id| id.scene.as_deref() == active_scene && id.actor == actor && id.time_ms == time_ms)
}

/// Is this exact property keyframe selected?
pub(crate) fn keyframe_selected(
    selected: &[KeyframeId],
    active_scene: Option<&str>,
    actor: &str,
    property: &str,
    time_ms: u64,
) -> bool {
    selected.iter().any(|id| {
        id.scene.as_deref() == active_scene
            && id.actor == actor
            && id.property == property
            && id.time_ms == time_ms
    })
}

/// Shared state for static keyframe diamonds and workspace selection.
pub(crate) struct KeyframeRenderCtx<'a> {
    pub coords: &'a TimelineCoords,
    pub preview: &'a PreviewPaneState,
    pub active_scene: Option<&'a str>,
    pub scene_offset: f64,
    pub scene_for_selection: Option<String>,
    pub selected_keyframes: &'a mut Vec<KeyframeId>,
    pub shift_held: bool,
    pub theme: eparts::Theme,
}

struct AggregateDiamond<'a> {
    actor: &'a str,
    time_ms: u64,
    time_s: f64,
    props: &'a [String],
}

impl AggregateDiamond<'_> {
    fn label(&self) -> String {
        self.props.join(", ")
    }

    fn selection_ids(&self, scene: &Option<String>) -> Vec<KeyframeId> {
        self.props
            .iter()
            .map(|property| KeyframeId {
                scene: scene.clone(),
                actor: self.actor.to_string(),
                property: property.clone(),
                time_ms: self.time_ms,
            })
            .collect()
    }
}

/// Click a diamond to select its keyframe identities. Shift-click toggles them
/// in the existing selection.
fn select_keyframes(
    response: &egui::Response,
    ids: Vec<KeyframeId>,
    selected: &mut Vec<KeyframeId>,
    shift_held: bool,
) {
    if !response.clicked() {
        return;
    }
    if shift_held {
        for id in ids {
            if let Some(index) = selected.iter().position(|current| current == &id) {
                selected.remove(index);
            } else {
                selected.push(id);
            }
        }
    } else {
        *selected = ids;
    }
}

fn render_aggregate_diamond(
    ui: &mut egui::Ui,
    diamond: &AggregateDiamond<'_>,
    track_idx: usize,
    bar_area: Rect,
    ctx: &mut KeyframeRenderCtx<'_>,
) {
    let global_time_s = ctx.scene_offset + diamond.time_s;
    let x = ctx.coords.time_to_x(global_time_s);
    if x < bar_area.left() || x > bar_area.right() {
        return;
    }
    let selected = aggregate_selected(
        ctx.selected_keyframes,
        ctx.active_scene,
        diamond.actor,
        diamond.time_ms,
    );
    let active = (global_time_s - ctx.preview.playback.current_time_s()).abs() < 0.01;
    let color = if selected {
        ctx.theme.palette.accent.primary
    } else if active {
        ctx.theme.palette.text.primary
    } else {
        ctx.theme.palette.status.warning
    };
    let center = Pos2::new(x, bar_area.center().y);
    let hit_rect = Rect::from_center_size(center, Vec2::splat(KF_DIAMOND_HALF * 2.5));
    let response = ui.interact(
        hit_rect,
        ui.scope_id().with(("kf_diamond", track_idx, diamond.time_ms)),
        Sense::click(),
    );
    if response.hovered() || selected || active {
        ui.painter().circle_filled(
            center,
            KF_DIAMOND_HALF * 1.8,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 45),
        );
    }
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(x, center.y - KF_DIAMOND_HALF),
            Pos2::new(x + KF_DIAMOND_HALF, center.y),
            Pos2::new(x, center.y + KF_DIAMOND_HALF),
            Pos2::new(x - KF_DIAMOND_HALF, center.y),
        ],
        if response.hovered() {
            color
        } else {
            color.linear_multiply(0.85)
        },
        Stroke::new(0.5, Color32::from_rgba_unmultiplied(255, 255, 255, 70)),
    ));
    text_tooltip(
        ui,
        response.id.with("tooltip"),
        &response,
        &format!("{} @ {:.2}s", diamond.label(), global_time_s),
    );
    select_keyframes(
        &response,
        diamond.selection_ids(&ctx.scene_for_selection),
        ctx.selected_keyframes,
        ctx.shift_held,
    );
}

pub(crate) fn render_actor_keyframes(
    ui: &mut egui::Ui,
    geometry: &animatix::timeline::animation_track::GeometryTracks,
    style: &animatix::timeline::animation_track::StyleTracks,
    shape: &animatix::timeline::animation_track::ShapeTracks,
    text: &animatix::timeline::animation_track::TextTracks,
    actor_label: &str,
    track_idx: usize,
    bar_area: Rect,
    ctx: &mut KeyframeRenderCtx<'_>,
) {
    for (time_ms, props) in collect_actor_keyframe_groups(geometry, style, shape, text) {
        let diamond = AggregateDiamond {
            actor: actor_label,
            time_ms,
            time_s: time_ms as f64 / 1000.0,
            props: &props,
        };
        render_aggregate_diamond(ui, &diamond, track_idx, bar_area, ctx);
    }
}

pub(crate) fn render_property_keyframes(
    ui: &mut egui::Ui,
    keyframe_times: &[u64],
    actor_label: &str,
    property: &str,
    track_idx: usize,
    bar_area: Rect,
    group: Option<PropertyGroup>,
    ctx: &mut KeyframeRenderCtx<'_>,
) {
    for &time_ms in keyframe_times {
        let time_s = ctx.scene_offset + time_ms as f64 / 1000.0;
        let x = ctx.coords.time_to_x(time_s);
        if x < bar_area.left() || x > bar_area.right() {
            continue;
        }
        let selected = keyframe_selected(
            ctx.selected_keyframes,
            ctx.active_scene,
            actor_label,
            property,
            time_ms,
        );
        let active = (time_s - ctx.preview.playback.current_time_s()).abs() < 0.01;
        let base_color = group
            .map(|group| property_group_color(group, ctx.theme))
            .unwrap_or(ctx.theme.palette.status.warning);
        let color = if selected {
            ctx.theme.palette.accent.primary
        } else if active {
            ctx.theme.palette.text.primary
        } else {
            base_color
        };
        let center = Pos2::new(x, bar_area.center().y);
        let hit_rect = Rect::from_center_size(center, Vec2::splat(KF_DIAMOND_HALF * 2.5));
        let response = ui.interact(
            hit_rect,
            ui.scope_id().with(("prop_kf_diamond", track_idx, property, time_ms)),
            Sense::click(),
        );
        if response.hovered() || selected || active {
            ui.painter().circle_filled(
                center,
                KF_DIAMOND_HALF * 1.8,
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 45),
            );
        }
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(x, center.y - KF_DIAMOND_HALF),
                Pos2::new(x + KF_DIAMOND_HALF, center.y),
                Pos2::new(x, center.y + KF_DIAMOND_HALF),
                Pos2::new(x - KF_DIAMOND_HALF, center.y),
            ],
            if response.hovered() {
                color
            } else {
                color.linear_multiply(0.85)
            },
            Stroke::new(STROKE_WIDTH, color),
        ));
        text_tooltip(
            ui,
            response.id.with("tooltip"),
            &response,
            &format!("{} @ {:.2}s", property, time_s),
        );
        select_keyframes(
            &response,
            vec![KeyframeId {
                scene: ctx.scene_for_selection.clone(),
                actor: actor_label.to_string(),
                property: property.to_string(),
                time_ms,
            }],
            ctx.selected_keyframes,
            ctx.shift_held,
        );
    }
}
