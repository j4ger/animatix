//! Timeline content rendering: coordinate mapping, section layout, wheel
//! navigation, and the per-actor/per-property track rows.
//!
//! This is the implementation half of the panel; `mod.rs` keeps only the
//! module declarations and re-exports. Drawing order, hit regions, scroll and
//! zoom inputs are unchanged from the monolithic version.

use std::collections::HashSet;

use animatix::composition::Composition;
use animatix::timeline::{EntityId, Timeline};
use egui::{Pos2, Rect, Stroke, Vec2};

use super::keyframes::{self, KeyframeRenderCtx};
use super::narration::{ReviewTrack, ScriptReview};
use super::ruler::{
    bar_interaction, render_playhead, render_range_slider, render_ruler, render_timeline_zoom_bar,
};
use super::track_lanes::{
    property_group_color, property_group_for_prop, render_action_blocks, render_speech_track,
    speech_row_count,
};
use super::track_tree::{
    ActorRowHeaderLayout, build_actor_tree, render_actor_row_header, render_property_row_header,
};
use super::{audio, markers, review};
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command};
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
        let frac = ((t - self.scroll_s) / self.visible_s) as f32;
        self.bar_origin_x + frac * self.bar_width
    }

    pub fn x_to_time(&self, x: f32) -> f64 {
        let frac = ((x - self.bar_origin_x) / self.bar_width).clamp(0.0, 1.0) as f64;
        self.scroll_s + frac * self.visible_s
    }
}

/// Draw loop region overlay across the given vertical bounds.
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
    pub statements: &'a [animatix_syntax::ast::Stmt],
    pub source_current: bool,
    pub diagnostics: &'a [animatix_syntax::diagnostics::Diagnostic],
    pub source_epoch: crate::app::document::version::SourceEpoch,
    pub source_text: &'a str,
    pub audio_segments: &'a [animatix::timeline::AudioSegment],
    pub audio_revision: u64,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a Composition>,
    pub active_scene: Option<&'a str>,
    pub commands: &'a mut ActionQueue,
    pub collapsed_actors: &'a mut HashSet<String>,
    pub expanded_properties: &'a mut HashSet<String>,
    pub selected_actors: &'a mut HashSet<String>,
    /// Scene-qualified keyframe selection for read-only track inspection.
    pub selected_keyframes: &'a mut Vec<KeyframeId>,
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
    marker_top: f32,
    audio_top: f32,
    audio_bot: f32,
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
    transcript_rows: usize,
    has_audio: bool,
    total_track_rows: usize,
) -> TimelineSections {
    let ruler_top = content_top;
    let ruler_bot = ruler_top + sp.timeline.ruler_height;

    let scene_track_top = ruler_bot;
    let scene_track_bot = scene_track_top + sp.timeline.track_row_height;

    let marker_top = if has_composition {
        scene_track_bot
    } else {
        ruler_bot
    };
    let audio_top = marker_top + 36.0;
    let audio_bot = audio_top + if has_audio { 56.0 } else { 0.0 };
    let speech_track_top = audio_bot;
    let speech_track_bot = speech_track_top + transcript_rows as f32 * sp.timeline.track_row_height;

    let actor_first_top = if transcript_rows > 0 {
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
        marker_top,
        audio_top,
        audio_bot,
        speech_track_top,
        speech_track_bot,
        actor_first_top,
        range_top,
        range_bot,
        content_bottom: range_bot,
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
    theme: eparts::Theme,
    sp: &'a Spatial,
    preview: &'a PreviewPaneState,
    collapsed_actors: &'a mut HashSet<String>,
    expanded_properties: &'a mut HashSet<String>,
    selected_actors: &'a mut HashSet<String>,
    selected_keyframes: &'a mut Vec<KeyframeId>,
}

impl TimelineRowCtx<'_> {
    fn keyframe_ctx(&mut self) -> KeyframeRenderCtx<'_> {
        KeyframeRenderCtx {
            coords: self.coords,
            preview: self.preview,
            active_scene: self.active_scene,
            scene_for_selection: self.scene_for_selection.clone(),
            selected_keyframes: &mut *self.selected_keyframes,
            shift_held: self.shift_held,
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
            &mut *self.collapsed_actors,
            &mut *self.expanded_properties,
            &mut *self.selected_actors,
            self.theme,
            self.sp,
        );

        let bar_area = Rect::from_min_max(
            Pos2::new(self.bar_origin_x, at_top),
            Pos2::new(self.scroll_rect.right(), at_bot),
        );

        if let Some(tl) = timeline {
            render_action_blocks(ui, tl, actor_label, track_idx, bar_area, self.coords, self.theme);
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

            if !kf_times.is_empty() {
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
    let TimelineContext {
        preview,
        timeline,
        composition,
        commands,
        collapsed_actors,
        expanded_properties,
        selected_actors,
        selected_keyframes,
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

    let duration_s = preview.playback.duration_s.max(0.1);
    let panel_id = ui.scope_id().with("timeline_panel");
    let active_scene = ctx.active_scene;
    let scene_for_selection = active_scene.map(ToOwned::to_owned);
    let shift_held = ui.input(|i| i.modifiers.shift);
    let mut source_markers =
        markers::review_markers(ctx.statements, *timeline, *composition, ctx.diagnostics);
    if ctx.source_current {
        super::marker_source::locate_cached(ui, ctx.source_text, &mut source_markers);
    }
    if !ctx.source_current {
        source_markers.iter_mut().for_each(|marker| marker.line = None);
    }
    let scene_offset = composition
        .and_then(|c| active_scene.and_then(|s| c.scene_start_times.get(s)))
        .copied()
        .unwrap_or(0.0);
    let (show_debug, script_track) = review_controls(
        ui,
        preview,
        commands,
        panel_id,
        &sp,
        ScriptReview {
            source: ctx.source_text,
            current: ctx.source_current,
            epoch: ctx.source_epoch,
            revision: ctx.audio_revision,
            scene: active_scene,
            timeline: *timeline,
            markers: &source_markers,
            offset: scene_offset,
        },
    );

    // ── All content lives inside the ScrollArea ──
    egui::ScrollArea::vertical().id_salt("timeline_scroll").show(ui, |ui| {
        let viewport = TimelineViewport::new(ui, preview, &sp, duration_s);
        handle_timeline_navigation(ui, preview, &viewport.coords, duration_s);

        let speech_detail = script_track.detail;
        let word_rows = match speech_detail {
            super::narration::SpeechDetail::Paragraphs => 0,
            _ => speech_row_count(*timeline, speech_detail),
        };
        let transcript_rows = word_rows + usize::from(script_track.has_script());
        let (actor_tree, extra_prop_lanes) =
            actor_rows(*timeline, collapsed_actors, expanded_properties);
        let total_track_rows = if show_debug {
            actor_tree.len() + extra_prop_lanes
        } else {
            0
        };
        let sections = layout_sections(
            viewport.scroll_rect.top(),
            &sp,
            composition.is_some(),
            transcript_rows,
            !ctx.audio_segments.is_empty(),
            total_track_rows,
        );

        ui.allocate_space(Vec2::new(
            viewport.scroll_rect.width(),
            sections.content_bottom - ui.cursor().min.y,
        ));

        // ── Ruler and click-to-scrub ──
        render_ruler_and_scrubber(ui, &viewport, &sections, &sp, theme, commands);
        let playhead_x = viewport.coords.time_to_x(preview.playback.current_time_s());

        if let Some(comp) = composition {
            review::scene_track(
                ui,
                comp,
                viewport.scroll_rect,
                sections.scene_track_top,
                sections.scene_track_bot,
                &viewport.coords,
                commands,
            );
        }
        let marker_rect = Rect::from_min_max(
            Pos2::new(viewport.scroll_rect.left(), sections.marker_top),
            Pos2::new(viewport.scroll_rect.right(), sections.marker_top + 36.0),
        );
        markers::render_markers(ui, marker_rect, &viewport.coords, &source_markers, commands);
        render_audio_lane(
            ui,
            &viewport,
            &sections,
            ctx.audio_segments,
            ctx.audio_revision,
            commands,
        );

        // ── Speech / audio transcript track ──
        let word_top = if script_track.has_script() {
            let bottom = sections.speech_track_top + sp.timeline.track_row_height;
            let rect = Rect::from_min_max(
                Pos2::new(viewport.scroll_rect.left(), sections.speech_track_top),
                Pos2::new(viewport.scroll_rect.right(), bottom),
            );
            script_track.draw(ui, rect, &viewport.coords, scene_offset, commands);
            bottom
        } else {
            sections.speech_track_top
        };
        if word_rows > 0 {
            render_speech_track(
                ui,
                *timeline,
                speech_detail,
                word_top,
                sections.speech_track_bot,
                viewport.scroll_rect,
                &viewport.coords,
                preview,
                commands,
                scene_offset,
                panel_id,
                theme,
                &sp,
            );
        }

        // Debug property tracks are read-only, selectable inspection rows.
        if show_debug {
            let mut rows = TimelineRowCtx {
                timeline: *timeline,
                coords: &viewport.coords,
                scroll_rect: viewport.scroll_rect,
                bar_origin_x: viewport.coords.bar_origin_x,
                playhead_x,
                active_scene,
                scene_for_selection: scene_for_selection.clone(),
                shift_held,
                theme,
                sp: &sp,
                preview,
                collapsed_actors,
                expanded_properties,
                selected_actors,
                selected_keyframes,
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

        render_timeline_overlays(ui, &viewport, &sections, playhead_x, theme);
    });
}

fn render_timeline_overlays(
    ui: &egui::Ui,
    viewport: &TimelineViewport,
    sections: &TimelineSections,
    playhead_x: f32,
    theme: eparts::Theme,
) {
    render_playhead(
        ui.painter(),
        playhead_x,
        sections.ruler_top,
        sections.content_bottom,
        &viewport.coords,
        theme,
    );

    // Continue the track-name gutter below the final track.
    if sections.content_bottom < viewport.scroll_rect.bottom() {
        let gutter = Rect::from_min_max(
            Pos2::new(viewport.scroll_rect.left(), sections.content_bottom),
            Pos2::new(viewport.coords.bar_origin_x, viewport.scroll_rect.bottom()),
        );
        ui.painter().rect_filled(gutter, 0.0, theme.palette.surface.base);
        let edge = gutter.right();
        ui.painter().line_segment(
            [
                Pos2::new(edge, gutter.top()),
                Pos2::new(edge, gutter.bottom()),
            ],
            Stroke::new(STROKE_WIDTH, theme.palette.border.default),
        );
    }
}

fn render_audio_lane(
    ui: &mut egui::Ui,
    viewport: &TimelineViewport,
    sections: &TimelineSections,
    segments: &[animatix::timeline::AudioSegment],
    revision: u64,
    commands: &mut ActionQueue,
) {
    if segments.is_empty() {
        return;
    }
    let rect = Rect::from_min_max(
        Pos2::new(viewport.scroll_rect.left(), sections.audio_top),
        Pos2::new(viewport.scroll_rect.right(), sections.audio_bot),
    );
    if let Some(time) = audio::render_audio_track(ui, rect, &viewport.coords, segments, revision) {
        commands.push_back(Command::ScrubTo(time).into());
    }
}

fn review_controls(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
    panel_id: egui::Id,
    sp: &Spatial,
    script: ScriptReview<'_>,
) -> (bool, ReviewTrack) {
    let theme = eparts::theme(ui);
    egui::Frame::new()
        .fill(theme.palette.surface.panel)
        .inner_margin(egui::Margin::symmetric(sp.base.space_4 as i8, sp.base.space_2 as i8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = sp.base.space_2;
                let show_debug = review::toolbar(ui, preview, script.markers, commands, panel_id);
                // Leave space on the right so a selected paragraph's text can
                // truncate without pushing the zoom buttons onto another row.
                let zoom_width = 105.0;
                let mut script_rect = ui.available_rect_before_wrap();
                if script_rect.width() >= 285.0 {
                    script_rect.max.x -= zoom_width;
                }
                let script_track = ui
                    .scope_builder(
                        egui::UiBuilder::new().id_salt("script_controls").max_rect(script_rect),
                        |ui| script.show(ui, commands),
                    )
                    .inner;
                if ui.available_width() >= zoom_width {
                    if !script.current && ui.available_width() > 310.0 {
                        ui.label("Source navigation paused")
                            .on_hover_text("Source links resume after a successful rebuild.");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        render_timeline_zoom_bar(ui, preview);
                    });
                } else {
                    ui.horizontal(|ui| render_timeline_zoom_bar(ui, preview));
                }
                (show_debug, script_track)
            })
            .inner
        })
        .inner
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
