//! Timeline content rendering: coordinate mapping, section layout, wheel
//! navigation, and the per-actor/per-property track rows.
//!
//! This is the implementation half of the panel; `mod.rs` keeps only the
//! module declarations and re-exports. Drawing order, hit regions, scroll and
//! zoom inputs are unchanged from the monolithic version.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use animatix::composition::Composition;
use animatix::timeline::{EntityId, Timeline};
use egui::{Pos2, Rect, Sense, Stroke, Vec2};

use super::keyframes::{self, KeyframeRenderCtx, KfDrag};
use super::ruler::{
    bar_interaction, render_playhead, render_range_slider, render_ruler, render_timeline_zoom_bar,
};
use super::track_lanes::{
    ActionDragState, property_group_color, property_group_for_prop, render_action_blocks,
    render_empty_property_lane, render_scene_track, render_speech_track,
};
use super::track_tree::{
    ActorRowHeaderLayout, build_actor_tree, render_actor_row_header, render_property_row_header,
};
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command, ShellAction};
use crate::app::components::layout;
use crate::app::design_tokens::semantic::timeline;
use crate::app::design_tokens::spatial::{STROKE_WIDTH, Spatial};
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
    if let (Some(ls), Some(le)) = (preview.playback.loop_start_s, preview.playback.loop_end_s)
        && le > ls
    {
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

/// Horizontal geometry shared by every lane in one frame.
struct TimelineViewport {
    scroll_rect: Rect,
    coords: TimelineCoords,
}

impl TimelineViewport {
    fn new(ui: &egui::Ui, preview: &PreviewPaneState, sp: &Spatial, duration_s: f64) -> Self {
        let scroll_rect = ui.available_rect_before_wrap();
        let available = scroll_rect.width();
        let label_col_w = sp.timeline.label_col_width.min(available * 0.3);
        let bar_origin_x = scroll_rect.left() + label_col_w;
        let bar_width = (available - label_col_w).max(80.0);

        let zoom = preview.timeline_zoom.max(0.1);
        let visible_s = duration_s / zoom as f64;
        let max_context_s = duration_s / 0.25;
        let max_scroll = (max_context_s - visible_s).max(0.0);
        let scroll_s = preview.timeline_scroll_offset.clamp(0.0, max_scroll);

        Self {
            scroll_rect,
            coords: TimelineCoords {
                bar_origin_x,
                bar_width,
                scroll_s,
                visible_s,
            },
        }
    }
}

/// Vertical placement of the ruler, optional tracks, actor rows, and range slider.
struct TimelineSections {
    ruler_top: f32,
    ruler_bot: f32,
    scene_track_top: f32,
    scene_track_bot: f32,
    speech_track_top: f32,
    speech_track_bot: f32,
    actor_first_top: f32,
    range_top: f32,
    range_bot: f32,
    content_bottom: f32,
}

fn layout_sections(
    content_top: f32,
    sp: &Spatial,
    has_composition: bool,
    has_transcripts: bool,
    total_track_rows: usize,
) -> TimelineSections {
    let ruler_top = content_top;
    let ruler_bot = ruler_top + sp.timeline.ruler_height;

    let scene_track_top = ruler_bot;
    let scene_track_bot = scene_track_top + sp.timeline.track_row_height;

    let speech_track_top = if has_composition {
        scene_track_bot
    } else {
        ruler_bot
    };
    let speech_track_bot = speech_track_top + sp.timeline.track_row_height;

    let actor_first_top = if has_transcripts {
        speech_track_bot
    } else {
        speech_track_top
    };
    let actor_last_bot = actor_first_top + total_track_rows as f32 * sp.timeline.track_row_height;

    let range_top = actor_last_bot;
    let range_bot = range_top + sp.timeline.range_height;

    TimelineSections {
        ruler_top,
        ruler_bot,
        scene_track_top,
        scene_track_bot,
        speech_track_top,
        speech_track_bot,
        actor_first_top,
        range_top,
        range_bot,
        content_bottom: range_bot,
    }
}

/// Keyframe/action drag plus the cross-row keyframe selection for one frame.
struct TimelineInteractionState {
    kf_drag: Option<KfDrag>,
    new_kf_drag: Option<KfDrag>,
    action_drag: Option<ActionDragState>,
    new_action_drag: Option<ActionDragState>,
    multi_selected: Vec<KeyframeId>,
}

impl TimelineInteractionState {
    fn load(ui: &egui::Ui, panel_id: egui::Id) -> Self {
        let kf_drag: Option<KfDrag> =
            ui.data(|d| d.get_temp(panel_id.with("kf_drag").with("data")));
        let action_drag: Option<ActionDragState> =
            ui.data(|d| d.get_temp(panel_id.with("action_drag").with("data")));
        let multi_selected: Vec<KeyframeId> =
            ui.data(|d| d.get_temp(panel_id.with("kf_multi"))).unwrap_or_default();

        Self {
            new_kf_drag: kf_drag.clone(),
            new_action_drag: action_drag,
            kf_drag,
            action_drag,
            multi_selected,
        }
    }

    fn save(&self, ui: &egui::Ui, panel_id: egui::Id) {
        let kf_drag_data_id = panel_id.with("kf_drag").with("data");
        let action_drag_data_id = panel_id.with("action_drag").with("data");
        let kf_multi_select_id = panel_id.with("kf_multi");

        ui.data_mut(|d| {
            if let Some(drag) = self.new_kf_drag.clone() {
                d.insert_temp(kf_drag_data_id, drag);
            } else {
                d.remove::<KfDrag>(kf_drag_data_id);
            }
            d.insert_temp(kf_multi_select_id, self.multi_selected.clone());
            if let Some(drag) = self.new_action_drag {
                d.insert_temp(action_drag_data_id, drag);
            } else {
                d.remove::<ActionDragState>(action_drag_data_id);
            }
        });
    }
}

/// Per-frame state shared by every actor row and property lane.
///
/// Grouping it once keeps the row helpers from taking a dozen independent
/// arguments while still borrowing each field separately.
struct TimelineRowCtx<'a> {
    timeline: Option<&'a Timeline>,
    coords: &'a TimelineCoords,
    scroll_rect: Rect,
    bar_origin_x: f32,
    playhead_x: f32,
    active_scene: Option<&'a str>,
    scene_for_selection: Option<String>,
    shift_held: bool,
    snap_fps: f32,
    speech_snap_targets: &'a [(f64, String)],
    theme: eparts::Theme,
    sp: &'a Spatial,
    preview: &'a PreviewPaneState,
    collapsed_actors: &'a mut HashSet<String>,
    expanded_properties: &'a mut HashSet<String>,
    selected_actors: &'a mut HashSet<String>,
    commands: &'a mut ActionQueue,
    multi_selected: &'a mut Vec<KeyframeId>,
    kf_drag: Option<&'a KfDrag>,
    new_kf_drag: &'a mut Option<KfDrag>,
    action_drag: Option<&'a ActionDragState>,
    new_action_drag: &'a mut Option<ActionDragState>,
}

impl TimelineRowCtx<'_> {
    fn keyframe_ctx(&mut self) -> KeyframeRenderCtx<'_> {
        KeyframeRenderCtx {
            coords: self.coords,
            preview: self.preview,
            active_scene: self.active_scene,
            scene_for_selection: self.scene_for_selection.clone(),
            multi_selected: &mut *self.multi_selected,
            kf_drag: self.kf_drag,
            new_kf_drag: &mut *self.new_kf_drag,
            commands: &mut *self.commands,
            shift_held: self.shift_held,
            snap_fps: self.snap_fps,
            speech_snap_targets: self.speech_snap_targets,
            theme: self.theme,
        }
    }

    /// Draw one actor's header, action blocks, keyframe row, and property
    /// lanes. Returns the y coordinate immediately below the last lane.
    fn render_actor_row(
        &mut self,
        ui: &mut egui::Ui,
        actor_label: &str,
        depth: usize,
        track_idx: usize,
        at_top: f32,
    ) -> f32 {
        let timeline = self.timeline;
        let at_bot = at_top + self.sp.timeline.track_row_height;
        let is_collapsed = self.collapsed_actors.contains(actor_label);
        let has_children = timeline.is_some_and(|tl| tl.child_labels(actor_label).next().is_some());
        let is_selected = self.selected_actors.contains(actor_label);
        let prop_expanded = self.expanded_properties.contains(actor_label);

        let header_layout = ActorRowHeaderLayout {
            actor_label,
            depth,
            track_idx,
            at_top,
            at_bot,
            scroll_rect: self.scroll_rect,
            bar_origin_x: self.bar_origin_x,
            is_selected,
            is_collapsed,
            has_children,
            prop_expanded,
        };
        render_actor_row_header(
            ui,
            &header_layout,
            timeline,
            &mut *self.collapsed_actors,
            &mut *self.expanded_properties,
            &mut *self.selected_actors,
            &mut *self.commands,
            self.theme,
            self.sp,
        );

        let bar_area = Rect::from_min_max(
            Pos2::new(self.bar_origin_x, at_top),
            Pos2::new(self.scroll_rect.right(), at_bot),
        );

        if let Some(tl) = timeline {
            render_action_blocks(
                ui,
                tl,
                actor_label,
                track_idx,
                bar_area,
                self.coords,
                self.action_drag,
                &mut *self.new_action_drag,
                &mut *self.commands,
                self.theme,
            );
        }

        if let Some(tl) = timeline
            && let Some(id) = tl.get_entity_id(actor_label)
        {
            let mut kf_ctx = self.keyframe_ctx();
            keyframes::render_actor_keyframes(
                ui,
                tl.world().geometry(id),
                tl.world().style(id),
                tl.world().shape_or_default(id),
                tl.world().text_or_default(id),
                actor_label,
                track_idx,
                bar_area,
                &mut kf_ctx,
            );
        }

        draw_loop_region(
            ui.painter(),
            bar_area.top(),
            bar_area.bottom(),
            self.preview,
            self.coords,
        );

        // Track bar right-click: bulk operations for selected keyframes
        let track_bar_resp =
            ui.interact(bar_area, ui.scope_id().with(("track_bar", track_idx)), Sense::click());
        keyframes::handle_track_bar_context_menu(
            &track_bar_resp,
            actor_label,
            self.active_scene,
            &mut *self.multi_selected,
            &mut *self.commands,
        );

        self.draw_track_guides(ui, bar_area, at_bot);
        let mut current_y = at_bot;

        if self.expanded_properties.contains(actor_label)
            && let Some(tl) = timeline
            && let Some(id) = tl.get_entity_id(actor_label)
        {
            current_y =
                self.render_property_lanes(ui, tl, id, actor_label, depth, track_idx, current_y);
        }

        current_y
    }

    /// Per-property lanes for one expanded actor. Returns the y below the last lane.
    fn render_property_lanes(
        &mut self,
        ui: &mut egui::Ui,
        timeline: &Timeline,
        id: EntityId,
        actor_label: &str,
        depth: usize,
        track_idx: usize,
        start_y: f32,
    ) -> f32 {
        let prop_lanes = collect_actor_lanes(timeline, id);
        let mut current_y = start_y;

        for (prop_name, kf_times) in &prop_lanes {
            let prop_top = current_y;
            let prop_bot = prop_top + self.sp.timeline.track_row_height;
            let group = property_group_for_prop(prop_name);
            let group_col = group
                .map(|g| property_group_color(g, self.theme))
                .unwrap_or(self.theme.palette.status.warning);

            render_property_row_header(
                ui.painter(),
                prop_name,
                depth,
                track_idx,
                prop_top,
                prop_bot,
                self.scroll_rect,
                self.bar_origin_x,
                group_col,
                self.theme,
                self.sp,
            );

            let prop_bar_area = Rect::from_min_max(
                Pos2::new(self.bar_origin_x, prop_top),
                Pos2::new(self.scroll_rect.right(), prop_bot),
            );

            if kf_times.is_empty() {
                render_empty_property_lane(
                    ui,
                    timeline,
                    id,
                    timeline.world().effects(id),
                    actor_label,
                    prop_name,
                    track_idx,
                    prop_bar_area,
                    self.coords,
                    group_col,
                    self.preview,
                    &mut *self.commands,
                    self.theme,
                );
            } else {
                let mut kf_ctx = self.keyframe_ctx();
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
                self.preview,
                self.coords,
            );
            self.draw_track_guides(ui, prop_bar_area, prop_bot);
            current_y = prop_bot;
        }

        current_y
    }

    /// Per-row playhead tick and the separator below the row.
    fn draw_track_guides(&self, ui: &egui::Ui, bar_area: Rect, separator_y: f32) {
        ui.painter().line_segment(
            [
                Pos2::new(self.playhead_x, bar_area.top()),
                Pos2::new(self.playhead_x, bar_area.bottom()),
            ],
            Stroke::new(STROKE_WIDTH, self.theme.palette.text.faint),
        );
        ui.painter().line_segment(
            [
                Pos2::new(self.scroll_rect.left(), separator_y),
                Pos2::new(self.scroll_rect.right(), separator_y),
            ],
            Stroke::new(STROKE_WIDTH, self.theme.palette.border.default),
        );
    }
}

pub(crate) fn render_timeline_content(ctx: &mut TimelineContext<'_>, ui: &mut egui::Ui) {
    let theme = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let timeline_outer_rect = ui.available_rect_before_wrap();
    update_timeline_focus(ui, ctx.timeline_focused, timeline_outer_rect, theme);

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
    let panel_id = ui.scope_id().with("timeline_panel");
    let active_scene = ctx.active_scene;
    let scene_for_selection = active_scene.map(ToOwned::to_owned);
    let shift_held = ui.input(|i| i.modifiers.shift);
    let speech_snap_targets = collect_speech_snap_targets(*timeline);
    let mut interaction = TimelineInteractionState::load(ui, panel_id);

    // ── Timeline-local toolbar (zoom). Playback transport is global now. ──
    render_zoom_strip(ui, preview, &sp);

    // ── All content lives inside the ScrollArea ──
    egui::ScrollArea::vertical().id_salt("timeline_scroll").show(ui, |ui| {
        let viewport = TimelineViewport::new(ui, preview, &sp, duration_s);
        handle_timeline_navigation(ui, preview, &viewport.coords, duration_s);

        let has_transcripts = timeline.is_some_and(|tl| tl.transcripts_iter().next().is_some());
        let (actor_tree, extra_prop_lanes) =
            actor_rows(*timeline, collapsed_actors, expanded_properties);
        let total_track_rows = actor_tree.len() + extra_prop_lanes;
        let sections = layout_sections(
            viewport.scroll_rect.top(),
            &sp,
            composition.is_some(),
            has_transcripts,
            total_track_rows,
        );

        ui.allocate_space(Vec2::new(
            viewport.scroll_rect.width(),
            sections.content_bottom - ui.cursor().min.y,
        ));

        // ── Ruler and click-to-scrub ──
        render_ruler_and_scrubber(ui, &viewport, &sections, &sp, theme, commands);
        let playhead_x = viewport.coords.time_to_x(preview.playback.current_time_s());

        // ── Scene track (composition only) ──
        if let Some(comp) = composition {
            render_scene_track(
                ui,
                comp,
                sections.scene_track_top,
                sections.scene_track_bot,
                viewport.scroll_rect,
                &viewport.coords,
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
                sections.speech_track_top,
                sections.speech_track_bot,
                viewport.scroll_rect,
                &viewport.coords,
                preview,
                commands,
                playhead_x,
                panel_id,
                theme,
                &sp,
            );
        }

        // ── Actor tracks (tree structure, all actors) ──
        {
            let mut rows = TimelineRowCtx {
                timeline: *timeline,
                coords: &viewport.coords,
                scroll_rect: viewport.scroll_rect,
                bar_origin_x: viewport.coords.bar_origin_x,
                playhead_x,
                active_scene,
                scene_for_selection: scene_for_selection.clone(),
                shift_held,
                snap_fps,
                speech_snap_targets: &speech_snap_targets,
                theme,
                sp: &sp,
                preview,
                collapsed_actors,
                expanded_properties,
                selected_actors,
                commands,
                multi_selected: &mut interaction.multi_selected,
                kf_drag: interaction.kf_drag.as_ref(),
                new_kf_drag: &mut interaction.new_kf_drag,
                action_drag: interaction.action_drag.as_ref(),
                new_action_drag: &mut interaction.new_action_drag,
            };
            let mut current_y = sections.actor_first_top;
            for (track_idx, (actor_label, depth)) in actor_tree.iter().enumerate() {
                current_y = rows.render_actor_row(ui, actor_label, *depth, track_idx, current_y);
            }
        }

        // ── Range slider for work/export region ──
        render_range_slider(
            ui,
            sections.range_top,
            sections.range_bot,
            viewport.scroll_rect,
            &viewport.coords,
            duration_s,
            preview,
            theme,
            &sp,
        );

        // ── Global playhead ──
        render_playhead(
            ui.painter(),
            playhead_x,
            sections.ruler_top,
            sections.content_bottom,
            &viewport.coords,
            theme,
        );

        // ── Save keyframe drag + multi-select state ──
        let prev_multi_selected = interaction.multi_selected.clone();
        interaction.save(ui, panel_id);

        // ── Mirror keyframe selection into the shared store ──
        if interaction.multi_selected != prev_multi_selected {
            mirror_keyframe_selection(
                &interaction.multi_selected,
                *timeline,
                *composition,
                commands,
            );
        }

        // Draw clip rect border
        ui.painter().rect_stroke(
            viewport.scroll_rect,
            0.0,
            Stroke::new(STROKE_WIDTH, theme.palette.border.default),
            egui::StrokeKind::Inside,
        );
    });
}

/// Click-to-focus and the focus ring that scopes Delete to keyframes.
fn update_timeline_focus(
    ui: &egui::Ui,
    focused: &mut bool,
    outer_rect: Rect,
    theme: eparts::Theme,
) {
    // Focus the panel on click, not on hover. Hover-based focus silently
    // repurposed Delete whenever the pointer crossed the timeline; clicking
    // the timeline focuses it and clicking anywhere else releases it.
    if ui.input(|i| i.pointer.primary_pressed()) {
        *focused = ui.rect_contains_pointer(outer_rect);
    }
    // Visible cue for the focus state that scopes Delete to keyframes.
    if *focused {
        let layer =
            egui::LayerId::new(egui::Order::Foreground, ui.scope_id().with("timeline_focus_ring"));
        ui.ctx().layer_painter(layer).rect_stroke(
            outer_rect.shrink(1.0),
            egui::CornerRadius::ZERO,
            Stroke::new(STROKE_WIDTH, theme.palette.border.focus),
            egui::StrokeKind::Inside,
        );
    }
}

/// Timeline-local zoom strip above the scroll area.
fn render_zoom_strip(ui: &mut egui::Ui, preview: &mut PreviewPaneState, sp: &Spatial) {
    let outer_rect = ui.available_rect_before_wrap();
    let strip_bot = outer_rect.top() + sp.timeline.playback_strip_height;
    let strip_rect = Rect::from_min_max(
        Pos2::new(outer_rect.left(), outer_rect.top()),
        Pos2::new(outer_rect.right(), strip_bot),
    );
    render_timeline_zoom_bar(ui, strip_rect, preview);
    ui.add_space(sp.timeline.playback_strip_height);
}

fn render_ruler_and_scrubber(
    ui: &mut egui::Ui,
    viewport: &TimelineViewport,
    sections: &TimelineSections,
    sp: &Spatial,
    theme: eparts::Theme,
    commands: &mut ActionQueue,
) {
    render_ruler(
        ui.painter(),
        viewport.scroll_rect,
        sections.ruler_top,
        sections.ruler_bot,
        &viewport.coords,
        theme,
        sp,
    );

    let ruler_rect = Rect::from_min_max(
        Pos2::new(viewport.scroll_rect.left(), sections.ruler_top),
        Pos2::new(viewport.scroll_rect.right(), sections.ruler_bot),
    );
    bar_interaction(ui, ruler_rect, "ruler", commands, |x| viewport.coords.x_to_time(x));
}

/// Wheel navigation: ctrl/cmd zooms around the cursor, plain horizontal wheel pans.
fn handle_timeline_navigation(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    coords: &TimelineCoords,
    duration_s: f64,
) {
    if !ui.rect_contains_pointer(ui.clip_rect()) {
        return;
    }

    let modifiers = ui.input(|i| i.modifiers);
    let wheel = ui.input(|i| i.smooth_scroll_delta);
    let zoom_factor = ui.input(|i| i.zoom_delta());
    let ctrl_or_cmd = modifiers.ctrl || modifiers.command;
    let max_context_s = duration_s / 0.25;
    let max_scroll = (max_context_s - coords.visible_s).max(0.0);

    if ctrl_or_cmd && zoom_factor != 1.0 {
        let old_zoom = preview.timeline_zoom;
        let new_zoom = (old_zoom * zoom_factor).clamp(0.25, 8.0);
        if (new_zoom - old_zoom).abs() > 0.001 {
            if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                let cursor_time = if cursor.x >= coords.bar_origin_x
                    && cursor.x <= coords.bar_origin_x + coords.bar_width
                {
                    let frac = ((cursor.x - coords.bar_origin_x) / coords.bar_width).clamp(0.0, 1.0)
                        as f64;
                    coords.scroll_s + frac * coords.visible_s
                } else if cursor.x < coords.bar_origin_x {
                    coords.scroll_s
                } else {
                    coords.scroll_s + coords.visible_s
                };
                let new_visible = duration_s / new_zoom as f64;
                let new_max_scroll = (max_context_s - new_visible).max(0.0);
                let frac =
                    ((cursor.x - coords.bar_origin_x) / coords.bar_width).clamp(0.0, 1.0) as f64;
                preview.timeline_scroll_offset =
                    (cursor_time - frac * new_visible).clamp(0.0, new_max_scroll);
            }
            preview.timeline_zoom = new_zoom;
        }
        ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    } else if !ctrl_or_cmd && wheel.x.abs() > 0.0 {
        let delta_s = wheel.x as f64 / coords.bar_width as f64 * coords.visible_s;
        let new_scroll = (coords.scroll_s + delta_s).clamp(0.0, max_scroll);
        if (new_scroll - coords.scroll_s).abs() > 0.001 {
            preview.timeline_scroll_offset = new_scroll;
        }
        ui.input_mut(|i| i.smooth_scroll_delta.x = 0.0);
    }
}

/// Snap targets for speech markers and word boundaries.
fn collect_speech_snap_targets(timeline: Option<&Timeline>) -> Vec<(f64, String)> {
    timeline
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
        .unwrap_or_default()
}

/// Actor tree plus the number of expanded property lanes beneath it.
fn actor_rows(
    timeline: Option<&Timeline>,
    collapsed_actors: &HashSet<String>,
    expanded_properties: &HashSet<String>,
) -> (Vec<(String, usize)>, usize) {
    let actor_tree: Vec<(String, usize)> =
        timeline.map(|tl| build_actor_tree(tl, collapsed_actors)).unwrap_or_default();

    let mut extra_prop_lanes = 0usize;
    if let Some(tl) = timeline {
        for (actor_label, _) in &actor_tree {
            if expanded_properties.contains(actor_label)
                && let Some(id) = tl.get_entity_id(actor_label)
            {
                extra_prop_lanes += collect_actor_lanes(tl, id).len();
            }
        }
    }

    (actor_tree, extra_prop_lanes)
}

fn collect_actor_lanes(timeline: &Timeline, id: EntityId) -> Vec<(String, Vec<u64>)> {
    collect_property_lanes(
        timeline.world(),
        id,
        timeline.world().geometry(id),
        timeline.world().style(id),
        timeline.world().shape_or_default(id),
        timeline.world().text_or_default(id),
        None,
    )
}

/// Push the canonical (scene-qualified) keyframe selection into the shell store.
fn mirror_keyframe_selection(
    multi_selected: &[KeyframeId],
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    commands: &mut ActionQueue,
) {
    let canonical: Vec<KeyframeId> = multi_selected
        .iter()
        .filter(|id| {
            let scene_timeline = match &id.scene {
                None => timeline,
                Some(name) => {
                    composition.and_then(|comp| comp.scenes.get(name)).map(|scene| &scene.timeline)
                },
            };
            scene_timeline.is_some_and(|tl| tl.has_actor(&id.actor))
        })
        .cloned()
        .collect();
    commands.push_back(ShellAction::Command(Command::SetSelectedKeyframes(canonical)));
}
