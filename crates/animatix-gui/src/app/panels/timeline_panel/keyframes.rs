//! Keyframe diamond rendering, multi-select state, dragging, and context menus.

use animatix::timeline::AnimationTrack;
use egui::{Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use eparts::widget::UiExt;

use super::TimelineCoords;
use super::ruler::snap_to_speech_or_fps;
use super::track_lanes::{PropertyGroup, property_group_color};
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command, MoveKeyframeSpec, ShellAction};
use crate::app::components::button::Button;
use crate::app::components::text_tooltip;
use crate::app::design_tokens::semantic::timeline;
use crate::app::design_tokens::spatial::timeline::KF_HALF as KF_DIAMOND_HALF;
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH};
use crate::app::document::timeline_diff::{KeyframeId, collect_per_property_keyframes};

/// In-flight keyframe drag.
///
/// `anchor` is the grabbed diamond's scene-local identity; `target_s` is the
/// snapped time it currently points at. `ids` is the selection snapshot taken
/// at drag start — the whole selection when the grabbed diamond was already
/// selected, otherwise just the grabbed diamond's identities.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KfDrag {
    pub(crate) scene: Option<String>,
    pub(crate) actor: String,
    /// Property of the grabbed diamond (the first coincident property for an
    /// aggregate diamond).
    pub(crate) property: String,
    /// Original scene-local keyframe time of the grabbed diamond, in ms.
    pub(crate) anchor_keyframe_ms: u64,
    /// Original scene-local time of the grabbed diamond, in seconds.
    pub(crate) anchor_s: f64,
    /// Snapped target time of the grabbed diamond, in seconds.
    pub(crate) target_s: f64,
    /// Selection snapshot moved as one undoable step.
    pub(crate) ids: Vec<KeyframeId>,
}

impl KfDrag {
    /// Current offset applied to every dragged keyframe, in seconds.
    pub(crate) fn delta_s(&self) -> f64 {
        self.target_s - self.anchor_s
    }

    pub(crate) fn involves(&self, actor: &str, property: Option<&str>, time_ms: u64) -> bool {
        self.ids.iter().any(|id| {
            id.actor == actor
                && id.time_ms == time_ms
                && property.is_none_or(|property| id.property == property)
        })
    }
}

/// Offset (seconds) a drag applies to a specific keyframe, if any.
pub(crate) fn drag_delta_for(
    drag: Option<&KfDrag>,
    actor: &str,
    property: Option<&str>,
    time_ms: u64,
) -> Option<f64> {
    drag.filter(|drag| drag.involves(actor, property, time_ms)).map(KfDrag::delta_s)
}

/// Emit one batched [`Command::MoveKeyframes`] for every keyframe the drag
/// covers, offset by the drag delta. No-op for an unmoved drag.
pub(crate) fn emit_keyframe_moves(commands: &mut ActionQueue, drag: Option<&KfDrag>) {
    let Some(drag) = drag else {
        return;
    };
    let delta = drag.delta_s();
    if delta.abs() <= 0.01 {
        return;
    }
    tracing::debug!(
        actor = %drag.actor,
        property = %drag.property,
        scene = ?drag.scene,
        anchor_ms = drag.anchor_keyframe_ms,
        delta_s = delta,
        count = drag.ids.len(),
        "moving keyframe selection"
    );
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
    commands.push_back(ShellAction::Command(Command::MoveKeyframes(specs)));
}

/// Group an actor's keyframes by time, preserving every property keyed at that
/// time.
pub(crate) fn collect_actor_keyframe_groups(
    track: &AnimationTrack,
    shape: &animatix::timeline::animation_track::ShapeTracks,
    text: &animatix::timeline::animation_track::TextTracks,
) -> Vec<(u64, Vec<String>)> {
    let mut by_time: std::collections::BTreeMap<u64, Vec<String>> =
        std::collections::BTreeMap::new();
    for (property, times) in collect_per_property_keyframes(track, shape, text, None) {
        for time_ms in times {
            by_time.entry(time_ms).or_default().push(property.clone());
        }
    }
    by_time.into_iter().collect()
}

/// Is `time_ms`'s aggregate diamond (any property) selected?
pub(crate) fn aggregate_selected(
    multi: &[KeyframeId],
    active_scene: Option<&str>,
    actor: &str,
    time_ms: u64,
) -> bool {
    multi
        .iter()
        .any(|id| id.scene.as_deref() == active_scene && id.actor == actor && id.time_ms == time_ms)
}

/// Is this exact property keyframe selected?
pub(crate) fn keyframe_selected(
    multi: &[KeyframeId],
    active_scene: Option<&str>,
    actor: &str,
    property: &str,
    time_ms: u64,
) -> bool {
    multi.iter().any(|id| {
        id.scene.as_deref() == active_scene
            && id.actor == actor
            && id.property == property
            && id.time_ms == time_ms
    })
}

/// Context bundle for rendering keyframe diamonds and handling drag/selection interactions.
pub(crate) struct KeyframeRenderCtx<'a> {
    pub coords: &'a TimelineCoords,
    pub preview: &'a PreviewPaneState,
    pub active_scene: Option<&'a str>,
    pub scene_for_selection: Option<String>,
    pub multi_selected: &'a mut Vec<KeyframeId>,
    pub kf_drag: Option<&'a KfDrag>,
    pub new_kf_drag: &'a mut Option<KfDrag>,
    pub commands: &'a mut ActionQueue,
    pub shift_held: bool,
    pub snap_fps: f32,
    pub speech_snap_targets: &'a [(f64, String)],
    pub theme: eparts::Theme,
}

/// Render aggregate keyframe diamonds for an actor track row.
pub(crate) fn render_actor_keyframes(
    ui: &mut egui::Ui,
    track: &AnimationTrack,
    shape: &animatix::timeline::animation_track::ShapeTracks,
    text: &animatix::timeline::animation_track::TextTracks,
    actor_label: &str,
    track_idx: usize,
    bar_area: Rect,
    ctx: &mut KeyframeRenderCtx<'_>,
) {
    let coords = ctx.coords;
    let preview = ctx.preview;
    let active_scene = ctx.active_scene;
    let scene_for_selection = &ctx.scene_for_selection;
    let multi_selected = &mut *ctx.multi_selected;
    let kf_drag = ctx.kf_drag;
    let new_kf_drag = &mut *ctx.new_kf_drag;
    let commands = &mut *ctx.commands;
    let shift_held = ctx.shift_held;
    let snap_fps = ctx.snap_fps;
    let speech_snap_targets = ctx.speech_snap_targets;
    let theme = ctx.theme;
    let painter = ui.painter();
    let kf_groups = collect_actor_keyframe_groups(track, shape, text);
    for (kf_ms, props) in kf_groups {
        let prop_label = props.join(", ");
        let drag_prop = props.first().cloned().unwrap_or_default();
        let kf_s = kf_ms as f64 / 1000.0;
        let kf_x = coords.time_to_x(kf_s);
        if kf_x < bar_area.left() || kf_x > bar_area.right() {
            continue;
        }
        let is_act = (kf_s - preview.playback.current_time_s()).abs() < 0.01;
        let is_ms = aggregate_selected(multi_selected, active_scene, actor_label, kf_ms);
        let is_drag = kf_drag.as_ref().is_some_and(|drag| drag.involves(actor_label, None, kf_ms));
        let is_flashed =
            preview.flashed_keyframe_times.iter().any(|(t, _)| (*t - kf_s).abs() < 0.001);
        let ds = if is_flashed {
            KF_DIAMOND_HALF * 2.0
        } else if is_drag {
            KF_DIAMOND_HALF * 1.5
        } else {
            KF_DIAMOND_HALF
        };
        let kc = if is_flashed {
            timeline::KF_FLASH
        } else if is_ms {
            theme.palette.accent.primary
        } else if is_act {
            theme.palette.text.primary
        } else {
            theme.palette.status.warning
        };
        let cy = bar_area.center().y;
        let draw_x = drag_delta_for(new_kf_drag.as_ref(), actor_label, None, kf_ms)
            .map_or(kf_x, |delta| coords.time_to_x(kf_s + delta));
        let hit_size = (ds * 2.5).max(8.0);
        let dr = Rect::from_center_size(Pos2::new(draw_x, cy), Vec2::new(hit_size, hit_size));
        let dresp = ui.interact(
            dr,
            ui.id().with(("kf_diamond", track_idx, kf_ms)),
            Sense::click_and_drag(),
        );
        if dresp.hovered() || is_ms || is_drag || is_act {
            painter.circle_filled(
                Pos2::new(draw_x, cy),
                ds * 1.8,
                Color32::from_rgba_unmultiplied(kc.r(), kc.g(), kc.b(), 45),
            );
        }
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(draw_x, cy - ds),
                Pos2::new(draw_x + ds, cy),
                Pos2::new(draw_x, cy + ds),
                Pos2::new(draw_x - ds, cy),
            ],
            if dresp.hovered() || is_drag {
                kc
            } else {
                kc.linear_multiply(0.85)
            },
            Stroke::new(0.5, Color32::from_rgba_unmultiplied(255, 255, 255, 70)),
        ));

        if dresp.hovered() && !is_drag {
            text_tooltip(
                ui,
                dresp.id.with("tooltip"),
                &dresp,
                &format!("{prop_label} @ {:.2}s", kf_s),
            );
        }

        dresp.context_menu(|ui| {
            ui.set_min_width(140.0);
            ui.strong(format!("{prop_label} @ {:.2}s", kf_s));
            ui.separator();
            let selected_at_time: Vec<KeyframeId> = multi_selected
                .iter()
                .filter(|id| {
                    id.scene.as_deref() == active_scene
                        && id.actor == actor_label
                        && id.time_ms == kf_ms
                })
                .cloned()
                .collect();
            let targets: Vec<KeyframeId> = if selected_at_time.is_empty() {
                props
                    .iter()
                    .map(|prop| KeyframeId {
                        scene: scene_for_selection.clone(),
                        actor: actor_label.to_string(),
                        property: (*prop).to_string(),
                        time_ms: kf_ms,
                    })
                    .collect()
            } else {
                selected_at_time
            };
            ui.menu_button("Easing", |ui| {
                for &(id_str, display_name) in animatix_syntax::easing::EASING_REGISTRY {
                    if ui.stable_selectable_label(false, display_name).clicked() {
                        let variant = animatix_syntax::easing::parse_easing_name(id_str)
                            .unwrap_or(animatix_syntax::easing::Easing::Linear);
                        for target in &targets {
                            commands.push_back(ShellAction::Command(Command::SetKeyframeEasing {
                                scene: target.scene.clone(),
                                actor: target.actor.clone(),
                                property: target.property.clone(),
                                time_s: target.time_ms as f64 / 1000.0,
                                easing: variant,
                            }));
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
                commands.push_back(ShellAction::Command(Command::DeleteKeyframes(targets.clone())));
                ui.close();
            }
        });

        if dresp.clicked() {
            let coincident: Vec<KeyframeId> = props
                .iter()
                .map(|prop| KeyframeId {
                    scene: scene_for_selection.clone(),
                    actor: actor_label.to_string(),
                    property: (*prop).to_string(),
                    time_ms: kf_ms,
                })
                .collect();
            if shift_held {
                for id in coincident {
                    if let Some(p) = multi_selected.iter().position(|s| *s == id) {
                        multi_selected.remove(p);
                    } else {
                        multi_selected.push(id);
                    }
                }
            } else {
                *multi_selected = coincident;
            }
        }
        if dresp.drag_started() {
            let coincident: Vec<KeyframeId> = props
                .iter()
                .map(|prop| KeyframeId {
                    scene: scene_for_selection.clone(),
                    actor: actor_label.to_string(),
                    property: (*prop).to_string(),
                    time_ms: kf_ms,
                })
                .collect();
            let grabbed_selected = coincident.iter().any(|id| {
                keyframe_selected(multi_selected, active_scene, actor_label, &id.property, kf_ms)
            });
            if !shift_held && !grabbed_selected {
                *multi_selected = coincident.clone();
            }
            let ids = if grabbed_selected {
                multi_selected.clone()
            } else {
                coincident
            };
            *new_kf_drag = Some(KfDrag {
                scene: scene_for_selection.clone(),
                actor: actor_label.to_string(),
                property: drag_prop.to_string(),
                anchor_keyframe_ms: kf_ms,
                anchor_s: kf_s,
                target_s: kf_s,
                ids,
            });
        }
        if is_drag {
            if let Some(pos) = dresp.interact_pointer_pos() {
                let nt = coords.x_to_time(pos.x);
                let (snapped, snap_label) = if shift_held {
                    (nt, None)
                } else {
                    snap_to_speech_or_fps(nt, snap_fps, speech_snap_targets)
                };
                if let Some(drag) = new_kf_drag.as_mut() {
                    drag.target_s = snapped;
                }
                let gx = coords.time_to_x(snapped);
                painter.line_segment(
                    [
                        Pos2::new(gx, bar_area.top()),
                        Pos2::new(gx, bar_area.bottom()),
                    ],
                    Stroke::new(STROKE_WIDTH, theme.palette.status.warning.linear_multiply(0.5)),
                );
                let label_text = if let Some(ref label) = snap_label {
                    format!("🧲 {} ({:.2}s)", label, snapped)
                } else {
                    format!("{:.2}s → {:.2}s", kf_s, snapped)
                };
                let g = painter.layout_no_wrap(
                    label_text,
                    FontId::monospace(10.0), // 10px mono: no TextRole
                    theme.palette.text.primary,
                );
                let tr = Rect::from_min_size(
                    Pos2::new(gx - g.size().x / 2.0, bar_area.top() - 16.0),
                    g.size() + Vec2::new(8.0, 4.0),
                );
                painter.rect_filled(tr, RADIUS_S, theme.palette.surface.surface);
                painter.galley(tr.min + Vec2::new(4.0, 2.0), g, theme.palette.text.primary);
            }
        }
        if dresp.drag_stopped() && is_drag {
            emit_keyframe_moves(commands, new_kf_drag.as_ref());
            *new_kf_drag = None;
        }
    }
}

/// Render keyframe diamonds for a specific property lane row.
pub(crate) fn render_property_keyframes(
    ui: &mut egui::Ui,
    kf_times: &[u64],
    actor_label: &str,
    prop_name: &str,
    track_idx: usize,
    prop_bar_area: Rect,
    group: Option<PropertyGroup>,
    ctx: &mut KeyframeRenderCtx<'_>,
) {
    let coords = ctx.coords;
    let preview = ctx.preview;
    let active_scene = ctx.active_scene;
    let scene_for_selection = &ctx.scene_for_selection;
    let multi_selected = &mut *ctx.multi_selected;
    let kf_drag = ctx.kf_drag;
    let new_kf_drag = &mut *ctx.new_kf_drag;
    let commands = &mut *ctx.commands;
    let shift_held = ctx.shift_held;
    let snap_fps = ctx.snap_fps;
    let speech_snap_targets = ctx.speech_snap_targets;
    let theme = ctx.theme;
    let painter = ui.painter();
    for kf_ms in kf_times {
        let kf_s = *kf_ms as f64 / 1000.0;
        let kf_x = coords.time_to_x(kf_s);
        if kf_x < prop_bar_area.left() || kf_x > prop_bar_area.right() {
            continue;
        }
        let is_act = (kf_s - preview.playback.current_time_s()).abs() < 0.01;
        let is_ms = keyframe_selected(multi_selected, active_scene, actor_label, prop_name, *kf_ms);
        let is_drag = kf_drag
            .as_ref()
            .is_some_and(|drag| drag.involves(actor_label, Some(prop_name), *kf_ms));
        let is_flashed =
            preview.flashed_keyframe_times.iter().any(|(t, _)| (*t - kf_s).abs() < 0.001);
        let ds = if is_flashed {
            KF_DIAMOND_HALF * 2.0
        } else if is_drag {
            KF_DIAMOND_HALF * 1.5
        } else {
            KF_DIAMOND_HALF
        };
        let base_color = group
            .map(|g| property_group_color(g, theme))
            .unwrap_or(theme.palette.status.warning);
        let kc = if is_flashed {
            timeline::KF_FLASH
        } else if is_ms {
            theme.palette.accent.primary
        } else if is_act {
            theme.palette.text.primary
        } else {
            base_color
        };
        let cy = prop_bar_area.center().y;
        let draw_x = drag_delta_for(new_kf_drag.as_ref(), actor_label, Some(prop_name), *kf_ms)
            .map_or(kf_x, |delta| coords.time_to_x(kf_s + delta));
        let hit_size = (ds * 2.5).max(8.0);
        let dr = Rect::from_center_size(Pos2::new(draw_x, cy), Vec2::new(hit_size, hit_size));
        let dresp = ui.interact(
            dr,
            ui.id().with(("prop_kf_diamond", track_idx, prop_name, kf_ms)),
            Sense::click_and_drag(),
        );
        if dresp.hovered() || is_ms || is_drag || is_act {
            painter.circle_filled(
                Pos2::new(draw_x, cy),
                ds * 1.8,
                Color32::from_rgba_unmultiplied(kc.r(), kc.g(), kc.b(), 45),
            );
        }
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(draw_x, cy - ds),
                Pos2::new(draw_x + ds, cy),
                Pos2::new(draw_x, cy + ds),
                Pos2::new(draw_x - ds, cy),
            ],
            if dresp.hovered() || is_drag {
                kc
            } else {
                kc.linear_multiply(0.85)
            },
            Stroke::new(0.5, Color32::from_rgba_unmultiplied(255, 255, 255, 70)),
        ));

        if !is_drag {
            text_tooltip(
                ui,
                dresp.id.with("tooltip"),
                &dresp,
                &format!("{} @ {:.2}s", prop_name, kf_s),
            );
        }

        // Click to select this exact property keyframe.
        if dresp.clicked() {
            let id = KeyframeId {
                scene: scene_for_selection.clone(),
                actor: actor_label.to_string(),
                property: prop_name.to_string(),
                time_ms: *kf_ms,
            };
            if shift_held {
                if let Some(p) = multi_selected.iter().position(|s| *s == id) {
                    multi_selected.remove(p);
                } else {
                    multi_selected.push(id);
                }
            } else {
                *multi_selected = vec![id];
            }
        }

        // Support dragging for per-property diamonds
        if dresp.drag_started() {
            let id = KeyframeId {
                scene: scene_for_selection.clone(),
                actor: actor_label.to_string(),
                property: prop_name.to_string(),
                time_ms: *kf_ms,
            };
            let grabbed_selected = multi_selected.contains(&id);
            if !shift_held && !grabbed_selected {
                *multi_selected = vec![id.clone()];
            }
            let ids = if grabbed_selected {
                multi_selected.clone()
            } else {
                vec![id]
            };
            *new_kf_drag = Some(KfDrag {
                scene: scene_for_selection.clone(),
                actor: actor_label.to_string(),
                property: prop_name.to_string(),
                anchor_keyframe_ms: *kf_ms,
                anchor_s: kf_s,
                target_s: kf_s,
                ids,
            });
        }
        if is_drag {
            if let Some(pos) = dresp.interact_pointer_pos() {
                let nt = coords.x_to_time(pos.x);
                let (snapped, _) = if shift_held {
                    (nt, None)
                } else {
                    snap_to_speech_or_fps(nt, snap_fps, speech_snap_targets)
                };
                if let Some(drag) = new_kf_drag.as_mut() {
                    drag.target_s = snapped;
                }
            }
        }
        if dresp.drag_stopped() && is_drag {
            emit_keyframe_moves(commands, new_kf_drag.as_ref());
            *new_kf_drag = None;
        }
    }
}

/// Track bar right-click context menu: bulk delete or clear selection.
pub(crate) fn handle_track_bar_context_menu(
    track_bar_resp: &egui::Response,
    actor_label: &str,
    active_scene: Option<&str>,
    multi_selected: &mut Vec<KeyframeId>,
    commands: &mut ActionQueue,
) {
    track_bar_resp.context_menu(|ui| {
        let track_selected: Vec<KeyframeId> = multi_selected
            .iter()
            .filter(|id| id.scene.as_deref() == active_scene && id.actor == actor_label)
            .cloned()
            .collect();
        if !track_selected.is_empty() {
            ui.strong(format!("{} selected keyframes", track_selected.len()));
            ui.separator();
            if ui
                .add(Button::danger("Delete selected").with_icon(egui_phosphor::regular::TRASH))
                .clicked()
            {
                commands.push_back(ShellAction::Command(Command::DeleteKeyframes(
                    track_selected.clone(),
                )));
                multi_selected
                    .retain(|id| id.scene.as_deref() != active_scene || id.actor != actor_label);
                ui.close();
            }
            ui.separator();
            if ui.button("Clear selection").clicked() {
                multi_selected
                    .retain(|id| id.scene.as_deref() != active_scene || id.actor != actor_label);
                ui.close();
            }
        } else {
            ui.label("No keyframes selected");
        }
    });
}
