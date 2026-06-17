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

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::app::commands::{ActionQueue, Command, PlaybackCommand, ShellAction};
use crate::app::components::button::{self, Button, toolbar_separator};
use crate::app::components::layout;
use crate::app::design_tokens::semantic::accent::{PRIMARY as semantic_accent_primary, CYAN as semantic_accent_cyan, selection as semantic_accent_selection};
use crate::app::design_tokens::semantic::border::DEFAULT as semantic_border_default;
use crate::app::design_tokens::semantic::category::ACTION as semantic_category_action;
use crate::app::design_tokens::semantic::status::{SUCCESS as semantic_status_success, WARNING as semantic_status_warning, ERROR as semantic_status_error};
use crate::app::design_tokens::semantic::surface::{BASE as semantic_surface_base, SURFACE as semantic_surface_surface, WIDGET as semantic_surface_widget};
use crate::app::design_tokens::semantic::text::{PRIMARY as semantic_text_primary, SECONDARY as semantic_text_secondary, MUTED as semantic_text_muted, faint as semantic_text_faint, dim as semantic_text_dim};
use crate::app::design_tokens::semantic::timeline::{KF_FLASH as semantic_timeline_kf_flash, loop_region as semantic_timeline_loop_region, track_block_1 as semantic_timeline_track_block_1, track_block_2 as semantic_timeline_track_block_2, track_block_3 as semantic_timeline_track_block_3, track_block_4 as semantic_timeline_track_block_4, track_block_5 as semantic_timeline_track_block_5, row_alt as semantic_timeline_row_alt};
use crate::app::design_tokens::spatial::{SPACE_2 as spatial_space_s, STROKE_WIDTH, RADIUS_S};
use crate::app::design_tokens::spatial::timeline::{LABEL_COL_WIDTH, TRACK_ROW_HEIGHT, RULER_HEIGHT, RANGE_HEIGHT, KF_HALF as KF_DIAMOND_HALF, PLAYBACK_STRIP_HEIGHT};
use crate::app::design_tokens::typography::{TextRole};
use crate::app::PreviewPaneState;
use animatix::composition::Composition;
use animatix::timeline::Timeline;
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

/// Property groups for per-property lanes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropertyGroup {
    Transform,
    Style,
    #[allow(dead_code)] // Reserved for future filter property lanes
    Filter,
    Shape,
    Text,
    #[allow(dead_code)] // Reserved for future layout property lanes
    Layout,
}

const PROPERTY_GROUPS: &[(PropertyGroup, &str, &[&str])] = &[
    (PropertyGroup::Transform, "Transform", &["position", "motion_offset", "rotation", "scale", "size", "layout_size", "placement_mode", "position_binding"]),
    (PropertyGroup::Style, "Style", &["color", "opacity", "stroke_width", "stroke_color", "stroke_progress", "fill_opacity", "line_cap", "line_join", "morph_options", "filter_blur", "filter_brightness", "filter_contrast", "filter_saturate", "filter_hue_rotate", "filter_sepia"]),
    (PropertyGroup::Shape, "Shape", &["shape_type", "line_from", "line_to", "head_size", "arc_angles", "points", "commands", "vector_paths"]),
    (PropertyGroup::Text, "Text", &["text_content", "font_family", "font_size", "text_paths"]),
];

#[allow(dead_code)] // Reserved for future property lane group headers
fn property_group_name(prop: &str) -> Option<&'static str> {
    for (_, group_name, props) in PROPERTY_GROUPS {
        if props.contains(&prop) {
            return Some(group_name);
        }
    }
    None
}

fn property_group_for_prop(prop: &str) -> Option<PropertyGroup> {
    for (group, _, props) in PROPERTY_GROUPS {
        if props.contains(&prop) {
            return Some(*group);
        }
    }
    None
}

fn property_group_color(group: PropertyGroup) -> Color32 {
    match group {
        PropertyGroup::Transform => semantic_accent_primary,
        PropertyGroup::Style => semantic_status_success,
        PropertyGroup::Filter => semantic_category_action,
        PropertyGroup::Shape => semantic_status_warning,
        PropertyGroup::Text => semantic_accent_cyan,
        PropertyGroup::Layout => semantic_accent_primary,
    }
}

pub(crate) struct TimelineContext<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a Composition>,
    #[allow(dead_code)] // Kept for future composition workrange display
    pub active_scene: Option<&'a str>,
    pub commands: &'a mut ActionQueue,
    pub collapsed_actors: &'a mut HashSet<String>,
    pub expanded_properties: &'a mut HashSet<String>,
    pub selected_actors: &'a mut HashSet<String>,
    /// Cached actor labels (recomputed in behavior.rs when stale).
    #[allow(dead_code)] // Reserved for future search/filter in timeline
    pub actor_labels: &'a [String],
    /// Cached per-actor keyframe property lists.
    #[allow(dead_code)] // Reserved for future per-actor keyframe count display
    pub actor_keyframes: &'a [(String, Vec<(u64, &'static str)>)],
    /// Cached per-scene keyframe time positions (for density strip rendering).
    pub scene_keyframe_times: &'a HashMap<String, Vec<f64>>,
    pub snap_fps: f32,
}

/// Render the entire timeline panel.
pub(crate) fn timeline_panel_ui(ctx: &mut TimelineContext<'_>, ui: &mut egui::Ui) {
    render_timeline_content(ctx, ui);
}


/// Width of the track label column on the left.
// (Imported via spatial::timeline at the top of the file)

fn action_category_color(cat: animatix::timeline::ActionCategory) -> Color32 {
    use animatix::timeline::ActionCategory;
    match cat {
        ActionCategory::Entrance => semantic_status_success,
        ActionCategory::Motion => semantic_accent_primary,
        ActionCategory::Exit => semantic_status_error,
        ActionCategory::Effect => semantic_status_warning,
        ActionCategory::Reorder => semantic_category_action,
        ActionCategory::Reveal => semantic_accent_cyan,
    }
}

/// Build a flat list of (actor_label, depth) in tree order (depth-first).
fn build_actor_tree(timeline: &Timeline, collapsed: &HashSet<String>) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    for root in timeline.root_actor_labels() {
        add_actor_and_children(timeline, root, 0, collapsed, &mut result);
    }
    result
}

fn add_actor_and_children(
    timeline: &Timeline,
    label: &str,
    depth: usize,
    collapsed: &HashSet<String>,
    result: &mut Vec<(String, usize)>,
) {
    result.push((label.to_string(), depth));
    if !collapsed.contains(label) {
        if let Some(track) = timeline.get_track(label) {
            for child in &track.children {
                add_actor_and_children(timeline, child, depth + 1, collapsed, result);
            }
        }
    }
}

/// Collect keyframe times for all properties of an actor.
fn collect_actor_keyframes(track: &animatix::timeline::AnimationTrack) -> Vec<(u64, &'static str)> {
    let mut result = Vec::new();
    use animatix::timeline::PropertyTrack;
    fn push<T>(result: &mut Vec<(u64, &'static str)>, opt: &Option<PropertyTrack<T>>, name: &'static str) {
        if let Some(pt) = opt {
            result.extend(pt.keyframes.keys().copied().map(|ms| (ms, name)));
        }
    }
    macro_rules! push_all {
        ($($field:ident => $name:literal),* $(,)?) => { $(
            push(&mut result, &track.$field, $name);
        )* };
    }
    push_all! {
        position => "position",
        motion_offset => "motion_offset",
        rotation => "rotation",
        scale => "scale",
        size => "size",
        color => "color",
        opacity => "opacity",
        stroke_width => "stroke_width",
        stroke_color => "stroke_color",
        stroke_progress => "stroke_progress",
        fill_opacity => "fill_opacity",
        text_content => "text_content",
        font_family => "font_family",
        font_size => "font_size",
        shape_type => "shape_type",
        line_from => "line_from",
        line_to => "line_to",
        arc_angles => "arc_angles",
        points => "points",
        commands => "commands",
        layout_size => "layout_size",
        vector_paths => "vector_paths",
        filter_blur => "filter_blur",
        filter_brightness => "filter_brightness",
        filter_contrast => "filter_contrast",
        filter_saturate => "filter_saturate",
        filter_hue_rotate => "filter_hue_rotate",
        filter_sepia => "filter_sepia",
        head_size => "head_size",
        line_cap => "line_cap",
        line_join => "line_join",
    }
    result.sort_by_key(|(ms, _)| *ms);
    result.dedup_by(|a, b| a.0 == b.0);
    result
}

/// Collect per-property keyframe times. Returns (property_name, [keyframe_times_ms]).
fn collect_per_property_keyframes(track: &animatix::timeline::AnimationTrack) -> Vec<(&'static str, Vec<u64>)> {
    let mut result = Vec::new();
    use animatix::timeline::PropertyTrack;
    fn push<T>(result: &mut Vec<(&'static str, Vec<u64>)>, opt: &Option<PropertyTrack<T>>, name: &'static str) {
        if let Some(pt) = opt {
            if !pt.keyframes.is_empty() {
                result.push((name, pt.keyframes.keys().copied().collect()));
            }
        }
    }
    macro_rules! push_all {
        ($($field:ident => $name:literal),* $(,)?) => { $(
            push(&mut result, &track.$field, $name);
        )* };
    }
    push_all! {
        position => "position",
        motion_offset => "motion_offset",
        rotation => "rotation",
        scale => "scale",
        size => "size",
        color => "color",
        opacity => "opacity",
        stroke_width => "stroke_width",
        stroke_color => "stroke_color",
        stroke_progress => "stroke_progress",
        fill_opacity => "fill_opacity",
        text_content => "text_content",
        font_family => "font_family",
        font_size => "font_size",
        shape_type => "shape_type",
        line_from => "line_from",
        line_to => "line_to",
        arc_angles => "arc_angles",
        points => "points",
        commands => "commands",
        layout_size => "layout_size",
        vector_paths => "vector_paths",
        filter_blur => "filter_blur",
        filter_brightness => "filter_brightness",
        filter_contrast => "filter_contrast",
        filter_saturate => "filter_saturate",
        filter_hue_rotate => "filter_hue_rotate",
        filter_sepia => "filter_sepia",
        head_size => "head_size",
        line_cap => "line_cap",
        line_join => "line_join",
    }
    result
}

/// Handle click-and-drag scrubbing on a timeline bar.
fn bar_interaction(
    ui: &egui::Ui,
    bar_rect: Rect,
    id_salt: impl std::hash::Hash,
    cmds: &mut ActionQueue,
    x_to_time: impl Fn(f32) -> f64,
) {
    let bar_id = ui.id().with(id_salt);
    let response = ui.interact(bar_rect, bar_id, Sense::click_and_drag());
    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let new_time = x_to_time(pos.x);
            cmds.push_back(PlaybackCommand::ScrubTo(new_time).into());
        }
    }
}

/// Render the playback transport strip: play/pause/stop buttons, speed
/// dropdown, loop/ping-pong toggle, zoom controls, and timecode display.
fn render_transport_strip(
    ui: &mut egui::Ui,
    scroll_rect: egui::Rect,
    strip_top: f32,
    strip_bot: f32,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
) {
    let strip_rect = Rect::from_min_size(
        Pos2::new(scroll_rect.left(), strip_top),
        Vec2::new(scroll_rect.width(), PLAYBACK_STRIP_HEIGHT),
    );

    ui.scope_builder(egui::UiBuilder::new().max_rect(strip_rect), |ui| {
        // Background fill
        ui.painter().rect_filled(
            Rect::from_min_max(
                Pos2::new(scroll_rect.left(), strip_top),
                Pos2::new(scroll_rect.right(), strip_bot),
            ),
            0.0,
            semantic_surface_base,
        );

        ui.horizontal(|ui| {
            ui.add_space(spatial_space_s);

            // Go to start
            if ui.add(Button::ghost("").with_icon(egui_phosphor::regular::SKIP_BACK).with_tooltip("Go to start")).clicked() {
                commands.push_back(PlaybackCommand::ScrubTo(0.0).into());
            }

            // Previous keyframe
            if ui.add(Button::ghost("").with_icon(egui_phosphor::regular::CARET_LEFT).with_tooltip("Previous keyframe")).clicked() {
                commands.push_back(PlaybackCommand::PrevKeyframe.into());
            }

            // Play / Pause
            if ui.add(Button::icon(button::play_pause_icon(preview.playback.is_playing)).with_tooltip("Play/Pause (Space)")).clicked() {
                commands.push_back(PlaybackCommand::TogglePlayback.into());
            }

            // Next keyframe
            if ui.add(Button::ghost("").with_icon(egui_phosphor::regular::CARET_RIGHT).with_tooltip("Next keyframe")).clicked() {
                commands.push_back(PlaybackCommand::NextKeyframe.into());
            }

            // Frame-step back
            if ui.add(Button::ghost("").with_icon("⏪").with_tooltip("Step back one frame")).clicked() {
                commands.push_back(PlaybackCommand::FrameStepBackward.into());
            }

            // Frame-step forward
            if ui.add(Button::ghost("").with_icon("⏩").with_tooltip("Step forward one frame")).clicked() {
                commands.push_back(PlaybackCommand::FrameStepForward.into());
            }

            // Go to end
            if ui.add(Button::ghost("").with_icon(egui_phosphor::regular::SKIP_FORWARD).with_tooltip("Go to end")).clicked() {
                commands.push_back(PlaybackCommand::ScrubTo(preview.playback.duration_s).into());
            }

            toolbar_separator(ui);

            // Speed dropdown
            const SPEEDS: [(f32, &str); 4] = [(0.5, "\u{BD}\u{D7}"), (1.0, "1\u{D7}"), (2.0, "2\u{D7}"), (4.0, "4\u{D7}")];
            let si = SPEEDS.iter().position(|(v, _)| (*v - preview.playback.playback_speed).abs() < f32::EPSILON).unwrap_or(1);
            ui.menu_button(RichText::new(SPEEDS[si].1).monospace().size(TextRole::BodyS.size()).color(semantic_text_secondary), |ui| {
                for (speed, label) in &SPEEDS {
                    let is_active = (*speed - preview.playback.playback_speed).abs() < f32::EPSILON;
                    if ui.selectable_label(is_active, *label).clicked() {
                        preview.playback.playback_speed = *speed;
                        ui.close();
                    }
                }
            });

            // Loop toggle
            let loop_active = preview.playback.loop_start_s.is_some() && preview.playback.loop_end_s.is_some();
            if ui.add(Button::ghost("").with_icon(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE).with_tooltip("Toggle loop playback").active(loop_active)).clicked() {
                if loop_active {
                    preview.playback.loop_start_s = None;
                    preview.playback.loop_end_s = None;
                } else {
                    preview.playback.loop_start_s = Some(0.0);
                    preview.playback.loop_end_s = Some(preview.playback.duration_s);
                }
            }

            // Ping-pong toggle
            let ping_pong_active = preview.playback.ping_pong;
            if ui.add(Button::ghost("").with_icon(egui_phosphor::regular::ARROWS_CLOCKWISE).with_tooltip("Toggle ping-pong playback (bounce at boundaries)").active(ping_pong_active)).clicked() {
                preview.playback.ping_pong = !preview.playback.ping_pong;
                if !preview.playback.ping_pong {
                    preview.playback.ping_pong_direction = 1;
                }
            }

            toolbar_separator(ui);

            // Zoom controls
            let zoom_text = format!("{:.0}%", preview.timeline_zoom * 100.0);
            if ui.button(egui::RichText::new(zoom_text).monospace().size(TextRole::BodyS.size()).color(semantic_text_secondary))
                .on_hover_text("Reset zoom")
                .clicked()
            {
                preview.timeline_zoom = 1.0;
                preview.timeline_scroll_offset = 0.0;
            }
            if ui.add(Button::ghost("").with_icon(egui_phosphor::regular::MINUS).with_tooltip("Zoom out")).clicked() {
                let new_zoom = (preview.timeline_zoom * 0.8).max(0.25);
                if new_zoom <= 1.0 {
                    preview.timeline_scroll_offset = 0.0;
                }
                preview.timeline_zoom = new_zoom;
            }
            if ui.add(Button::ghost("").with_icon(egui_phosphor::regular::PLUS).with_tooltip("Zoom in")).clicked() {
                let new_zoom = (preview.timeline_zoom * 1.25).min(8.0);
                if new_zoom <= 1.0 {
                    preview.timeline_scroll_offset = 0.0;
                }
                preview.timeline_zoom = new_zoom;
            }

            // Time display (right-aligned) — timecode + fps
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let current_tc = preview.playback.timecode_string();
                let dur = preview.playback.duration_s.max(0.0);
                let dh = (dur / 3600.0).floor() as u32;
                let dm = ((dur % 3600.0) / 60.0).floor() as u32;
                let ds = (dur % 60.0).floor() as u32;
                let df = ((dur % 1.0) * preview.playback.fps as f64).floor() as u32;
                let duration_tc = format!("{:02}:{:02}:{:02}:{:02}", dh, dm, ds, df);
                let fps_val = preview.playback.fps;
                ui.add(egui::Label::new(
                    egui::RichText::new(format!("{} / {}  {:.0}fps", current_tc, duration_tc, fps_val))
                        .font(TextRole::Mono.font_id())
                        .color(semantic_text_primary),
                ).selectable(false));
            });
        });
    });

    // Bottom border
    let painter = ui.painter();
    painter.line_segment(
        [Pos2::new(scroll_rect.left(), strip_bot - 1.0), Pos2::new(scroll_rect.right(), strip_bot - 1.0)],
        Stroke::new(STROKE_WIDTH, semantic_border_default));
}

fn render_timeline_content(ctx: &mut TimelineContext<'_>, ui: &mut egui::Ui) {
    let TimelineContext {
        preview,
        timeline,
        composition,
        active_scene: _,
        commands,
        collapsed_actors,
        expanded_properties,
        selected_actors,
        actor_labels: _,
        actor_keyframes: _,
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
    preview.flashed_keyframe_times.retain(|(_, instant)| now.duration_since(*instant) < Duration::from_millis(300));
    let duration_s = preview.playback.duration_s.max(0.1);
    let snap_fps = ctx.snap_fps;
    let panel_id = ui.id().with("timeline_panel");

    // ── Keyframe drag state ──
    // (actor_label, property_name, keyframe_ms, target_time_s)
    let kf_drag_id = panel_id.with("kf_drag");
    let kf_drag_data_id = kf_drag_id.with("data");
    let kf_drag: Option<(String, &'static str, u64, f64)> = ui.data(|d| d.get_temp(kf_drag_data_id));
    let mut new_kf_drag: Option<(String, &'static str, u64, f64)> = kf_drag.clone();

    // ── Action block drag state ──
    // (track_idx, event_start_ms, edge: LeftOrRight, initial_pointer_x, original_start_s, original_duration_s)
    #[derive(Clone, Copy, PartialEq)]
    enum Edge { Left, Right }
    let action_drag_id = panel_id.with("action_drag");
    let action_drag_data_id = action_drag_id.with("data");
    let action_drag: Option<(usize, u64, Edge, f32, f64, f64)> = ui.data(|d| d.get_temp(action_drag_data_id));
    let mut new_action_drag: Option<(usize, u64, Edge, f32, f64, f64)> = action_drag;

    // ── Keyframe multi-select state ──
    let kf_multi_select_id = panel_id.with("kf_multi");
    let mut multi_selected: Vec<(String, u64)> = ui.data(|d| d.get_temp(kf_multi_select_id)).unwrap_or_default();
    let shift_held = ui.input(|i| i.modifiers.shift);

    // actor_labels and actor_keyframes are precomputed by behavior.rs but we
    // compute the actor tree directly from the timeline for correctness.

    // ── Helper: draw loop region (function, not closure, to avoid borrow conflicts) ──
    fn draw_loop_region(
        p: &egui::Painter,
        y_top: f32,
        y_bot: f32,
        preview: &PreviewPaneState,
        time_to_x: &dyn Fn(f64) -> f32,
    ) {
        if let (Some(ls), Some(le)) = (preview.playback.loop_start_s, preview.playback.loop_end_s) {
            if le > ls {
                let lx = time_to_x(ls);
                let rx = time_to_x(le);
                if (rx - lx).abs() > 2.0 {
                    p.rect_filled(
                        Rect::from_min_max(Pos2::new(lx, y_top), Pos2::new(rx, y_bot)),
                        0.0,
                        semantic_timeline_loop_region(),
                    );
                }
            }
        }
    }

    /// Render colored block segments for each scene in a composition track,
    /// with a keyframe density strip and duration label inside each block.
    fn render_scene_blocks(
        painter: &egui::Painter,
        composition: &Composition,
        track_rect: egui::Rect,
        time_to_x: &dyn Fn(f64) -> f32,
        label_color: Color32,
        duration_s: f64,
        scene_keyframe_times: &HashMap<String, Vec<f64>>,
    ) {
        let palette = [semantic_timeline_track_block_1(), semantic_timeline_track_block_2(), semantic_timeline_track_block_3(), semantic_timeline_track_block_4(), semantic_timeline_track_block_5()];
        for (idx, sn) in composition.declaration_order.iter().enumerate() {
            let Some(scene) = composition.scenes.get(sn) else { continue };
            let Some(start_s) = composition.scene_start_times.get(sn).copied() else { continue };
            let end_s = (start_s + scene.duration_s).min(duration_s);
            if end_s <= start_s { continue; }
            let sr = Rect::from_min_max(
                Pos2::new(time_to_x(start_s), track_rect.top()),
                Pos2::new(time_to_x(end_s), track_rect.bottom()),
            );
            painter.rect_filled(sr, 2.0, palette[idx % palette.len()]);

            // ── Keyframe density strip ──
            if let Some(times) = scene_keyframe_times.get(sn) {
                let strip_y = sr.bottom() - 6.0;
                let strip_h = 4.0;
                // Semi-transparent background for the density strip
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(sr.left(), strip_y),
                        Pos2::new(sr.right(), strip_y + strip_h),
                    ),
                    0.0,
                    Color32::BLACK.linear_multiply(0.3),
                );
                // Draw tiny vertical marks for each keyframe
                for &kf_time_s in times {
                    let kf_x = time_to_x(start_s + kf_time_s);
                    if kf_x >= sr.left() && kf_x <= sr.right() {
                        painter.line_segment(
                            [Pos2::new(kf_x, strip_y), Pos2::new(kf_x, strip_y + strip_h)],
                            Stroke::new(1.0, semantic_accent_primary),
                        );
                    }
                }
            }

            // ── Scene name label ──
            if sr.width() > 24.0 {
                painter.text(
                    Pos2::new(sr.center().x, sr.center().y - 2.0),
                    Align2::CENTER_CENTER,
                    sn.as_str(),
                    FontId::monospace(10.0), // 10px mono: no TextRole
                    label_color,
                );
            }

            // ── Duration label (bottom-left of block) ──
            if sr.width() > 40.0 {
                let dur = scene.duration_s;
                let dur_text = if dur < 1.0 {
                    format!("{}ms", (dur * 1000.0).round() as u64)
                } else {
                    format!("{:.1}s", dur)
                };
                painter.text(
                    Pos2::new(sr.left() + 4.0, sr.bottom() - 2.0),
                    Align2::LEFT_BOTTOM,
                    dur_text,
                    FontId::monospace(10.0), // 10px mono: no TextRole
                    label_color.linear_multiply(0.6),
                );
            }
        }
    }

    // ── Transport strip (outside ScrollArea, always visible) ──
    {
        let outer_rect = ui.available_rect_before_wrap();
        let strip_top = outer_rect.top();
        let strip_bot = strip_top + PLAYBACK_STRIP_HEIGHT;
        render_transport_strip(ui, outer_rect, strip_top, strip_bot, preview, commands);
    }
    ui.add_space(PLAYBACK_STRIP_HEIGHT);

    // ── All content lives inside the ScrollArea ──
    // Wheel navigation (zoom/pan) is handled inside the closure so we share
    // the inner coordinate system and can read smooth_scroll_delta before
    // ScrollArea's post-processing consumes it.
    egui::ScrollArea::vertical()
        .id_salt("timeline_scroll")
        .show(ui, |ui| {
            // Compute layout metrics from the *inner* Ui so they exactly match
            // the ScrollArea content rect (no mismatch with outer captures).
            let scroll_rect = ui.available_rect_before_wrap();
            let left_edge = scroll_rect.left();
            let available = scroll_rect.width();
            let label_col_w = LABEL_COL_WIDTH.min(available * 0.3);
            let bar_origin_x = left_edge + label_col_w;
            let bar_width = (available - label_col_w).max(80.0);

            let zoom = preview.timeline_zoom.max(0.1);
            let visible_s = duration_s / zoom as f64;
            // Allow panning up to the edge of the max context (the visible range
            // at minimum zoom 0.25), so content/ruler space beyond the timeline
            // duration is still reachable.
            let max_context_s = duration_s / 0.25;
            let max_scroll = (max_context_s - visible_s).max(0.0);
            let scroll_s = preview.timeline_scroll_offset.clamp(0.0, max_scroll);

            // ── Wheel navigation (inside the ScrollArea closure, before any
            //     painting, so ScrollArea's post-processing can consume whatever
            //     we leave behind). ──
            //
            // Key egui behaviors we must account for:
            //   • Ctrl/Cmd+wheel → egui converts to zoom_delta(),
            //     smooth_scroll_delta is ZERO.
            //   • Shift+wheel    → egui remaps x←x+y, y←0, so wheel.x alone
            //     captures both plain horizontal and Shift+vertical pan.
            //   • ScrollArea zeros consumed axes from smooth_scroll_delta
            //     after the closure returns — we zero whichever axes we
            //     handle so ScrollArea doesn't re-use them.
            //   • rect_contains_pointer replaces fragile Sense::hover() which
            //     can miss events when ScrollArea overlays the area.
            {
                if ui.rect_contains_pointer(ui.clip_rect()) {
                    let modifiers = ui.input(|i| i.modifiers);
                    let wheel = ui.input(|i| i.smooth_scroll_delta);
                    let zoom_factor = ui.input(|i| i.zoom_delta());
                    let ctrl_or_cmd = modifiers.ctrl || modifiers.command;

                    if ctrl_or_cmd && zoom_factor != 1.0 {
                        // ── Zoom (cursor-stable) using egui's zoom_factor ──
                        let old_zoom = preview.timeline_zoom;
                        let new_zoom = (old_zoom * zoom_factor).clamp(0.25, 8.0);
                        if (new_zoom - old_zoom).abs() > 0.001 {
                            if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                                let cursor_time = if cursor.x >= bar_origin_x && cursor.x <= bar_origin_x + bar_width {
                                    let frac = ((cursor.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                                    scroll_s + frac * visible_s
                                } else if cursor.x < bar_origin_x {
                                    scroll_s
                                } else {
                                    scroll_s + visible_s
                                };
                                let new_visible = duration_s / new_zoom as f64;
                                let new_max_scroll = (max_context_s - new_visible).max(0.0);
                                let frac = ((cursor.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                                preview.timeline_scroll_offset = (cursor_time - frac * new_visible)
                                    .clamp(0.0, new_max_scroll);
                            }
                            preview.timeline_zoom = new_zoom;
                        }
                        // Zero all axes — Ctrl+scroll should not scroll rows
                        ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
                    } else if !ctrl_or_cmd && wheel.x.abs() > 0.0 {
                        // ── Horizontal pan ──
                        // wheel.x captures both plain horizontal scroll AND
                        // Shift+scroll (egui remaps Shift to x).
                        let delta_s = wheel.x as f64 / bar_width as f64 * visible_s;
                        let new_scroll = (scroll_s + delta_s).clamp(0.0, max_scroll);
                        if (new_scroll - scroll_s).abs() > 0.001 {
                            preview.timeline_scroll_offset = new_scroll;
                        }
                        // Zero X axis so ScrollArea doesn't re-interpret it
                        ui.input_mut(|i| i.smooth_scroll_delta.x = 0.0);
                    }
                }
            }

            let time_to_x = |t: f64| -> f32 {
                let frac = ((t - scroll_s) / visible_s).clamp(0.0, 1.0) as f32;
                bar_origin_x + frac * bar_width
            };

            let x_to_time = |x: f32| -> f64 {
                let frac = ((x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                scroll_s + frac * visible_s
            };

            let mut content_y = scroll_rect.top();

            // ── Compute y positions for all sections ──
            let ruler_top = content_y;
            let ruler_bot = ruler_top + RULER_HEIGHT;
            content_y = ruler_bot;

            let scene_track_top = content_y;
            let scene_track_bot = scene_track_top + TRACK_ROW_HEIGHT;
            if composition.is_some() {
                content_y = scene_track_bot;
            }

            // Build actor tree from timeline (all actors, not just roots)
            let actor_tree: Vec<(String, usize)> = timeline.map(|tl| build_actor_tree(tl, collapsed_actors)).unwrap_or_default();
            // Count extra rows for expanded property lanes
            let mut extra_prop_lanes = 0usize;
            if let Some(tl) = timeline {
                for (actor_label, _) in &actor_tree {
                    if expanded_properties.contains(actor_label) {
                        if let Some(track) = tl.get_track(actor_label) {
                            extra_prop_lanes += collect_per_property_keyframes(track).len();
                        }
                    }
                }
            }
            let actor_track_count = actor_tree.len();
            let total_track_rows = actor_track_count + extra_prop_lanes;
            let actor_first_top = content_y;
            let actor_last_bot = actor_first_top + total_track_rows as f32 * TRACK_ROW_HEIGHT;
            content_y = actor_last_bot;

            let rs_top = content_y;
            let rs_bot = rs_top + RANGE_HEIGHT;
            content_y = rs_bot;

            let content_bottom = content_y;

            // ── Allocate total space (releases mutable borrow of ui) ──
            ui.allocate_space(Vec2::new(
                scroll_rect.width(),
                content_bottom - ui.cursor().min.y,
            ));

            // ── Get painter (immutable borrow) and draw everything ──
            let painter = ui.painter();

            // ── Ruler ──
            {
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(scroll_rect.left(), ruler_top), Pos2::new(scroll_rect.right(), ruler_bot)),
                    0.0, semantic_surface_surface);
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(scroll_rect.left(), ruler_top), Pos2::new(bar_origin_x, ruler_bot)),
                    0.0, semantic_surface_base);

                let tick_step = if visible_s <= 2.0 { 0.25 } else if visible_s <= 5.0 { 0.5 }
                    else if visible_s <= 15.0 { 1.0 } else if visible_s <= 45.0 { 5.0 } else { 10.0 };
                let mut t = (scroll_s / tick_step).floor() * tick_step;
                while t <= scroll_s + visible_s {
                    let x = time_to_x(t);
                    if x >= bar_origin_x && x <= bar_origin_x + bar_width {
                        painter.line_segment([Pos2::new(x, ruler_bot - 6.0), Pos2::new(x, ruler_bot)], Stroke::new(STROKE_WIDTH, semantic_border_default));
                        painter.text(Pos2::new(x, ruler_top + RULER_HEIGHT * 0.35), Align2::CENTER_CENTER,
                            if tick_step >= 1.0 { format!("{:.0}s", t) } else { format!("{:.1}s", t) },
                            FontId::monospace(10.0), // 10px mono: no TextRole
                            semantic_text_muted);
                    }
                    t += tick_step;
                }
            }

            // ── Ruler click-to-scrub ──
            let ruler_rect = Rect::from_min_max(Pos2::new(scroll_rect.left(), ruler_top), Pos2::new(scroll_rect.right(), ruler_bot));
            bar_interaction(ui, ruler_rect, "ruler", commands, x_to_time);

            // ── Playhead X position ──
            let playhead_x = time_to_x(preview.playback.current_time_s());

            // ── Scene track (composition only) ──
            if let Some(comp) = composition {
                let st_top = scene_track_top;
                let st_bot = scene_track_bot;

                let label_rect = Rect::from_min_max(Pos2::new(scroll_rect.left(), st_top), Pos2::new(bar_origin_x, st_bot));
                painter.rect_filled(label_rect, 0.0, semantic_surface_base);
                painter.text(Pos2::new(scroll_rect.left() + spatial_space_s, (st_top + st_bot) / 2.0), Align2::LEFT_CENTER,
                    format!("{} Scenes", egui_phosphor::regular::FILM_STRIP),
                    TextRole::BodyS.font_id(), semantic_text_muted);

                let bar_area = Rect::from_min_max(Pos2::new(bar_origin_x, st_top), Pos2::new(scroll_rect.right(), st_bot));

                // ── Scene block drag state ──
                let scene_drag_id = panel_id.with("scene_drag");
                let scene_drag_data_id = scene_drag_id.with("data");
                let scene_drag: Option<(String, f32)> = ui.data(|d| d.get_temp(scene_drag_data_id));
                let mut new_scene_drag: Option<(String, f32)> = scene_drag.clone();

                render_scene_blocks(painter, comp, bar_area, &time_to_x, semantic_text_dim(), duration_s, scene_keyframe_times);

                // Draw drag ghost if dragging
                if let Some((ref drag_name, drag_offset_x)) = scene_drag {
                    if let Some(scene) = comp.scenes.get(drag_name) {
                        if let Some(start_s) = comp.scene_start_times.get(drag_name).copied() {
                            let end_s = start_s + scene.duration_s;
                            let block_w = time_to_x(end_s) - time_to_x(start_s);
                            let ghost_rect = Rect::from_min_size(
                                Pos2::new(bar_area.left() + drag_offset_x, bar_area.top()),
                                Vec2::new(block_w, bar_area.height()),
                            );
                            painter.rect_filled(ghost_rect, 2.0, Color32::from_rgba_premultiplied(60, 130, 230, 80));
                            painter.rect_stroke(ghost_rect, 2.0, Stroke::new(1.5, semantic_accent_primary), egui::StrokeKind::Outside);
                            if ghost_rect.width() > 24.0 {
                                painter.text(
                                    ghost_rect.center(),
                                    Align2::CENTER_CENTER,
                                    drag_name.as_str(),
                                    FontId::monospace(10.0), // 10px mono: no TextRole
                                    semantic_accent_primary,
                                );
                            }
                        }
                    }
                }

                for (src_name, edge) in &comp.edges {
                    let Some(src_scene) = comp.scenes.get(src_name) else { continue };
                    let Some(src_start) = comp.scene_start_times.get(src_name).copied() else { continue };
                    let src_right = time_to_x(src_start + src_scene.duration_s);
                    let Some(tgt_start) = comp.scene_start_times.get(&edge.to_scene).copied() else { continue };
                    let tgt_left = time_to_x(tgt_start);
                    if tgt_left <= src_right { continue; }
                    let cy = bar_area.center().y;
                    // Draw edge arrow line
                    painter.line_segment([Pos2::new(src_right, cy), Pos2::new(tgt_left, cy)], Stroke::new(STROKE_WIDTH, semantic_text_muted));
                    painter.add(egui::Shape::convex_polygon(
                        vec![Pos2::new(tgt_left, cy), Pos2::new(tgt_left - 4.0, cy - 2.5), Pos2::new(tgt_left - 4.0, cy + 2.5)],
                        semantic_text_muted, Stroke::NONE));

                    // ── Transition badge at midpoint ──
                    let mid_x = (src_right + tgt_left) / 2.0;
                    let badge_r = 7.0;
                    let badge_rect = Rect::from_center_size(Pos2::new(mid_x, cy), Vec2::new(badge_r * 2.0, badge_r * 2.0));

                    // Determine transition icon
                    let icon = match edge.transition.id.as_str() {
                        "fade" => "F",
                        "wipe-left" | "wipe-right" | "wipe-up" | "wipe-down" => "W",
                        _ => "C",
                    };

                    // Badge background circle
                    painter.circle_filled(Pos2::new(mid_x, cy), badge_r, semantic_surface_surface);
                    painter.circle_stroke(Pos2::new(mid_x, cy), badge_r, Stroke::new(1.0, semantic_text_muted));

                    // Badge icon text
                    painter.text(
                        badge_rect.center(),
                        Align2::CENTER_CENTER,
                        icon,
                        FontId::monospace(10.0), // 10px mono: no TextRole
                        semantic_text_primary,
                    );

                    // Tooltip on hover
                    let badge_resp = ui.interact(
                        badge_rect,
                        ui.id().with(("transition_badge", src_name, &edge.to_scene)),
                        Sense::hover(),
                    );
                    let dur_s = edge.transition.duration_ms as f64 / 1000.0;
                    let dur_text = if dur_s < 1.0 {
                        format!("{}ms", edge.transition.duration_ms)
                    } else {
                        format!("{:.1}s", dur_s)
                    };
                    badge_resp.on_hover_text(format!(
                        "{:?} → {} [{}]",
                        edge.transition.id,
                        edge.to_scene,
                        dur_text,
                    ));
                }

                draw_loop_region(painter, bar_area.top(), bar_area.bottom(), preview, &time_to_x);

                // Scene track interaction (click to scrub, drag scene blocks to reorder)
                let scene_bar_resp = ui.interact(bar_area, ui.id().with("scene_track"), Sense::click_and_drag());
                if scene_bar_resp.drag_started() {
                    if let Some(pos) = scene_bar_resp.interact_pointer_pos() {
                        // Check if click is on a scene block
                        for scene_name in &comp.declaration_order {
                            if let Some(scene) = comp.scenes.get(scene_name) {
                                if let Some(start_s) = comp.scene_start_times.get(scene_name).copied() {
                                    let end_s = start_s + scene.duration_s;
                                    let block_x0 = time_to_x(start_s);
                                    let block_x1 = time_to_x(end_s);
                                    if pos.x >= block_x0 && pos.x <= block_x1 {
                                        new_scene_drag = Some((scene_name.clone(), pos.x - bar_area.left()));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                if scene_bar_resp.dragged() && scene_drag.is_some() {
                    if let Some(pos) = scene_bar_resp.interact_pointer_pos() {
                        if let Some((ref name, _)) = new_scene_drag {
                            let block_w = comp.scenes.get(name)
                                .and_then(|s| comp.scene_start_times.get(name).map(|st| time_to_x(st + s.duration_s) - time_to_x(*st)))
                                .unwrap_or(40.0);
                            new_scene_drag = Some((name.clone(), (pos.x - bar_area.left() - block_w / 2.0).max(0.0)));
                        }
                    }
                }
                if scene_bar_resp.drag_stopped() {
                    if let Some((ref drag_name, drop_offset_x)) = scene_drag {
                        // Compute drop position in scene order
                        // Use actual block width instead of hardcoded value
                        let block_w = comp.scenes.get(drag_name)
                            .and_then(|s| comp.scene_start_times.get(drag_name).map(|st| time_to_x(st + s.duration_s) - time_to_x(*st)))
                            .unwrap_or(80.0);
                        let drop_center_x = bar_area.left() + drop_offset_x + block_w / 2.0;
                        let drop_time = x_to_time(drop_center_x);

                        // Find which scene index the drop lands on
                        let mut new_order = comp.declaration_order.clone();
                        if let Some(drag_idx) = new_order.iter().position(|n| n == drag_name) {
                            let _ = new_order.remove(drag_idx);
                            // Find insertion index based on drop time
                            let mut insert_idx = new_order.len();
                            for (i, name) in new_order.iter().enumerate() {
                                if let Some(start_s) = comp.scene_start_times.get(name).copied() {
                                    let mid_s = start_s + comp.scenes.get(name).map(|s| s.duration_s / 2.0).unwrap_or(0.0);
                                    if drop_time < mid_s {
                                        insert_idx = i;
                                        break;
                                    }
                                }
                            }
                            new_order.insert(insert_idx, drag_name.clone());

                            if new_order != comp.declaration_order {
                                commands.push_back(ShellAction::Command(Command::ReorderScenes(new_order)));
                            }
                        }
                    }
                    new_scene_drag = None;
                }


                // Persist scene drag state
                ui.data_mut(|d| d.insert_temp(scene_drag_data_id, new_scene_drag));

                painter.line_segment([Pos2::new(playhead_x, bar_area.top() - 2.0), Pos2::new(playhead_x, bar_area.bottom() + 2.0)], Stroke::new(1.5, semantic_text_primary));
                painter.line_segment([Pos2::new(scroll_rect.left(), st_bot), Pos2::new(scroll_rect.right(), st_bot)], Stroke::new(STROKE_WIDTH, semantic_border_default));
            }

            // ── Actor tracks (tree structure, all actors) ──
            let mut current_y = actor_first_top;
            for (track_idx, (actor_label, depth)) in actor_tree.iter().enumerate() {
                let is_collapsed = collapsed_actors.contains(actor_label);
                let has_children = timeline.and_then(|tl| tl.get_track(actor_label)).is_some_and(|t| !t.children.is_empty());
                let is_selected = selected_actors.contains(actor_label);
                let at_top = current_y;
                let at_bot = at_top + TRACK_ROW_HEIGHT;
                let track_rect = Rect::from_min_max(Pos2::new(scroll_rect.left(), at_top), Pos2::new(scroll_rect.right(), at_bot));

                // Alternating row background
                if track_idx % 2 == 0 {
                    painter.rect_filled(track_rect, 0.0, semantic_timeline_row_alt());
                }

                // Selection highlight
                if is_selected {
                    painter.rect_filled(track_rect, 0.0, semantic_accent_selection());
                    let accent = Rect::from_min_size(track_rect.min, Vec2::new(2.0, track_rect.height()));
                    painter.rect_filled(accent, 0.0, semantic_accent_primary);
                }

                // Label column background
                painter.rect_filled(Rect::from_min_max(Pos2::new(scroll_rect.left(), at_top), Pos2::new(bar_origin_x, at_bot)), 0.0, semantic_surface_base);

                // Indent based on depth
                let indent = *depth as f32 * 14.0;

                // Chevron toggle button (only if actor has children)
                let chevron_x = scroll_rect.left() + spatial_space_s + indent;
                if has_children {
                    let chevron_icon = if is_collapsed {
                        egui_phosphor::regular::CARET_RIGHT
                    } else {
                        egui_phosphor::regular::CARET_DOWN
                    };
                    let chevron_rect = Rect::from_min_size(
                        Pos2::new(chevron_x, at_top + 2.0),
                        Vec2::new(14.0, TRACK_ROW_HEIGHT - 4.0),
                    );
                    let chevron_resp = ui.interact(
                        chevron_rect,
                        ui.id().with(("actor_chevron", actor_label)),
                        Sense::click(),
                    );
                    painter.text(
                        chevron_rect.center(),
                        Align2::CENTER_CENTER,
                        chevron_icon,
                        TextRole::BodyS.font_id(),
                        if chevron_resp.hovered() { semantic_text_primary } else { semantic_text_muted },
                    );
                    if chevron_resp.clicked() {
                        if is_collapsed {
                            collapsed_actors.remove(actor_label);
                        } else {
                            collapsed_actors.insert(actor_label.clone());
                        }
                    }
                }

                // Property expand toggle (LIST icon)
                let prop_expanded = expanded_properties.contains(actor_label);
                let prop_toggle_x = chevron_x + if has_children { 18.0 } else { 4.0 };
                let prop_toggle_rect = Rect::from_min_size(
                    Pos2::new(prop_toggle_x, at_top + 2.0),
                    Vec2::new(14.0, TRACK_ROW_HEIGHT - 4.0),
                );
                let prop_toggle_resp = ui.interact(
                    prop_toggle_rect,
                    ui.id().with(("prop_toggle", actor_label)),
                    Sense::click(),
                );
                painter.text(
                    prop_toggle_rect.center(),
                    Align2::CENTER_CENTER,
                    egui_phosphor::regular::LIST,
                    TextRole::Micro.font_id(),
                    if prop_expanded { semantic_accent_primary } else { semantic_text_muted },
                );
                if prop_toggle_resp.clicked() {
                    if prop_expanded {
                        expanded_properties.remove(actor_label);
                    } else {
                        expanded_properties.insert(actor_label.clone());
                    }
                }

                // Track label
                let label_x = prop_toggle_x + 16.0 + spatial_space_s;
                let label_text = if actor_label.chars().count() > 16 {
                    actor_label.chars().take(15).collect::<String>() + "…"
                } else {
                    actor_label.clone()
                };
                painter.text(
                    Pos2::new(label_x, track_rect.center().y),
                    Align2::LEFT_CENTER,
                    &label_text,
                    TextRole::BodyS.font_id(),
                    if is_selected { semantic_text_primary } else { semantic_text_secondary },
                );

                let bar_area = Rect::from_min_max(Pos2::new(bar_origin_x, at_top), Pos2::new(scroll_rect.right(), at_bot));

                // Click on label to select actor
                let label_area = Rect::from_min_max(
                    Pos2::new(scroll_rect.left(), at_top),
                    Pos2::new(bar_origin_x, at_bot),
                );
                let label_resp = ui.interact(label_area, ui.id().with(("actor_label", track_idx)), Sense::click());
                if label_resp.clicked() {
                    let modifiers = ui.ctx().input(|i| i.modifiers);
                    let multi = modifiers.shift || modifiers.ctrl || modifiers.command;
                    if multi {
                        if selected_actors.contains(actor_label) {
                            selected_actors.remove(actor_label);
                        } else {
                            selected_actors.insert(actor_label.clone());
                        }
                    } else {
                        selected_actors.clear();
                        selected_actors.insert(actor_label.clone());
                    }
                    // Only scroll if the clicked row is not fully visible
                    let viewport = ui.clip_rect();
                    let row_visible = at_top >= viewport.top() && at_bot <= viewport.bottom();
                    if !row_visible {
                        let track_rect = Rect::from_min_max(
                            Pos2::new(scroll_rect.left(), at_top),
                            Pos2::new(scroll_rect.right(), at_bot),
                        );
                        ui.scroll_to_rect(track_rect, Some(egui::Align::Center));
                    }
                }

                // Action blocks
                if let Some(tl) = timeline {
                    for event in &tl.action_events {
                        if !event.targets.contains(actor_label) { continue; }
                        let start_s = event.start_time_ms as f64 / 1000.0;
                        let end_s = (event.start_time_ms + event.duration_ms) as f64 / 1000.0;
                        let duration_s = event.duration_ms as f64 / 1000.0;
                        let left = time_to_x(start_s);
                        let right = time_to_x(end_s);
                        if right > bar_area.left() && left < bar_area.right() {
                            let br = Rect::from_min_max(
                                Pos2::new(left.max(bar_area.left()), bar_area.top() + 3.0),
                                Pos2::new(right.min(bar_area.right()), bar_area.bottom() - 3.0),
                            );
                            let color = action_category_color(event.category);

                            // Check if this block is being dragged
                            let is_action_drag = action_drag.as_ref().is_some_and(|(ti, ms, _, _, _, _)| *ti == track_idx && *ms == event.start_time_ms);

                            // Draw drag handles (left/right edges)
                            let handle_w = 8.0;
                            if br.width() > handle_w * 2.0 + 4.0 {
                                let left_handle = Rect::from_min_max(br.left_top(), Pos2::new(br.left() + handle_w, br.bottom()));
                                let right_handle = Rect::from_min_max(Pos2::new(br.right() - handle_w, br.top()), br.right_bottom());
                                let handle_color = if is_action_drag { semantic_accent_primary } else { color.linear_multiply(0.7) };
                                painter.rect_filled(left_handle, RADIUS_S, handle_color);
                                painter.rect_filled(right_handle, RADIUS_S, handle_color);
                            }

                            // Draw main block body
                            painter.rect_filled(br, RADIUS_S, color.linear_multiply(0.5));
                            painter.rect_stroke(br, RADIUS_S, Stroke::new(if is_action_drag { 2.0 } else { STROKE_WIDTH }, if is_action_drag { semantic_accent_primary } else { color }), egui::StrokeKind::Outside);
                            if br.width() > 40.0 {
                                painter.text(br.center(), Align2::CENTER_CENTER, &event.verb, FontId::monospace(10.0), // 10px mono: no TextRole
                                semantic_text_primary);
                            }

                            // Interaction: click_and_drag for resize
                            let action_resp = ui.interact(br, ui.id().with(("action_block", track_idx, event.start_time_ms)), Sense::click_and_drag());

                            // Drag start: detect which edge by proximity
                            if action_resp.drag_started() {
                                if let Some(pos) = action_resp.interact_pointer_pos() {
                                    // Choose closest edge: left or right
                                    let dist_left = (pos.x - br.left()).abs();
                                    let dist_right = (pos.x - br.right()).abs();
                                    let edge = if dist_left <= dist_right { Edge::Left } else { Edge::Right };
                                    new_action_drag = Some((track_idx, event.start_time_ms, edge, pos.x, start_s, duration_s));
                                }
                            }

                            // During drag: visual feedback
                            if is_action_drag {
                                if let Some((_, _, edge, init_x, orig_start, orig_dur)) = action_drag {
                                    if let Some(pos) = action_resp.interact_pointer_pos() {
                                        let dx = pos.x - init_x;
                                        let dt = (dx / (bar_width / visible_s as f32)) as f64;
                                        match edge {
                                            Edge::Right => {
                                                let new_end_x = time_to_x(orig_start + orig_dur + dt);
                                                painter.line_segment([Pos2::new(new_end_x, br.top()), Pos2::new(new_end_x, br.bottom())], Stroke::new(2.0, semantic_accent_primary));
                                                let new_dur = (orig_dur + dt).max(0.1);
                                                let dur_text = if new_dur < 1.0 { format!("{}ms", (new_dur * 1000.0).round()) } else { format!("{:.2}s", new_dur) };
                                                painter.text(Pos2::new(new_end_x, br.top() - 4.0), Align2::CENTER_BOTTOM, dur_text, FontId::monospace(10.0), // 10px mono: no TextRole
                                                semantic_accent_primary);
                                            }
                                            Edge::Left => {
                                                let new_start_x = time_to_x(orig_start + dt);
                                                painter.line_segment([Pos2::new(new_start_x, br.top()), Pos2::new(new_start_x, br.bottom())], Stroke::new(2.0, semantic_accent_primary));
                                            }
                                        }
                                    }
                                }
                            }

                            // Drag stop: emit ResizeAction command
                            if action_resp.drag_stopped() {
                                if let Some((_, _, edge, init_x, orig_start, orig_dur)) = new_action_drag {
                                    if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                                        let dx = pos.x - init_x;
                                        let dt = (dx / (bar_width / visible_s as f32)) as f64;
                                        let (new_start_s, new_duration_s) = match edge {
                                            Edge::Right => (orig_start, (orig_dur + dt).max(0.1)),
                                            Edge::Left => {
                                                let ns = (orig_start + dt).max(0.0);
                                                (ns, (orig_dur - dt).max(0.1))
                                            }
                                        };
                                        commands.push_back(ShellAction::Command(Command::ResizeAction {
                                            verb: event.verb.clone(),
                                            targets: event.targets.clone(),
                                            old_start_s: orig_start,
                                            new_start_s,
                                            new_duration_s,
                                        }));
                                    }
                                }
                                new_action_drag = None;
                            }

                            // Tooltip on hover (when not dragging)
                            if !is_action_drag {
                                action_resp.on_hover_text(format!(
                                    "{:?}: {}\n{:.2}s → {:.2}s\nDrag edges to resize\nTargets: {}",
                                    event.category,
                                    event.verb,
                                    start_s,
                                    end_s,
                                    event.targets.join(", ")
                                ));
                            }
                        }
                    }
                }

                // Keyframe diamonds (computed from timeline)
                if let Some(tl) = timeline {
                    if let Some(track) = tl.get_track(actor_label) {
                        let kf_props = collect_actor_keyframes(track);
                        for (kf_ms, prop) in kf_props {
                            let kf_s = kf_ms as f64 / 1000.0;
                            let kf_x = time_to_x(kf_s);
                            if kf_x < bar_area.left() || kf_x > bar_area.right() { continue; }
                            let is_act = (kf_s - preview.playback.current_time_s()).abs() < 0.01;
                            let is_ms = multi_selected.iter().any(|(l, t)| l == actor_label && *t == kf_ms);
                            let is_drag = kf_drag.as_ref().is_some_and(|(l, _, t, _)| l == actor_label && *t == kf_ms);
                            let is_flashed = preview.flashed_keyframe_times.iter().any(|(t, _)| (*t - kf_s).abs() < 0.001);
                            let ds = if is_flashed { KF_DIAMOND_HALF * 2.0 } else if is_drag { KF_DIAMOND_HALF * 1.5 } else { KF_DIAMOND_HALF };
                            let kc = if is_flashed { semantic_timeline_kf_flash } else if is_ms { semantic_accent_primary } else if is_act { semantic_text_primary } else { semantic_status_warning };
                            let cy = bar_area.center().y;
                            let hit_size = (ds * 2.5).max(8.0);
                            let dr = Rect::from_center_size(Pos2::new(kf_x, cy), Vec2::new(hit_size, hit_size));
                            let dresp = ui.interact(dr, ui.id().with(("kf_diamond", track_idx, kf_ms)), Sense::click_and_drag());
                            painter.add(egui::Shape::convex_polygon(
                                vec![Pos2::new(kf_x, cy - ds), Pos2::new(kf_x + ds, cy), Pos2::new(kf_x, cy + ds), Pos2::new(kf_x - ds, cy)],
                                if dresp.hovered() || is_drag { kc } else { kc.linear_multiply(0.7) }, Stroke::NONE));

                            let dresp = if dresp.hovered() && !is_drag { dresp.on_hover_text(format!("{prop} @ {:.2}s", kf_s)) } else { dresp };

                            dresp.context_menu(|ui| {
                                ui.set_min_width(140.0);
                                ui.strong(format!("{} @ {:.2}s", prop, kf_s));
                                ui.separator();
                                ui.menu_button("Easing", |ui| {
                                    for &(id_str, display_name) in animatix_syntax::easing::EASING_REGISTRY {
                                        if ui.selectable_label(false, display_name).clicked() {
                                            let variant = animatix_syntax::easing::parse_easing_name(id_str).unwrap_or(animatix_syntax::easing::Easing::Linear);
                                            commands.push_back(ShellAction::Command(Command::SetKeyframeEasing {
                                                actor: actor_label.clone(),
                                                property: prop.to_string(),
                                                time_s: kf_s,
                                                easing: variant,
                                            }));
                                            ui.close();
                                        }
                                    }
                                });
                                ui.separator();
                                if ui.button(format!("{} Delete keyframe", egui_phosphor::regular::TRASH)).clicked() {
                                    commands.push_back(ShellAction::Command(Command::DeleteKeyframe {
                                        actor: actor_label.clone(),
                                        property: prop.to_string(),
                                        time_s: kf_s,
                                    }));
                                    ui.close();
                                }
                            });

                            if dresp.clicked() {
                                if shift_held {
                                    if let Some(p) = multi_selected.iter().position(|(l, t)| l == actor_label && *t == kf_ms) { multi_selected.remove(p); }
                                    else { multi_selected.push((actor_label.clone(), kf_ms)); }
                                } else {
                                    multi_selected.clear();
                                    multi_selected.push((actor_label.clone(), kf_ms));
                                }
                            }
                            if dresp.drag_started() {
                                new_kf_drag = Some((actor_label.clone(), prop, kf_ms, kf_s));
                                if !shift_held && !multi_selected.iter().any(|(l, t)| l == actor_label && *t == kf_ms) {
                                    multi_selected.clear();
                                    multi_selected.push((actor_label.clone(), kf_ms));
                                }
                            }
                            if is_drag {
                                if let Some(pos) = dresp.interact_pointer_pos() {
                                    let nt = x_to_time(pos.x);
                                    // 60 fps snap by default; hold Shift for free drag
                                    let snapped = if shift_held {
                                        nt
                                    } else {
                                        (nt * ctx.snap_fps as f64).round() / ctx.snap_fps as f64
                                    };
                                    new_kf_drag = Some((actor_label.clone(), prop, kf_ms, snapped));
                                    let gx = time_to_x(snapped);
                                    painter.line_segment([Pos2::new(gx, bar_area.top()), Pos2::new(gx, bar_area.bottom())], Stroke::new(STROKE_WIDTH, semantic_status_warning.linear_multiply(0.5)));
                                    let g = painter.layout_no_wrap(format!("{:.2}s → {:.2}s", kf_s, snapped), FontId::monospace(10.0), // 10px mono: no TextRole
                                    semantic_text_primary);
                                    let tr = Rect::from_min_size(Pos2::new(gx - g.size().x / 2.0, bar_area.top() - 16.0), g.size() + Vec2::new(8.0, 4.0));
                                    painter.rect_filled(tr, RADIUS_S, semantic_surface_surface);
                                    painter.galley(tr.min + Vec2::new(4.0, 2.0), g, semantic_text_primary);
                                }
                            }
                            if dresp.drag_stopped() && is_drag {
                                if let Some((ref actor, prop_name, _, n)) = new_kf_drag {
                                    if (n - kf_s).abs() > 0.01 {
                                        commands.push_back(ShellAction::Command(Command::MoveKeyframe {
                                            actor: actor.clone(),
                                            property: prop_name.to_string(),
                                            old_time_s: kf_s,
                                            new_time_s: n,
                                        }));
                                    }
                                }
                                new_kf_drag = None;
                            }
                        }
                    }
                }

                draw_loop_region(painter, bar_area.top(), bar_area.bottom(), preview, &time_to_x);

                // Track bar right-click: bulk operations for selected keyframes
                let track_bar_resp = ui.interact(bar_area, ui.id().with(("track_bar", track_idx)), Sense::click());
                track_bar_resp.context_menu(|ui| {
                    let track_selected: Vec<(String, u64)> = multi_selected.iter().filter(|(l, _)| l == actor_label).cloned().collect();
                    if !track_selected.is_empty() {
                        ui.strong(format!("{} selected keyframes", track_selected.len()));
                        ui.separator();
                        if ui.button(format!("{} Delete selected", egui_phosphor::regular::TRASH)).clicked() {
                            for (actor, time_ms) in &track_selected {
                                if let Some(tl) = timeline {
                                    if let Some(track) = tl.get_track(actor) {
                                        // Use per-property collector to delete all matching keyframes across all properties
                                        for (prop_name, times) in collect_per_property_keyframes(track) {
                                            if times.contains(time_ms) {
                                                commands.push_back(ShellAction::Command(Command::DeleteKeyframe {
                                                    actor: actor.clone(),
                                                    property: prop_name.to_string(),
                                                    time_s: *time_ms as f64 / 1000.0,
                                                }));
                                            }
                                        }
                                    }
                                }
                            }
                            multi_selected.clear();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Clear selection").clicked() {
                            multi_selected.retain(|(l, _)| l != actor_label);
                            ui.close();
                        }
                    } else {
                        ui.label("No keyframes selected");
                    }
                });

                // Track bar interaction — scrubbing removed; only ruler moves the playhead

                // Per-track playhead
                painter.line_segment([Pos2::new(playhead_x, bar_area.top()), Pos2::new(playhead_x, bar_area.bottom())], Stroke::new(STROKE_WIDTH, semantic_text_faint()));

                // Track separator
                painter.line_segment([Pos2::new(scroll_rect.left(), at_bot), Pos2::new(scroll_rect.right(), at_bot)], Stroke::new(STROKE_WIDTH, semantic_border_default));

                // Advance y past the main track
                current_y = at_bot;

                // Per-property lanes (if expanded)
                if expanded_properties.contains(actor_label) {
                    if let Some(tl) = timeline {
                        if let Some(track) = tl.get_track(actor_label) {
                            let prop_kfs = collect_per_property_keyframes(track);
                            for (prop_name, kf_times) in &prop_kfs {
                                let prop_top = current_y;
                                let prop_bot = prop_top + TRACK_ROW_HEIGHT;
                                let prop_rect = Rect::from_min_max(Pos2::new(scroll_rect.left(), prop_top), Pos2::new(scroll_rect.right(), prop_bot));

                                // Alternating row (offset from main track)
                                if track_idx % 2 == 1 {
                                    painter.rect_filled(prop_rect, 0.0, semantic_timeline_row_alt());
                                }

                                // Label column background
                                painter.rect_filled(Rect::from_min_max(Pos2::new(scroll_rect.left(), prop_top), Pos2::new(bar_origin_x, prop_bot)), 0.0, semantic_surface_base);

                                // Property label (indented deeper than actor label)
                                let prop_indent = *depth as f32 * 14.0 + 18.0;
                                let group = property_group_for_prop(prop_name);
                                let group_col = group.map(property_group_color).unwrap_or(semantic_status_warning);

                                // Small colored dot indicator
                                let dot_x = scroll_rect.left() + spatial_space_s + prop_indent;
                                painter.circle_filled(Pos2::new(dot_x + 3.0, prop_rect.center().y), 3.0, group_col);

                                let prop_label_x = dot_x + 10.0;
                                painter.text(
                                    Pos2::new(prop_label_x, prop_rect.center().y),
                                    Align2::LEFT_CENTER,
                                    *prop_name,
                                    TextRole::Micro.font_id(),
                                    semantic_text_muted,
                                );

                                // Keyframe diamonds for this property
                                let prop_bar_area = Rect::from_min_max(Pos2::new(bar_origin_x, prop_top), Pos2::new(scroll_rect.right(), prop_bot));
                                for kf_ms in kf_times {
                                    let kf_s = *kf_ms as f64 / 1000.0;
                                    let kf_x = time_to_x(kf_s);
                                    if kf_x < prop_bar_area.left() || kf_x > prop_bar_area.right() { continue; }
                                    let is_act = (kf_s - preview.playback.current_time_s()).abs() < 0.01;
                                    let is_drag = kf_drag.as_ref().is_some_and(|(l, _, t, _)| l == actor_label && *t == *kf_ms);
                                    let is_flashed = preview.flashed_keyframe_times.iter().any(|(t, _)| (*t - kf_s).abs() < 0.001);
                                    let ds = if is_flashed { KF_DIAMOND_HALF * 2.0 } else if is_drag { KF_DIAMOND_HALF * 1.5 } else { KF_DIAMOND_HALF };
                                    let base_color = group.map(property_group_color).unwrap_or(semantic_status_warning);
                                    let kc = if is_flashed { semantic_timeline_kf_flash } else if is_act { semantic_text_primary } else { base_color };
                                    let cy = prop_bar_area.center().y;
                                    let hit_size = (ds * 2.5).max(8.0);
                                    let dr = Rect::from_center_size(Pos2::new(kf_x, cy), Vec2::new(hit_size, hit_size));
                                    let dresp = ui.interact(dr, ui.id().with(("prop_kf_diamond", track_idx, prop_name, kf_ms)), Sense::click_and_drag());
                                    painter.add(egui::Shape::convex_polygon(
                                        vec![Pos2::new(kf_x, cy - ds), Pos2::new(kf_x + ds, cy), Pos2::new(kf_x, cy + ds), Pos2::new(kf_x - ds, cy)],
                                        if dresp.hovered() || is_drag { kc } else { kc.linear_multiply(0.7) }, Stroke::NONE));

                                    if !is_drag {
                                        dresp.clone().on_hover_text(format!("{} @ {:.2}s", prop_name, kf_s));
                                    }

                                    // Support dragging for per-property diamonds
                                    if dresp.drag_started() {
                                        new_kf_drag = Some((actor_label.clone(), prop_name, *kf_ms, kf_s));
                                    }
                                    // Update dragged time during drag
                                    if is_drag {
                                        if let Some(pos) = dresp.interact_pointer_pos() {
                                            let nt = x_to_time(pos.x);
                                            let snapped = if shift_held {
                                                nt
                                            } else {
                                                (nt * snap_fps as f64).round() / snap_fps as f64
                                            };
                                            new_kf_drag = Some((actor_label.clone(), prop_name, *kf_ms, snapped));
                                        }
                                    }
                                    if dresp.drag_stopped() && is_drag {
                                        if let Some((ref actor, _, _, n)) = new_kf_drag {
                                            if (n - kf_s).abs() > 0.01 {
                                                commands.push_back(ShellAction::Command(Command::MoveKeyframe {
                                                    actor: actor.clone(),
                                                    property: prop_name.to_string(),
                                                    old_time_s: kf_s,
                                                    new_time_s: n,
                                                }));
                                            }
                                        }
                                        new_kf_drag = None;
                                    }

                                    // Drag ghost line (rendered by the main loop for all kf_drag)
                                }

                                draw_loop_region(painter, prop_bar_area.top(), prop_bar_area.bottom(), preview, &time_to_x);

                                // Per-property playhead tick
                                painter.line_segment([Pos2::new(playhead_x, prop_bar_area.top()), Pos2::new(playhead_x, prop_bar_area.bottom())], Stroke::new(STROKE_WIDTH, semantic_text_faint()));

                                // Property lane separator
                                painter.line_segment([Pos2::new(scroll_rect.left(), prop_bot), Pos2::new(scroll_rect.right(), prop_bot)], Stroke::new(STROKE_WIDTH, semantic_border_default));

                                current_y = prop_bot;
                            }
                        }
                    }
                }
            }

            // ── Range slider for work/export region ──
            {
                painter.rect_filled(Rect::from_min_max(Pos2::new(scroll_rect.left(), rs_top), Pos2::new(bar_origin_x, rs_bot)), 0.0, semantic_surface_base);
                painter.text(Pos2::new(scroll_rect.left() + spatial_space_s, (rs_top + rs_bot) / 2.0), Align2::LEFT_CENTER, "Region",
                    TextRole::Micro.font_id(), semantic_text_muted);

                let range_bar = Rect::from_min_max(Pos2::new(bar_origin_x, rs_top), Pos2::new(scroll_rect.right(), rs_bot));
                let loop_active = preview.playback.loop_start_s.is_some() && preview.playback.loop_end_s.is_some();

                painter.rect_filled(range_bar, RADIUS_S, semantic_surface_widget);

                if loop_active {
                    // Loop is active — show draggable range handles
                    let ws = preview.playback.loop_start_s.unwrap_or(0.0);
                    let we = preview.playback.loop_end_s.unwrap_or(duration_s);
                    let wx = time_to_x(ws);
                    let wy = time_to_x(we);

                    if (wy - wx).abs() > 2.0 {
                        painter.rect_filled(Rect::from_min_max(Pos2::new(wx, range_bar.top() + 2.0), Pos2::new(wy, range_bar.bottom() - 2.0)), RADIUS_S, semantic_accent_primary.linear_multiply(0.3));
                    }

                    let hs = Vec2::new(12.0, RANGE_HEIGHT);
                    let sh = Rect::from_center_size(Pos2::new(wx, range_bar.center().y), hs);
                    let sr = ui.interact(sh, ui.id().with("range_start_handle"), Sense::click_and_drag());
                    if sr.dragged() {
                        if let Some(pos) = sr.interact_pointer_pos() {
                            let end = preview.playback.loop_end_s.unwrap_or(duration_s);
                            preview.playback.loop_start_s = Some(x_to_time(pos.x).min(end - 0.05));
                        }
                    }
                    painter.rect_filled(sh, RADIUS_S, semantic_accent_primary);

                    let eh = Rect::from_center_size(Pos2::new(wy, range_bar.center().y), hs);
                    let er = ui.interact(eh, ui.id().with("range_end_handle"), Sense::click_and_drag());
                    if er.dragged() {
                        if let Some(pos) = er.interact_pointer_pos() {
                            let start = preview.playback.loop_start_s.unwrap_or(0.0);
                            preview.playback.loop_end_s = Some(x_to_time(pos.x).max(start + 0.05));
                        }
                    }
                    painter.rect_filled(eh, RADIUS_S, semantic_accent_primary);

                    // Reciprocal enforcement: ensure end > start + 0.05
                    if let (Some(ls), Some(le)) = (preview.playback.loop_start_s, preview.playback.loop_end_s) {
                        if le <= ls + 0.05 {
                            let mid = (ls + le) / 2.0;
                            preview.playback.loop_start_s = Some((mid - 0.025).max(0.0));
                            preview.playback.loop_end_s = Some((mid + 0.025).min(duration_s));
                        }
                    }
                } else {
                    // Loop is off — show full-duration static indicator
                    painter.rect_filled(range_bar.shrink2(Vec2::new(0.0, 2.0)), RADIUS_S, semantic_surface_widget);
                    let mid = range_bar.center();
                    painter.text(mid, Align2::CENTER_CENTER, "Enable loop to set region", FontId::monospace(10.0), // 10px mono: no TextRole
                            semantic_text_muted);
                }
            }

            // ── Global playhead ──
            if playhead_x >= bar_origin_x && playhead_x <= bar_origin_x + bar_width + 2.0 {
                painter.line_segment([Pos2::new(playhead_x, ruler_top), Pos2::new(playhead_x, content_bottom)], Stroke::new(1.5, semantic_status_warning));
            } else if playhead_x < bar_origin_x {
                // Off-screen to the left: draw left-pointing arrow at visible edge
                let tip_x = bar_origin_x + 6.0;
                let tip_y = ruler_top + 8.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![Pos2::new(tip_x, tip_y - 4.0), Pos2::new(tip_x - 6.0, tip_y), Pos2::new(tip_x, tip_y + 4.0)],
                    semantic_status_warning, Stroke::NONE,
                ));
            } else if playhead_x > bar_origin_x + bar_width {
                // Off-screen to the right: draw right-pointing arrow at visible edge
                let tip_x = bar_origin_x + bar_width - 6.0;
                let tip_y = ruler_top + 8.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![Pos2::new(tip_x, tip_y - 4.0), Pos2::new(tip_x + 6.0, tip_y), Pos2::new(tip_x, tip_y + 4.0)],
                    semantic_status_warning, Stroke::NONE,
                ));
            }

            // ── Save keyframe drag + multi-select state ──
            ui.data_mut(|d| {
                if let Some(drag) = new_kf_drag.clone() { d.insert_temp(kf_drag_data_id, drag); }
                else { d.remove::<(String, &'static str, u64, f64)>(kf_drag_data_id); }
                d.insert_temp(kf_multi_select_id, multi_selected.clone());
                if let Some(drag) = new_action_drag { d.insert_temp(action_drag_data_id, drag); }
                else { d.remove::<(usize, u64, Edge, f32, f64, f64)>(action_drag_data_id); }
            });

            // Draw clip rect border
            ui.painter().rect_stroke(scroll_rect, 0.0, Stroke::new(STROKE_WIDTH, semantic_border_default), egui::StrokeKind::Inside);
        });
}
