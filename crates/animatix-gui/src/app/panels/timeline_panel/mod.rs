//! Global Timeline Panel
//!
//! A vertical-scrolling timeline with a scene track (for compositions),
//! per-actor tracks, a ruler, draggable playhead, loop-region overlay,
//! and a work/export range slider.
//!
//! Design:
//! ┌────────────────────────────────────────────────────┐
//! │  Ruler: 0s  1s  2s  3s  4s  5s                    │
//! ├────────┬──────────────────────────────────────────-┤
//! │ Scenes │ [──Intro───][─Diagram──][───Outro──]      │
//! ├────────┼──────────────────────────────────────────-┤
//! │ rect1  │ ◆         ◆        ◆                      │
//! │ rect2  │    ◆           ◆                          │
//! │ circle │ ◆  ◆     ◆                                │
//! ├────────┼──────────────────────────────────────────-┤
//! │ Region │ [◀────────────▶]                          │
//! └────────┴──────────────────────────────────────────-┘

pub mod keyframes;
pub mod ruler;
pub mod track_lanes;
pub mod track_tree;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use animatix::composition::Composition;
use animatix::timeline::Timeline;
use egui::{Pos2, Rect, Sense, Stroke, Vec2};

use self::keyframes::{KeyframeRenderCtx, KfDrag};
use self::ruler::{
    bar_interaction, render_playhead, render_range_slider, render_ruler, render_timeline_zoom_bar,
};
use self::track_lanes::{
    ActionDragState, property_group_color, property_group_for_prop, render_action_blocks,
    render_empty_property_lane, render_scene_track, render_speech_track,
};
use self::track_tree::{
    ActorRowHeaderLayout, build_actor_tree, render_actor_row_header, render_property_row_header,
};
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command, ShellAction};
use crate::app::components::layout;
use crate::app::design_tokens::semantic::timeline;
use crate::app::design_tokens::spatial::STROKE_WIDTH;
use crate::app::document::timeline_diff::{KeyframeId, collect_property_lanes};

/// Coordinate mapping between time (seconds) and timeline horizontal pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TimelineCoords {
    pub bar_origin_x: f32,
    pub bar_width: f32,
    pub scroll_s: f64,
    pub visible_s: f64,
}

impl TimelineCoords {
    pub fn time_to_x(&self, t: f64) -> f32 {
        let frac = ((t - self.scroll_s) / self.visible_s).clamp(0.0, 1.0) as f32;
        self.bar_origin_x + frac * self.bar_width
    }

    pub fn x_to_time(&self, x: f32) -> f64 {
        let frac = ((x - self.bar_origin_x) / self.bar_width).clamp(0.0, 1.0) as f64;
        self.scroll_s + frac * self.visible_s
    }
}

/// Helper: draw loop region overlay across the given vertical bounds.
pub(crate) fn draw_loop_region(
    p: &egui::Painter,
    y_top: f32,
    y_bot: f32,
    preview: &PreviewPaneState,
    coords: &TimelineCoords,
) {
    if let (Some(ls), Some(le)) = (preview.playback.loop_start_s, preview.playback.loop_end_s) {
        if le > ls {
            let lx = coords.time_to_x(ls);
            let rx = coords.time_to_x(le);
            if (rx - lx).abs() > 2.0 {
                p.rect_filled(
                    Rect::from_min_max(Pos2::new(lx, y_top), Pos2::new(rx, y_bot)),
                    0.0,
                    timeline::loop_region(),
                );
            }
        }
    }
}

pub(crate) struct TimelineContext<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a Composition>,
    pub active_scene: Option<&'a str>,
    pub commands: &'a mut ActionQueue,
    pub collapsed_actors: &'a mut HashSet<String>,
    pub expanded_properties: &'a mut HashSet<String>,
    pub selected_actors: &'a mut HashSet<String>,
    /// Cached per-scene keyframe time positions (for density strip rendering).
    pub scene_keyframe_times: &'a HashMap<String, Vec<f64>>,
    pub snap_fps: f32,
    /// Set by the panel each frame; true when the panel or its children received pointer
    /// interaction.
    pub timeline_focused: &'a mut bool,
}

/// Render the entire timeline panel.
pub(crate) fn timeline_panel_ui(ctx: &mut TimelineContext<'_>, ui: &mut egui::Ui) {
    render_timeline_content(ctx, ui);
}

fn render_timeline_content(ctx: &mut TimelineContext<'_>, ui: &mut egui::Ui) {
    let theme = eparts::theme(ui);
    let TimelineContext {
        preview,
        timeline,
        composition,
        commands,
        collapsed_actors,
        expanded_properties,
        selected_actors,
        scene_keyframe_times,
        ..
    } = ctx;

    // Focus the panel on click, not on hover. Hover-based focus silently
    // repurposed Delete whenever the pointer crossed the timeline; clicking
    // the timeline focuses it and clicking anywhere else releases it.
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let timeline_outer_rect = ui.available_rect_before_wrap();
    if ui.input(|i| i.pointer.primary_pressed()) {
        *ctx.timeline_focused = ui.rect_contains_pointer(timeline_outer_rect);
    }
    // Visible cue for the focus state that scopes Delete to keyframes.
    if *ctx.timeline_focused {
        let layer =
            egui::LayerId::new(egui::Order::Foreground, ui.id().with("timeline_focus_ring"));
        ui.ctx().layer_painter(layer).rect_stroke(
            timeline_outer_rect.shrink(1.0),
            egui::CornerRadius::ZERO,
            Stroke::new(STROKE_WIDTH, eparts::theme(ui).palette.border.focus),
            egui::StrokeKind::Inside,
        );
    }

    // Empty state when no timeline is loaded
    if timeline.is_none() && composition.is_none() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No timeline loaded",
            "Open or create a scene to begin",
        );
        return;
    }

    // Prune expired keyframe flashes (300 ms lifetime)
    let now = std::time::Instant::now();
    preview
        .flashed_keyframe_times
        .retain(|(_, instant)| now.duration_since(*instant) < Duration::from_millis(300));
    let duration_s = preview.playback.duration_s.max(0.1);
    let snap_fps = ctx.snap_fps;
    let panel_id = ui.id().with("timeline_panel");

    // ── Keyframe drag state ──
    let kf_drag_id = panel_id.with("kf_drag");
    let kf_drag_data_id = kf_drag_id.with("data");
    let kf_drag: Option<KfDrag> = ui.data(|d| d.get_temp(kf_drag_data_id));
    let mut new_kf_drag: Option<KfDrag> = kf_drag.clone();

    // ── Action block drag state ──
    let action_drag_id = panel_id.with("action_drag");
    let action_drag_data_id = action_drag_id.with("data");
    let action_drag: Option<ActionDragState> = ui.data(|d| d.get_temp(action_drag_data_id));
    let mut new_action_drag: Option<ActionDragState> = action_drag;

    // ── Keyframe multi-select state ──
    let kf_multi_select_id = panel_id.with("kf_multi");
    let mut multi_selected: Vec<KeyframeId> =
        ui.data(|d| d.get_temp(kf_multi_select_id)).unwrap_or_default();
    let shift_held = ui.input(|i| i.modifiers.shift);
    let speech_snap_targets: Vec<(f64, String)> = timeline
        .map(|tl| {
            let mut targets = Vec::new();
            for (_, transcript) in tl.transcripts_iter() {
                for (marker, &time_s) in &transcript.markers {
                    targets.push((time_s, format!("@{}", marker)));
                }
                for seg in &transcript.segments {
                    for w in &seg.words {
                        targets.push((w.start_s, format!("\"{}\"", w.word)));
                        targets.push((w.end_s, format!("\"{}\" end", w.word)));
                    }
                }
            }
            targets
        })
        .unwrap_or_default();
    // Scene-qualified identity of the active document, used for selection
    // comparisons and the mirror into the shared store.
    let active_scene = ctx.active_scene;
    let scene_for_selection = active_scene.map(ToOwned::to_owned);

    // ── Timeline-local toolbar (zoom). Playback transport is global now. ──
    {
        let outer_rect = ui.available_rect_before_wrap();
        let strip_bot = outer_rect.top() + sp.timeline.playback_strip_height;
        let strip_rect = Rect::from_min_max(
            Pos2::new(outer_rect.left(), outer_rect.top()),
            Pos2::new(outer_rect.right(), strip_bot),
        );
        render_timeline_zoom_bar(ui, strip_rect, preview);
    }
    ui.add_space(sp.timeline.playback_strip_height);

    // ── All content lives inside the ScrollArea ──
    egui::ScrollArea::vertical().id_salt("timeline_scroll").show(ui, |ui| {
        let scroll_rect = ui.available_rect_before_wrap();
        let left_edge = scroll_rect.left();
        let available = scroll_rect.width();
        let label_col_w = sp.timeline.label_col_width.min(available * 0.3);
        let bar_origin_x = left_edge + label_col_w;
        let bar_width = (available - label_col_w).max(80.0);

        let zoom = preview.timeline_zoom.max(0.1);
        let visible_s = duration_s / zoom as f64;
        let max_context_s = duration_s / 0.25;
        let max_scroll = (max_context_s - visible_s).max(0.0);
        let scroll_s = preview.timeline_scroll_offset.clamp(0.0, max_scroll);

        // ── Wheel navigation (zoom / pan) ──
        if ui.rect_contains_pointer(ui.clip_rect()) {
            let modifiers = ui.input(|i| i.modifiers);
            let wheel = ui.input(|i| i.smooth_scroll_delta);
            let zoom_factor = ui.input(|i| i.zoom_delta());
            let ctrl_or_cmd = modifiers.ctrl || modifiers.command;

            if ctrl_or_cmd && zoom_factor != 1.0 {
                let old_zoom = preview.timeline_zoom;
                let new_zoom = (old_zoom * zoom_factor).clamp(0.25, 8.0);
                if (new_zoom - old_zoom).abs() > 0.001 {
                    if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                        let cursor_time =
                            if cursor.x >= bar_origin_x && cursor.x <= bar_origin_x + bar_width {
                                let frac =
                                    ((cursor.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                                scroll_s + frac * visible_s
                            } else if cursor.x < bar_origin_x {
                                scroll_s
                            } else {
                                scroll_s + visible_s
                            };
                        let new_visible = duration_s / new_zoom as f64;
                        let new_max_scroll = (max_context_s - new_visible).max(0.0);
                        let frac = ((cursor.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                        preview.timeline_scroll_offset =
                            (cursor_time - frac * new_visible).clamp(0.0, new_max_scroll);
                    }
                    preview.timeline_zoom = new_zoom;
                }
                ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            } else if !ctrl_or_cmd && wheel.x.abs() > 0.0 {
                let delta_s = wheel.x as f64 / bar_width as f64 * visible_s;
                let new_scroll = (scroll_s + delta_s).clamp(0.0, max_scroll);
                if (new_scroll - scroll_s).abs() > 0.001 {
                    preview.timeline_scroll_offset = new_scroll;
                }
                ui.input_mut(|i| i.smooth_scroll_delta.x = 0.0);
            }
        }

        let coords = TimelineCoords {
            bar_origin_x,
            bar_width,
            scroll_s,
            visible_s,
        };

        let mut content_y = scroll_rect.top();

        // ── Compute y positions for all sections ──
        let ruler_top = content_y;
        let ruler_bot = ruler_top + sp.timeline.ruler_height;
        content_y = ruler_bot;

        let scene_track_top = content_y;
        let scene_track_bot = scene_track_top + sp.timeline.track_row_height;
        if composition.is_some() {
            content_y = scene_track_bot;
        }

        let has_transcripts =
            timeline.map(|tl| tl.transcripts_iter().next().is_some()).unwrap_or(false);
        let speech_track_top = content_y;
        let speech_track_bot = speech_track_top + sp.timeline.track_row_height;
        if has_transcripts {
            content_y = speech_track_bot;
        }

        // Build actor tree from timeline (all actors, not just roots)
        let actor_tree: Vec<(String, usize)> =
            timeline.map(|tl| build_actor_tree(tl, collapsed_actors)).unwrap_or_default();
        let mut extra_prop_lanes = 0usize;
        if let Some(tl) = timeline {
            for (actor_label, _) in &actor_tree {
                if expanded_properties.contains(actor_label) {
                    if let Some(track) = tl.get_track(actor_label) {
                        extra_prop_lanes += collect_property_lanes(
                            track,
                            tl.world().geometry(track.id),
                            tl.world().shape(track.id),
                            tl.world().text(track.id),
                            None,
                        )
                        .len();
                    }
                }
            }
        }
        let actor_track_count = actor_tree.len();
        let total_track_rows = actor_track_count + extra_prop_lanes;
        let actor_first_top = content_y;
        let actor_last_bot =
            actor_first_top + total_track_rows as f32 * sp.timeline.track_row_height;
        content_y = actor_last_bot;

        let rs_top = content_y;
        let rs_bot = rs_top + sp.timeline.range_height;
        content_y = rs_bot;

        let content_bottom = content_y;

        // ── Allocate total space ──
        ui.allocate_space(Vec2::new(scroll_rect.width(), content_bottom - ui.cursor().min.y));

        // ── Ruler ──
        render_ruler(ui.painter(), scroll_rect, ruler_top, ruler_bot, &coords, theme, &sp);

        // ── Ruler click-to-scrub ──
        let ruler_rect = Rect::from_min_max(
            Pos2::new(scroll_rect.left(), ruler_top),
            Pos2::new(scroll_rect.right(), ruler_bot),
        );
        bar_interaction(ui, ruler_rect, "ruler", commands, |x| coords.x_to_time(x));

        // ── Playhead X position ──
        let playhead_x = coords.time_to_x(preview.playback.current_time_s());

        // ── Scene track (composition only) ──
        if let Some(comp) = composition {
            render_scene_track(
                ui,
                comp,
                scene_track_top,
                scene_track_bot,
                scroll_rect,
                &coords,
                duration_s,
                scene_keyframe_times,
                preview,
                commands,
                playhead_x,
                panel_id,
                theme,
                &sp,
            );
        }

        // ── Speech / audio transcript track ──
        if has_transcripts {
            render_speech_track(
                ui,
                *timeline,
                speech_track_top,
                speech_track_bot,
                scroll_rect,
                &coords,
                preview,
                commands,
                playhead_x,
                panel_id,
                theme,
                &sp,
            );
        }

        // ── Actor tracks (tree structure, all actors) ──
        let mut current_y = actor_first_top;
        for (track_idx, (actor_label, depth)) in actor_tree.iter().enumerate() {
            let is_collapsed = collapsed_actors.contains(actor_label);
            let has_children =
                timeline.is_some_and(|tl| tl.child_labels(actor_label).next().is_some());
            let is_selected = selected_actors.contains(actor_label);
            let prop_expanded = expanded_properties.contains(actor_label);
            let at_top = current_y;
            let at_bot = at_top + sp.timeline.track_row_height;

            let header_layout = ActorRowHeaderLayout {
                actor_label,
                depth: *depth,
                track_idx,
                at_top,
                at_bot,
                scroll_rect,
                bar_origin_x,
                is_selected,
                is_collapsed,
                has_children,
                prop_expanded,
            };
            render_actor_row_header(
                ui,
                &header_layout,
                *timeline,
                collapsed_actors,
                expanded_properties,
                selected_actors,
                commands,
                theme,
                &sp,
            );

            let bar_area = Rect::from_min_max(
                Pos2::new(bar_origin_x, at_top),
                Pos2::new(scroll_rect.right(), at_bot),
            );

            // Action blocks
            if let Some(tl) = timeline {
                render_action_blocks(
                    ui,
                    tl,
                    actor_label,
                    track_idx,
                    bar_area,
                    &coords,
                    action_drag.as_ref(),
                    &mut new_action_drag,
                    commands,
                    theme,
                );
            }

            // Keyframe diamonds (computed from timeline)
            if let Some(tl) = timeline {
                if let Some(track) = tl.get_track(actor_label) {
                    let mut kf_ctx = KeyframeRenderCtx {
                        coords: &coords,
                        preview,
                        active_scene,
                        scene_for_selection: scene_for_selection.clone(),
                        multi_selected: &mut multi_selected,
                        kf_drag: kf_drag.as_ref(),
                        new_kf_drag: &mut new_kf_drag,
                        commands,
                        shift_held,
                        snap_fps,
                        speech_snap_targets: &speech_snap_targets,
                        theme,
                    };
                    keyframes::render_actor_keyframes(
                        ui,
                        track,
                        tl.world().geometry(track.id),
                        tl.world().shape(track.id),
                        tl.world().text(track.id),
                        actor_label,
                        track_idx,
                        bar_area,
                        &mut kf_ctx,
                    );
                }
            }

            draw_loop_region(ui.painter(), bar_area.top(), bar_area.bottom(), preview, &coords);

            // Track bar right-click: bulk operations for selected keyframes
            let track_bar_resp =
                ui.interact(bar_area, ui.id().with(("track_bar", track_idx)), Sense::click());
            keyframes::handle_track_bar_context_menu(
                &track_bar_resp,
                actor_label,
                active_scene,
                &mut multi_selected,
                commands,
            );

            // Per-track playhead
            ui.painter().line_segment(
                [
                    Pos2::new(playhead_x, bar_area.top()),
                    Pos2::new(playhead_x, bar_area.bottom()),
                ],
                Stroke::new(STROKE_WIDTH, theme.palette.text.faint),
            );

            // Track separator
            ui.painter().line_segment(
                [
                    Pos2::new(scroll_rect.left(), at_bot),
                    Pos2::new(scroll_rect.right(), at_bot),
                ],
                Stroke::new(STROKE_WIDTH, theme.palette.border.default),
            );

            // Advance y past the main track
            current_y = at_bot;

            // Per-property lanes (if expanded)
            if expanded_properties.contains(actor_label) {
                if let Some(tl) = timeline {
                    if let Some(track) = tl.get_track(actor_label) {
                        let prop_lanes = collect_property_lanes(
                            track,
                            tl.world().geometry(track.id),
                            tl.world().shape(track.id),
                            tl.world().text(track.id),
                            None,
                        );
                        for (prop_name, kf_times) in &prop_lanes {
                            let prop_top = current_y;
                            let prop_bot = prop_top + sp.timeline.track_row_height;
                            let group = property_group_for_prop(prop_name);
                            let group_col = group
                                .map(|g| property_group_color(g, theme))
                                .unwrap_or(theme.palette.status.warning);

                            render_property_row_header(
                                ui.painter(),
                                prop_name,
                                *depth,
                                track_idx,
                                prop_top,
                                prop_bot,
                                scroll_rect,
                                bar_origin_x,
                                group_col,
                                theme,
                                &sp,
                            );

                            let prop_bar_area = Rect::from_min_max(
                                Pos2::new(bar_origin_x, prop_top),
                                Pos2::new(scroll_rect.right(), prop_bot),
                            );

                            if kf_times.is_empty() {
                                render_empty_property_lane(
                                    ui,
                                    tl,
                                    track,
                                    tl.world().effects(track.id),
                                    actor_label,
                                    prop_name,
                                    track_idx,
                                    prop_bar_area,
                                    &coords,
                                    group_col,
                                    preview,
                                    commands,
                                    theme,
                                );
                            } else {
                                let mut kf_ctx = KeyframeRenderCtx {
                                    coords: &coords,
                                    preview,
                                    active_scene,
                                    scene_for_selection: scene_for_selection.clone(),
                                    multi_selected: &mut multi_selected,
                                    kf_drag: kf_drag.as_ref(),
                                    new_kf_drag: &mut new_kf_drag,
                                    commands,
                                    shift_held,
                                    snap_fps,
                                    speech_snap_targets: &speech_snap_targets,
                                    theme,
                                };
                                keyframes::render_property_keyframes(
                                    ui,
                                    kf_times,
                                    actor_label,
                                    prop_name,
                                    track_idx,
                                    prop_bar_area,
                                    group,
                                    &mut kf_ctx,
                                );
                            }

                            draw_loop_region(
                                ui.painter(),
                                prop_bar_area.top(),
                                prop_bar_area.bottom(),
                                preview,
                                &coords,
                            );

                            // Per-property playhead tick
                            ui.painter().line_segment(
                                [
                                    Pos2::new(playhead_x, prop_bar_area.top()),
                                    Pos2::new(playhead_x, prop_bar_area.bottom()),
                                ],
                                Stroke::new(STROKE_WIDTH, theme.palette.text.faint),
                            );

                            // Property lane separator
                            ui.painter().line_segment(
                                [
                                    Pos2::new(scroll_rect.left(), prop_bot),
                                    Pos2::new(scroll_rect.right(), prop_bot),
                                ],
                                Stroke::new(STROKE_WIDTH, theme.palette.border.default),
                            );

                            current_y = prop_bot;
                        }
                    }
                }
            }
        }

        // ── Range slider for work/export region ──
        render_range_slider(
            ui,
            rs_top,
            rs_bot,
            scroll_rect,
            &coords,
            duration_s,
            preview,
            theme,
            &sp,
        );

        // ── Global playhead ──
        render_playhead(ui.painter(), playhead_x, ruler_top, content_bottom, &coords, theme);

        // ── Save keyframe drag + multi-select state ──
        let prev_multi_selected = multi_selected.clone();
        ui.data_mut(|d| {
            if let Some(drag) = new_kf_drag.clone() {
                d.insert_temp(kf_drag_data_id, drag);
            } else {
                d.remove::<KfDrag>(kf_drag_data_id);
            }
            d.insert_temp(kf_multi_select_id, multi_selected.clone());
            if let Some(drag) = new_action_drag {
                d.insert_temp(action_drag_data_id, drag);
            } else {
                d.remove::<ActionDragState>(action_drag_data_id);
            }
        });

        // ── Mirror keyframe selection into the shared store ──
        if multi_selected != prev_multi_selected {
            let active_timeline: Option<&Timeline> = timeline.as_deref();
            let active_composition: Option<&Composition> = composition.as_deref();
            let canonical: Vec<KeyframeId> = multi_selected
                .iter()
                .filter(|id| {
                    let scene_timeline = match &id.scene {
                        None => active_timeline,
                        Some(name) => active_composition
                            .and_then(|comp| comp.scenes.get(name))
                            .map(|scene| &scene.timeline),
                    };
                    scene_timeline.is_some_and(|tl| tl.get_track(&id.actor).is_some())
                })
                .cloned()
                .collect();
            commands.push_back(ShellAction::Command(Command::SetSelectedKeyframes(canonical)));
        }

        // Draw clip rect border
        ui.painter().rect_stroke(
            scroll_rect,
            0.0,
            Stroke::new(STROKE_WIDTH, theme.palette.border.default),
            egui::StrokeKind::Inside,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::ruler::snap_to_speech_or_fps;

    #[test]
    fn test_snap_to_speech_or_fps() {
        let targets = vec![
            (1.50, "@intro".to_string()),
            (2.25, "\"hello\"".to_string()),
        ];

        let (snapped, label) = snap_to_speech_or_fps(1.52, 60.0, &targets);
        assert!((snapped - 1.50).abs() < 1e-4);
        assert_eq!(label.as_deref(), Some("@intro"));

        let (snapped, label) = snap_to_speech_or_fps(2.22, 60.0, &targets);
        assert!((snapped - 2.25).abs() < 1e-4);
        assert_eq!(label.as_deref(), Some("\"hello\""));

        let (snapped, label) = snap_to_speech_or_fps(3.821, 60.0, &targets);
        let expected_fps_snap = (3.821 * 60.0_f64).round() / 60.0_f64;
        assert!((snapped - expected_fps_snap).abs() < 1e-4);
        assert_eq!(label, None);
    }
}
