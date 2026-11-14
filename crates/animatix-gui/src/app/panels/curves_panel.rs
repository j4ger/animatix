//! Read-only compiled animation curves. All authored changes belong in code.
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command};
use crate::app::components::layout;
use crate::app::design_tokens::spatial::timeline::RULER_HEIGHT;
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels::curve_plot::{self, Axis, CurveChannel};
use animatix::timeline::{EntityId, Timeline};
use egui::{Pos2, Sense, Stroke, Vec2};
use eparts::Theme;
use std::collections::{HashMap, HashSet};
const HEADER_HEIGHT: f32 = 24.0;
const LEGEND_HEIGHT: f32 = 20.0;
const PLOT_PADDING: f32 = 8.0;

pub(crate) struct CurvesContext<'a> {
    pub preview: &'a PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub scene_offset_s: f64,
    pub scene_duration_s: f64,
    pub selected_actors: &'a HashSet<String>,
    pub commands: &'a mut ActionQueue,
}

pub(crate) fn curves_panel_ui(ctx: &mut CurvesContext<'_>, ui: &mut egui::Ui) {
    let theme = eparts::theme(ui);
    let Some(timeline) = ctx.timeline else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::CHART_LINE,
            "Read-only curve review",
            "Open a scene to inspect compiled animation curves",
        );
        return;
    };
    let mut actors: Vec<_> = ctx.selected_actors.iter().collect();
    actors.sort();
    let tracks: Vec<(&str, EntityId)> = actors
        .iter()
        .filter_map(|name| timeline.get_entity_id(name).map(|id| (name.as_str(), id)))
        .collect();
    let Some((actor, _)) = tracks.first() else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::CURSOR_CLICK,
            "Select an actor to review curves",
            "Use the preview or scene browser. Edit animation in code.",
        );
        return;
    };
    let curves = collect_actor_curves(timeline, &tracks, theme, tracks.len() > 1);
    let outer = ui.available_rect_before_wrap();
    ui.allocate_rect(outer, Sense::hover());
    let painter = ui.painter_at(outer);
    let header = egui::Rect::from_min_size(outer.min, Vec2::new(outer.width(), HEADER_HEIGHT));
    draw_header(&painter, header, actor, &tracks, theme);
    let (visibility, legend) = draw_legend_row(ui, outer, header, &curves, theme);
    let ruler = egui::Rect::from_min_size(
        Pos2::new(legend.left(), legend.bottom() + PLOT_PADDING),
        Vec2::new(legend.width(), RULER_HEIGHT),
    );
    let plot = egui::Rect::from_min_max(
        Pos2::new(ruler.left(), ruler.bottom() + 4.0),
        Pos2::new(outer.right() - PLOT_PADDING, outer.bottom() - PLOT_PADDING),
    );
    if plot.width() <= 0.0 || plot.height() <= 0.0 {
        return;
    }
    let visible: Vec<_> =
        curves.iter().filter(|c| *visibility.get(&c.label).unwrap_or(&true)).collect();
    if visible.is_empty() {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            "No visible keyframed curves · changes are made in code",
            TextRole::BodyS.font_id(),
            theme.palette.text.muted,
        );
        return;
    }
    let duration = ctx.scene_duration_s.max(0.1);
    let (min, max) = curve_plot::value_window(&visible);
    let axis = Axis::new(plot, duration, min, max);
    draw_ruler(&painter, ruler, &axis, duration, theme);
    let ruler_response =
        ui.interact(ruler, ui.scope_id().with("review_ruler"), Sense::click_and_drag());
    if (ruler_response.clicked() || ruler_response.dragged())
        && let Some(pos) = ruler_response.interact_pointer_pos()
    {
        ctx.commands.push_back(
            Command::ScrubTo(global_seek(axis.time_at_x(pos.x), duration, ctx.scene_offset_s))
                .into(),
        );
    }
    ruler_response
        .on_hover_text("Scene-local seconds · drag to seek preview. Curves are read-only.");
    let painter = painter.with_clip_rect(plot.intersect(ui.clip_rect()));
    curve_plot::draw_value_grid(&painter, &axis, theme);
    let current = ctx.preview.playback.current_time_s() - ctx.scene_offset_s;
    for curve in visible {
        curve_plot::draw_curve(&painter, &axis, curve, current, theme);
        for (index, (time, value)) in curve.points.iter().enumerate() {
            let position = axis.point(*time, *value);
            painter.circle_filled(position, 3.0, curve.color);
            ui.interact(egui::Rect::from_center_size(position, Vec2::splat(10.0)).intersect(plot),
                ui.scope_id().with(("review_point", &curve.label, index)), Sense::hover())
                .on_hover_text(format!("{} · {}\n{time:.3}s scene / {:.3}s global · {value:.3}\nRead-only · edit the DSL to change this curve", curve.actor, curve.base_label, time + ctx.scene_offset_s));
        }
    }
}

fn global_seek(local: f64, duration: f64, offset: f64) -> f64 {
    local.clamp(0.0, duration) + offset
}
fn collect_actor_curves(
    timeline: &Timeline,
    tracks: &[(&str, EntityId)],
    theme: Theme,
    qualify: bool,
) -> Vec<CurveChannel> {
    let mut curves: Vec<CurveChannel> = Vec::new();
    for (index, (label, id)) in tracks.iter().enumerate() {
        // The first actor keeps the canonical channel colours; the rest get a
        // deterministic shade so two actors' `position.X` stay told apart.
        let shade = if index == 0 {
            1.0
        } else {
            curve_plot::actor_shade_factor(label)
        };
        curves.extend(curve_plot::collect_curves_for_actor(
            timeline, label, *id, theme, qualify, shade,
        ));
    }
    curves
}

fn draw_header(
    painter: &egui::Painter,
    header_rect: egui::Rect,
    actor: &str,
    tracks: &[(&str, EntityId)],
    theme: Theme,
) {
    let header = if tracks.len() > 1 {
        let names: Vec<&str> = tracks.iter().map(|(label, _)| *label).collect();
        format!("Read-only: {}  ({} actors)", names.join(", "), tracks.len())
    } else {
        format!("Read-only: {actor}")
    };
    painter.text(
        Pos2::new(header_rect.left() + PLOT_PADDING, header_rect.center().y),
        egui::Align2::LEFT_CENTER,
        header,
        TextRole::BodyS.font_id(),
        theme.palette.text.secondary,
    );
}

fn draw_legend_row(
    ui: &mut egui::Ui,
    outer: egui::Rect,
    header_rect: egui::Rect,
    curves: &[CurveChannel],
    theme: Theme,
) -> (HashMap<String, bool>, egui::Rect) {
    let visibility_id = ui.scope_id().with("curves_visibility");
    let mut visibility = curve_plot::legend_visibility(ui, visibility_id, curves);
    let legend_rect = egui::Rect::from_min_size(
        Pos2::new(outer.left() + PLOT_PADDING, header_rect.bottom()),
        Vec2::new(outer.width() - PLOT_PADDING * 2.0, LEGEND_HEIGHT),
    );
    curve_plot::draw_legend(ui, legend_rect, curves, &mut visibility, theme);
    ui.data_mut(|d| d.insert_temp(visibility_id, visibility.clone()));
    (visibility, legend_rect)
}

fn draw_ruler(
    painter: &egui::Painter,
    rect: egui::Rect,
    axis: &Axis,
    duration_s: f64,
    theme: Theme,
) {
    painter.rect_filled(rect, RADIUS_S, theme.palette.surface.surface);
    let tick_step = if duration_s <= 2.0 {
        0.25
    } else if duration_s <= 5.0 {
        0.5
    } else if duration_s <= 15.0 {
        1.0
    } else if duration_s <= 45.0 {
        5.0
    } else {
        10.0
    };
    let mut t = 0.0f64;
    while t <= duration_s {
        let x = axis.x(t);
        painter.line_segment(
            [
                Pos2::new(x, rect.bottom() - 6.0),
                Pos2::new(x, rect.bottom()),
            ],
            Stroke::new(STROKE_WIDTH, theme.palette.border.default),
        );
        painter.text(
            Pos2::new(x, rect.top() + rect.height() * 0.35),
            egui::Align2::CENTER_CENTER,
            if tick_step >= 1.0 {
                format!("{t:.0}s")
            } else {
                format!("{t:.1}s")
            },
            TextRole::Micro.font_id(),
            theme.palette.text.muted,
        );
        t += tick_step;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curve_seek_maps_scene_local_time_to_composition_time() {
        assert_eq!(global_seek(2.0, 5.0, 8.0), 10.0);
        assert_eq!(global_seek(-2.0, 5.0, 8.0), 8.0);
        assert_eq!(global_seek(9.0, 5.0, 8.0), 13.0);
    }
    #[test]
    fn review_collects_curves_for_all_selected_actors() {
        let timeline = curve_plot::two_actor_timeline();
        let tracks: Vec<_> = ["a", "b"]
            .iter()
            .map(|label| (*label, timeline.get_entity_id(label).unwrap()))
            .collect();
        let curves = collect_actor_curves(&timeline, &tracks, Theme::default(), true);
        assert!(curves.iter().any(|c| c.actor == "a"));
        assert!(curves.iter().any(|c| c.actor == "b"));
    }
}
