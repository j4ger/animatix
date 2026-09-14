//! Interactive Curves (F-curve) editor for the bottom tab group.
//!
//! The Inspector's graph editor is read-only; this panel is the editable
//! sibling. It graphs the keyframed properties of the currently selected
//! actor and supports:
//!
//! - horizontal keyframe drag → retime (one batched `MoveKeyframes`)
//! - vertical keyframe drag → value write-back (`SetKeyframeValue`)
//! - click / Shift+click selection, mirrored into the shared keyframe store
//! - right-click → easing submenu
//! - a scrub ruler plus playhead, like the timeline
//!
//! Sampling and axis mapping are shared with the Inspector through
//! [`crate::app::panels::curve_plot`]; this module owns only the interaction.

use std::collections::HashSet;

use animatix::timeline::{ActorField, AnimationTrack, Timeline};
use egui::{Pos2, Response, Sense, Stroke, Vec2};
use eparts::Theme;
use eparts::widget::UiExt;

use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command, MoveKeyframeSpec, PlaybackCommand, ShellAction};
use crate::app::components::layout;
use crate::app::design_tokens::spatial::timeline::RULER_HEIGHT;
use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_S, STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::timeline_diff::KeyframeId;
use crate::app::panels::curve_plot::{self, Axis, CurveChannel};

/// Pointer travel before a keyframe drag commits to an axis.
const DRAG_AXIS_THRESHOLD: f32 = 3.0;
/// Minimum time delta (seconds) worth emitting as a retime.
const MIN_TIME_DELTA_S: f64 = 0.01;
/// Click radius around a keyframe dot.
const KF_HIT_RADIUS: f32 = 7.0;

const HEADER_HEIGHT: f32 = 24.0;
const LEGEND_HEIGHT: f32 = 20.0;
const PLOT_PADDING: f32 = 8.0;

pub(crate) struct CurvesContext<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    /// Active scene name, or `None` for a single-scene document.
    pub active_scene: Option<&'a str>,
    pub selected_actors: &'a HashSet<String>,
    /// Canonical keyframe selection, shared with the timeline.
    pub selected_keyframes: &'a mut Vec<KeyframeId>,
    pub commands: &'a mut ActionQueue,
    pub snap_fps: f32,
    /// Set when this panel is focused; also gates the Delete shortcut so it
    /// removes keyframes rather than actors while the bottom region is active.
    pub timeline_focused: &'a mut bool,
}

/// Which axis a keyframe drag committed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Time,
    Value,
}

/// Vertical drag payload: the single channel being rewritten.
#[derive(Debug, Clone)]
struct ValueEdit {
    id: KeyframeId,
    field: ActorField,
    channel: usize,
    /// Value sampled at drag start, used to suppress a no-op write.
    original_value: f32,
    current_value: f32,
}

/// In-flight keyframe drag.
#[derive(Debug, Clone)]
struct CurveDrag {
    /// Selection moved by a retime drag.
    ids: Vec<KeyframeId>,
    /// Original time of the grabbed keyframe (seconds).
    anchor_time_s: f64,
    /// Snapped target time of the grabbed keyframe (seconds).
    target_time_s: f64,
    /// Accumulated pointer travel, used to pick the axis once.
    accum: Vec2,
    mode: Option<DragMode>,
    value_edit: Option<ValueEdit>,
}

impl CurveDrag {
    fn time_delta_s(&self) -> f64 {
        self.target_time_s - self.anchor_time_s
    }
}

/// Render the interactive curves editor.
pub(crate) fn curves_panel_ui(ctx: &mut CurvesContext<'_>, ui: &mut egui::Ui) {
    let theme = eparts::theme(ui);
    let sp = spatial(ui);
    let outer = ui.available_rect_before_wrap();

    // Focus the panel on click, not hover (mirrors the timeline): clicking the
    // curves editor focuses it and clicking elsewhere releases it.
    if ui.input(|i| i.pointer.primary_pressed()) {
        *ctx.timeline_focused = ui.rect_contains_pointer(outer);
    }
    if *ctx.timeline_focused {
        let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with("curves_focus_ring"));
        ui.ctx().layer_painter(layer).rect_stroke(
            outer.shrink(1.0),
            egui::CornerRadius::ZERO,
            Stroke::new(STROKE_WIDTH, theme.palette.border.focus),
            egui::StrokeKind::Inside,
        );
    }

    let Some(timeline) = ctx.timeline else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::CHART_LINE,
            "No timeline loaded",
            "Open or create a scene to edit curves",
        );
        return;
    };

    // Every selected actor is graphed, sorted for deterministic colours and
    // legend order. Edits address each curve's own actor, so the shared
    // keyframe selection works across actors.
    let mut actors: Vec<&String> = ctx.selected_actors.iter().collect();
    actors.sort();
    if actors.is_empty() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::CURSOR_CLICK,
            "Select an actor to edit curves",
            "Click an actor in the preview or timeline",
        );
        return;
    }

    let tracks: Vec<(&str, &AnimationTrack)> = actors
        .iter()
        .filter_map(|label| timeline.get_track(label).map(|track| (label.as_str(), track)))
        .collect();
    let Some(actor) = tracks.first().map(|(label, _)| *label) else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::CHART_LINE,
            "No track for these actors",
            "The actors have no animation data in this scene",
        );
        return;
    };
    // Actor qualification is what keeps labels — and therefore legend keys and
    // per-keyframe widget ids — unique once two actors are graphed.
    let qualify = tracks.len() > 1;

    // Reserve the pane so the docked region keeps its size, then paint into
    // the full rect (egui_tiles gives the pane exactly this space).
    ui.allocate_rect(outer, Sense::hover());
    let painter = ui.painter_at(outer);
    painter.rect_filled(outer, 0.0, theme.palette.surface.base);

    let scene = ctx.active_scene.map(ToOwned::to_owned);
    let mut curves: Vec<CurveChannel> = Vec::new();
    for (index, (label, track)) in tracks.iter().enumerate() {
        // The first actor keeps the canonical channel colours; the rest get a
        // deterministic shade so two actors' `position.X` stay told apart.
        let shade = if index == 0 {
            1.0
        } else {
            curve_plot::actor_shade_factor(label)
        };
        curves.extend(curve_plot::collect_curves_for_actor(label, track, theme, qualify, shade));
    }

    // The shared keyframe selection is the single source of truth, so a
    // timeline selection carries over when switching tabs and vice versa.
    let mut selection: Vec<KeyframeId> = ctx.selected_keyframes.clone();
    let selection_before = selection.clone();

    // ── Header ──────────────────────────────────────────────────────────
    let header_rect = egui::Rect::from_min_size(outer.min, Vec2::new(outer.width(), HEADER_HEIGHT));
    let header = if tracks.len() > 1 {
        let names: Vec<&str> = tracks.iter().map(|(label, _)| *label).collect();
        format!("Editing: {}  ({} actors)", names.join(", "), tracks.len())
    } else {
        format!("Editing: {actor}")
    };
    painter.text(
        Pos2::new(header_rect.left() + PLOT_PADDING, header_rect.center().y),
        egui::Align2::LEFT_CENTER,
        header,
        TextRole::BodyS.font_id(),
        theme.palette.text.secondary,
    );
    if curves.is_empty() {
        painter.text(
            egui::Rect::from_min_max(Pos2::new(outer.left(), header_rect.bottom()), outer.max)
                .center(),
            egui::Align2::CENTER_CENTER,
            "No keyframed properties to graph",
            TextRole::BodyS.font_id(),
            theme.palette.text.muted,
        );
        return;
    }

    // ── Legend ──────────────────────────────────────────────────────────
    let visibility_id = ui.id().with("curves_visibility");
    let mut visibility = curve_plot::legend_visibility(ui, visibility_id, &curves);
    let legend_rect = egui::Rect::from_min_size(
        Pos2::new(outer.left() + PLOT_PADDING, header_rect.bottom()),
        Vec2::new(outer.width() - PLOT_PADDING * 2.0, LEGEND_HEIGHT),
    );
    curve_plot::draw_legend(ui, legend_rect, &curves, &mut visibility, theme);
    ui.data_mut(|d| d.insert_temp(visibility_id, visibility.clone()));

    // ── Ruler (scrub) ───────────────────────────────────────────────────
    let ruler_rect = egui::Rect::from_min_size(
        Pos2::new(legend_rect.left(), legend_rect.bottom() + PLOT_PADDING),
        Vec2::new(legend_rect.width(), RULER_HEIGHT),
    );
    // ── Plot ────────────────────────────────────────────────────────────
    let plot_rect = egui::Rect::from_min_max(
        Pos2::new(ruler_rect.left(), ruler_rect.bottom() + sp.base.space_2),
        Pos2::new(outer.right() - PLOT_PADDING, outer.bottom() - PLOT_PADDING),
    );

    let visible_curves: Vec<&CurveChannel> =
        curves.iter().filter(|c| *visibility.get(&c.label).unwrap_or(&true)).collect();
    if visible_curves.is_empty() {
        painter.text(
            plot_rect.center(),
            egui::Align2::CENTER_CENTER,
            "All curves hidden",
            TextRole::BodyS.font_id(),
            theme.palette.text.muted,
        );
        return;
    }

    painter.rect_filled(plot_rect, RADIUS_M, theme.palette.surface.surface);
    painter.rect_stroke(
        plot_rect,
        RADIUS_M,
        Stroke::new(STROKE_WIDTH, theme.palette.border.default),
        egui::StrokeKind::Inside,
    );

    let duration_s = ctx.preview.playback.duration_s.max(0.1);
    let (min_val, max_val) = curve_plot::value_window(&visible_curves);
    let axis = Axis::new(plot_rect, duration_s, min_val, max_val);
    let current_time_s = ctx.preview.playback.current_time_s();

    curve_plot::draw_value_grid(&painter, &axis, theme);

    // ── Drag state ──────────────────────────────────────────────────────
    let drag_id = ui.id().with("curves_kf_drag");
    let drag: Option<CurveDrag> = ui.data(|d| d.get_temp(drag_id));
    let shift_held = ui.input(|i| i.modifiers.shift);
    let mut new_drag = drag.clone();

    // Draw curves and collect keyframe hit responses in one pass over each
    // channel so the dots sit above the lines.
    let mut hit_responses: Vec<(KeyframeId, ActorField, usize, f32, Response, Pos2)> = Vec::new();
    for curve in &visible_curves {
        curve_plot::draw_curve(&painter, &axis, curve, current_time_s, theme);

        for (i, (time_s, val)) in curve.points.iter().enumerate() {
            let time_ms = (*time_s * 1000.0).round() as u64;
            let id = KeyframeId {
                scene: scene.clone(),
                actor: curve.actor.clone(),
                property: curve.property.to_string(),
                time_ms,
            };

            // Ghost the in-flight drag so the point tracks the pointer.
            let is_time_dragged = new_drag
                .as_ref()
                .is_some_and(|d| d.mode == Some(DragMode::Time) && d.ids.contains(&id));
            let value_dragged = new_drag.as_ref().is_some_and(|d| {
                d.mode == Some(DragMode::Value)
                    && d.value_edit
                        .as_ref()
                        .is_some_and(|v| v.id == id && v.channel == curve.channel)
            });
            let draw_pos = if is_time_dragged {
                let delta = new_drag.as_ref().map_or(0.0, CurveDrag::time_delta_s);
                axis.point(*time_s + delta, *val)
            } else if value_dragged {
                let v = new_drag
                    .as_ref()
                    .and_then(|d| d.value_edit.as_ref())
                    .map_or(*val, |v| v.current_value);
                axis.point(*time_s, v)
            } else {
                axis.point(*time_s, *val)
            };

            let selected = selection.iter().any(|s| s == &id);
            let hit_rect = egui::Rect::from_center_size(draw_pos, Vec2::splat(KF_HIT_RADIUS * 2.0));
            let resp = ui.interact(
                hit_rect,
                ui.id().with(("curves_kf", curve.label.as_str(), i)),
                Sense::click_and_drag(),
            );

            let dot_color = if resp.hovered() || selected || is_time_dragged || value_dragged {
                theme.palette.accent.primary
            } else {
                curve.color
            };
            let radius = if selected { 5.0 } else { 4.0 };
            painter.circle_filled(draw_pos, radius, dot_color);
            if selected {
                painter.circle_stroke(
                    draw_pos,
                    radius + 2.0,
                    Stroke::new(STROKE_WIDTH, theme.palette.text.primary),
                );
            }

            hit_responses.push((id, curve.field, curve.channel, *val, resp, draw_pos));
        }
    }

    // ── Playhead ────────────────────────────────────────────────────────
    curve_plot::draw_playhead(&painter, &axis, current_time_s, theme);

    // ── Ruler interaction ───────────────────────────────────────────────
    draw_ruler(&painter, ruler_rect, &axis, duration_s, theme);
    let ruler_resp = ui.interact(ruler_rect, ui.id().with("curves_ruler"), Sense::click_and_drag());
    if ruler_resp.clicked() || ruler_resp.dragged() {
        if let Some(pos) = ruler_resp.interact_pointer_pos() {
            let mut t = axis.time_at_x(pos.x);
            if !shift_held {
                t = snap_time(t, ctx.snap_fps);
            }
            ctx.commands.push_back(PlaybackCommand::ScrubTo(t).into());
        }
    }

    // ── Keyframe interactions ───────────────────────────────────────────
    for (id, field, channel, point_value, resp, _draw_pos) in &hit_responses {
        let is_dragged = new_drag.as_ref().is_some_and(|d| {
            d.mode.is_some()
                && (d.ids.contains(id) || d.value_edit.as_ref().is_some_and(|v| v.id == *id))
        });

        if resp.hovered() && !is_dragged {
            let time_s = id.time_ms as f64 / 1000.0;
            crate::app::components::text_tooltip(
                ui,
                resp.id.with("curves_tip"),
                resp,
                &format!("{}.{} @ {:.2}s", id.property, channel_label(*channel), time_s),
            );
        }

        resp.context_menu(|ui| {
            ui.set_min_width(140.0);
            ui.strong(format!("{} @ {:.2}s", id.property, id.time_ms as f64 / 1000.0));
            ui.separator();
            // Edit the whole selection when the grabbed keyframe is part of it,
            // otherwise this keyframe alone.
            let targets: Vec<KeyframeId> = if selection.contains(id) {
                selection.clone()
            } else {
                vec![id.clone()]
            };
            ui.menu_button("Easing", |ui| {
                for &(id_str, display_name) in animatix_syntax::easing::EASING_REGISTRY {
                    if ui.stable_selectable_label(false, display_name).clicked() {
                        let variant = animatix_syntax::easing::parse_easing_name(id_str)
                            .unwrap_or(animatix_syntax::easing::Easing::Linear);
                        for target in &targets {
                            ctx.commands.push_back(ShellAction::Command(
                                Command::SetKeyframeEasing {
                                    scene: target.scene.clone(),
                                    actor: target.actor.clone(),
                                    property: target.property.clone(),
                                    time_s: target.time_ms as f64 / 1000.0,
                                    easing: variant,
                                },
                            ));
                        }
                        ui.close();
                    }
                }
            });
            ui.separator();
            if ui
                .button(format!("{} Delete keyframe", egui_phosphor::regular::TRASH))
                .clicked()
            {
                ctx.commands
                    .push_back(ShellAction::Command(Command::DeleteKeyframes(targets.clone())));
                ui.close();
            }
        });

        if resp.clicked() {
            if shift_held {
                if let Some(p) = selection.iter().position(|s| s == id) {
                    selection.remove(p);
                } else {
                    selection.push(id.clone());
                }
            } else {
                selection = vec![id.clone()];
            }
        }

        if resp.drag_started() {
            // Dragging a selected keyframe moves the whole selection;
            // dragging an unselected one starts a fresh single selection.
            if !shift_held && !selection.contains(id) {
                selection = vec![id.clone()];
            }
            let ids = if selection.contains(id) {
                selection.clone()
            } else {
                vec![id.clone()]
            };
            let time_s = id.time_ms as f64 / 1000.0;
            new_drag = Some(CurveDrag {
                ids,
                anchor_time_s: time_s,
                target_time_s: time_s,
                accum: Vec2::ZERO,
                mode: None,
                value_edit: Some(ValueEdit {
                    id: id.clone(),
                    field: *field,
                    channel: *channel,
                    original_value: *point_value,
                    current_value: *point_value,
                }),
            });
        }

        if resp.dragged() {
            if let (Some(d), Some(pos)) = (new_drag.as_mut(), resp.interact_pointer_pos()) {
                d.accum += resp.drag_delta();
                if d.mode.is_none() && d.accum.length() > DRAG_AXIS_THRESHOLD {
                    d.mode = Some(if d.accum.x.abs() >= d.accum.y.abs() {
                        DragMode::Time
                    } else {
                        DragMode::Value
                    });
                }
                match d.mode {
                    Some(DragMode::Time) => {
                        let t = axis.time_at_x(pos.x);
                        d.target_time_s = if shift_held {
                            t
                        } else {
                            snap_time(t, ctx.snap_fps)
                        };
                    },
                    Some(DragMode::Value) => {
                        if let Some(v) = d.value_edit.as_mut() {
                            v.current_value = axis.value_at_y(pos.y);
                        }
                    },
                    None => {},
                }
            }
        }

        if resp.drag_stopped() {
            if let Some(d) = new_drag.take() {
                emit_drag_commands(ctx.commands, &d, &tracks, &scene);
            }
        }
    }

    // ── Persist drag state and mirror selection into the shared store ───
    ui.data_mut(|d| {
        if let Some(drag) = new_drag.clone() {
            d.insert_temp(drag_id, drag);
        } else {
            d.remove::<CurveDrag>(drag_id);
        }
    });
    if selection != selection_before {
        let canonical = prune_selection(&selection, &curves, &scene);
        *ctx.selected_keyframes = canonical.clone();
        ctx.commands
            .push_back(ShellAction::Command(Command::SetSelectedKeyframes(canonical)));
    }
}

/// Emit the undoable command(s) for a completed drag.
fn emit_drag_commands(
    commands: &mut ActionQueue,
    drag: &CurveDrag,
    tracks: &[(&str, &AnimationTrack)],
    scene: &Option<String>,
) {
    // A drag can carry ids from several actors; each edit reads the value out
    // of the track that owns it.
    let track_for =
        |actor: &str| tracks.iter().find(|(label, _)| *label == actor).map(|(_, track)| *track);
    match drag.mode {
        Some(DragMode::Time) => {
            let delta = drag.time_delta_s();
            if delta.abs() <= MIN_TIME_DELTA_S {
                return;
            }
            let specs: Vec<MoveKeyframeSpec> = drag
                .ids
                .iter()
                .map(|id| {
                    let old_time_s = id.time_ms as f64 / 1000.0;
                    MoveKeyframeSpec {
                        scene: id.scene.clone(),
                        actor: id.actor.clone(),
                        property: id.property.clone(),
                        old_time_s,
                        new_time_s: old_time_s + delta,
                    }
                })
                .collect();
            tracing::debug!(
                scene = ?scene,
                count = specs.len(),
                delta_s = delta,
                "retiming keyframe selection from curves panel"
            );
            commands.push_back(ShellAction::Command(Command::MoveKeyframes(specs)));
        },
        Some(DragMode::Value) => {
            let Some(edit) = drag.value_edit.as_ref() else {
                return;
            };
            if (edit.current_value - edit.original_value).abs() <= f32::EPSILON {
                return;
            }
            let time_ms = edit.id.time_ms;
            let Some(track) = track_for(&edit.id.actor) else {
                tracing::warn!(
                    actor = %edit.id.actor,
                    "curves value drag: no track for actor; skipping edit"
                );
                return;
            };
            let Some(mut value) =
                animatix::timeline::read_property_value(track, edit.field, time_ms)
            else {
                tracing::warn!(
                    actor = %edit.id.actor,
                    property = %edit.id.property,
                    time_ms,
                    "curves value drag: no value at keyframe; skipping edit"
                );
                return;
            };
            if !curve_plot::set_channel(&mut value, edit.channel, edit.current_value) {
                tracing::warn!(
                    actor = %edit.id.actor,
                    property = %edit.id.property,
                    channel = edit.channel,
                    value = ?value,
                    "curves value drag: channel not writable for this value type; skipping edit"
                );
                return;
            }
            tracing::debug!(
                actor = %edit.id.actor,
                property = %edit.id.property,
                time_s = edit.id.time_ms as f64 / 1000.0,
                "setting keyframe value from curves panel"
            );
            commands.push_back(ShellAction::Command(Command::SetKeyframeValue {
                scene: edit.id.scene.clone(),
                actor: edit.id.actor.clone(),
                property: edit.id.property.clone(),
                time_s: time_ms as f64 / 1000.0,
                value,
            }));
        },
        None => {},
    }
}

/// Drop selected ids that no longer correspond to a drawn keyframe.
fn prune_selection(
    selection: &[KeyframeId],
    curves: &[CurveChannel],
    scene: &Option<String>,
) -> Vec<KeyframeId> {
    selection
        .iter()
        .filter(|id| {
            id.scene == *scene
                && curves.iter().any(|c| {
                    c.actor == id.actor
                        && c.property == id.property
                        && c.points.iter().any(|(t, _)| (*t * 1000.0).round() as u64 == id.time_ms)
                })
        })
        .cloned()
        .collect()
}

fn snap_time(time_s: f64, snap_fps: f32) -> f64 {
    let fps = snap_fps.max(1.0) as f64;
    (time_s * fps).round() / fps
}

fn channel_label(channel: usize) -> &'static str {
    match channel {
        0 => "0",
        1 => "1",
        2 => "2",
        3 => "3",
        _ => "?",
    }
}

/// Paint the tick marks and time labels of the scrub ruler.
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
    use crate::app::panels::curve_plot::{CurveChannel, collect_curves_for_actor};

    /// Two `Rect`s animating the same property — the multi-actor fixture.
    fn two_actor_timeline() -> Timeline {
        let source = r#"
#0s
a: Rect, at: (0, 0), size: (20, 20), color: (1, 1, 1, 1)
b: Rect, at: (10, 10), size: (20, 20), color: (1, 1, 1, 1)

#0.5s
a.at = (100, 0)
b.at = (0, 100)

#1s
a.at = (200, 0)
b.at = (0, 200)
"#;
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        Timeline::build(&ast.expect("ast"))
    }

    fn id_at(curve: &CurveChannel, point: usize) -> KeyframeId {
        KeyframeId {
            scene: None,
            actor: curve.actor.clone(),
            property: curve.property.to_string(),
            time_ms: (curve.points[point].0 * 1000.0).round() as u64,
        }
    }

    /// The shared keyframe selection spans actors, so pruning must keep every
    /// id that still has a drawn keyframe — the single-actor filter used to
    /// drop the other actors' ids on the first edit.
    #[test]
    fn prune_selection_keeps_ids_from_every_graphed_actor() {
        let timeline = two_actor_timeline();
        let theme = eparts::Theme::default();
        let mut curves: Vec<CurveChannel> = Vec::new();
        for actor in ["a", "b"] {
            let track = timeline.get_track(actor).expect("track");
            curves.extend(collect_curves_for_actor(actor, track, theme, true, 1.0));
        }
        assert!(curves.len() >= 2, "both actors must contribute channels");

        let a_curve = curves.iter().find(|c| c.actor == "a").expect("a curve");
        let b_curve = curves.iter().find(|c| c.actor == "b").expect("b curve");
        let mut selection = vec![id_at(a_curve, 1), id_at(b_curve, 1)];
        selection.push(KeyframeId {
            scene: None,
            actor: "not_graph".to_string(),
            property: a_curve.property.to_string(),
            time_ms: id_at(a_curve, 1).time_ms,
        });

        let pruned = prune_selection(&selection, &curves, &None);
        assert_eq!(pruned.len(), 2, "each graphed actor keeps its id: {pruned:?}");
        assert!(pruned.iter().any(|id| id.actor == "a"));
        assert!(pruned.iter().any(|id| id.actor == "b"));

        // A time that is not a keyframe of that actor is dropped.
        let mut stale = vec![id_at(a_curve, 1)];
        stale[0].time_ms += 7;
        assert!(prune_selection(&stale, &curves, &None).is_empty());
    }
}
