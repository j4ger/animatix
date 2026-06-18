//! Shared utilities extracted from the legacy drag handler.
//! These are pure helper functions that can be reused by gesture handlers.

use std::collections::HashSet;

use egui::Pos2;

use crate::app::commands::{DocumentCommand, PropertyEdit, PropertyValue};
use crate::app::design_tokens::semantic::accent;
use crate::app::design_tokens::semantic::status;
use crate::app::preview::context::PreviewContext;
use crate::app::preview::{ActorProps, DragState};
use animatix::timeline::{PositionBinding, TrackAccessor};

// ─── Helper 1: Body hit testing ─────────────────────────────────────────────

/// Check whether a scene-space point falls within the rotated bounding box of an actor.
pub(crate) fn hit_test_actor_body(scene: kurbo::Point, props: Option<&ActorProps>) -> bool {
    props
        .map(|p| {
            let local_pt = [
                (scene.x - p.position[0] as f64) as f32,
                (scene.y - p.position[1] as f64) as f32,
            ];
            let cos = (-p.rotation).cos();
            let sin = (-p.rotation).sin();
            let lx = local_pt[0] * cos - local_pt[1] * sin;
            let ly = local_pt[0] * sin + local_pt[1] * cos;
            let hw = p.size[0] / 2.0;
            let hh = p.size[1] / 2.0;
            lx.abs() <= hw && ly.abs() <= hh
        })
        .unwrap_or(false)
}

// ─── Helper 2: Handle hit testing ───────────────────────────────────────────

/// Find the scale handle nearest to the mouse (within hit radius), using distance tie-breaking.
/// Returns `Some(index)` for handles 0‑7, or `None`.
pub(crate) fn find_nearest_handle(
    mouse: Pos2,
    handle_screen: &[Pos2; 8],
    hit_radius: f32,
) -> Option<usize> {
    (0..8)
        .filter(|&i| mouse.distance(handle_screen[i]) <= hit_radius)
        .min_by_key(|&i| {
            let d = mouse.distance(handle_screen[i]);
            (d * 1000.0) as i32
        })
}

/// Check whether a scene-space point lies within any hit region of the given actor.
pub(crate) fn is_over_actor_hit_region(
    scene: kurbo::Point,
    actor: &str,
    hit_regions: &[(String, kurbo::Rect)],
) -> bool {
    hit_regions
        .iter()
        .any(|(label, bounds)| label == actor && bounds.contains(scene))
}

// ─── Helper 3: Snap resolution ──────────────────────────────────────────────

/// Result of snap resolution, containing updated coordinates and snap metadata.
pub(crate) struct SnapResult {
    pub nx: f32,
    pub ny: f32,
    #[allow(dead_code)] // Reserved for snap visual feedback
    pub snapped_guide_h: bool,
    #[allow(dead_code)] // Reserved for snap visual feedback
    pub snapped_guide_v: bool,
    #[allow(dead_code)] // Reserved for snap visual feedback
    pub snapped_actor_h: bool,
    #[allow(dead_code)] // Reserved for snap visual feedback
    pub snapped_actor_v: bool,
    #[allow(dead_code)] // Reserved for snap visual feedback
    pub snapped_container: bool,
    #[allow(dead_code)] // Reserved for snap visual feedback
    pub snapped_keyframe: bool,
    #[allow(dead_code)] // Reserved for snap HUD display
    pub snap_hud_text: Option<String>,
}

/// Snap a candidate position (`nx`, `ny`) to guides, actor edges, container edges,
/// and keyframe positions.
///
/// Updates `ctx.preview.snap.snap_lines_*` and `ctx.preview.snap.snap_line_color` /
/// `snap_hud_label` as side effects.
pub(crate) fn resolve_snap(
    actor: &str,
    nx: f32,
    ny: f32,
    threshold: f32,
    time_ms: u64,
    ctx: &mut PreviewContext<'_>,
) -> SnapResult {
    let mut nx = nx;
    let mut ny = ny;
    let mut snapped_guide_h = false;
    let mut snapped_guide_v = false;
    let mut snapped_actor_h = false;
    let mut snapped_actor_v = false;
    let mut snapped_container = false;
    let mut snapped_keyframe = false;
    let mut snap_hud_text: Option<String> = None;

    for &guide_y in &ctx.preview.guides.horizontal_guides {
        if (ny - guide_y).abs() < threshold {
            ny = guide_y;
            snapped_guide_h = true;
            snap_hud_text = Some(format!("Guide y={}", guide_y as i32));
        }
    }
    for &guide_x in &ctx.preview.guides.vertical_guides {
        if (nx - guide_x).abs() < threshold {
            nx = guide_x;
            snapped_guide_v = true;
            snap_hud_text = Some(format!("Guide x={}", guide_x as i32));
        }
    }

    let dragged_props = ctx.get_actor_props(actor);
    let half_w = dragged_props.as_ref().map(|p| p.size[0] / 2.0).unwrap_or(0.0);
    let half_h = dragged_props.as_ref().map(|p| p.size[1] / 2.0).unwrap_or(0.0);
    let dragged_x_edges = [nx - half_w, nx, nx + half_w];
    let dragged_y_edges = [ny - half_h, ny, ny + half_h];
    let edge_labels = ["left", "center", "right"];
    let edge_labels_y = ["top", "center", "bottom"];

    for (other_label, other_bounds) in ctx.hit_regions.iter() {
        if other_label == actor {
            continue;
        }
        let other_x_edges = [
            other_bounds.x0 as f32,
            (other_bounds.x0 + other_bounds.x1) as f32 / 2.0,
            other_bounds.x1 as f32,
        ];
        let other_y_edges = [
            other_bounds.y0 as f32,
            (other_bounds.y0 + other_bounds.y1) as f32 / 2.0,
            other_bounds.y1 as f32,
        ];

        for &de in dragged_x_edges.iter() {
            for (oi, &oe) in other_x_edges.iter().enumerate() {
                let candidate_nx = nx + (oe - de);
                if (candidate_nx - nx).abs() < threshold && (candidate_nx - nx).abs() > 0.001 {
                    nx = candidate_nx;
                    snapped_actor_v = true;
                    snap_hud_text = Some(format!("{} {}", other_label, edge_labels[oi]));
                }
            }
        }
        for &de in dragged_y_edges.iter() {
            for (oi, &oe) in other_y_edges.iter().enumerate() {
                let candidate_ny = ny + (oe - de);
                if (candidate_ny - ny).abs() < threshold && (candidate_ny - ny).abs() > 0.001 {
                    ny = candidate_ny;
                    snapped_actor_h = true;
                    snap_hud_text = Some(format!("{} {}", other_label, edge_labels_y[oi]));
                }
            }
        }
    }

    if let Some((container, _, _)) = ctx.find_layout_container(actor) {
        if let Some(container_props) = ctx.get_actor_props(&container) {
            if (nx - container_props.position[0]).abs() < threshold {
                nx = container_props.position[0];
                snapped_container = true;
                snap_hud_text = Some(format!("{} center X", container));
            }
            if (ny - container_props.position[1]).abs() < threshold {
                ny = container_props.position[1];
                snapped_container = true;
                snap_hud_text = Some(format!("{} center Y", container));
            }
            let c_hw = container_props.size[0] / 2.0;
            let c_left = container_props.position[0] - c_hw;
            let c_right = container_props.position[0] + c_hw;
            if (nx - c_left).abs() < threshold {
                nx = c_left;
                snapped_container = true;
                snap_hud_text = Some(format!("{} left", container));
            }
            if (nx - c_right).abs() < threshold {
                nx = c_right;
                snapped_container = true;
                snap_hud_text = Some(format!("{} right", container));
            }
            let c_hh = container_props.size[1] / 2.0;
            let c_top = container_props.position[1] - c_hh;
            let c_bottom = container_props.position[1] + c_hh;
            if (ny - c_top).abs() < threshold {
                ny = c_top;
                snapped_container = true;
                snap_hud_text = Some(format!("{} top", container));
            }
            if (ny - c_bottom).abs() < threshold {
                ny = c_bottom;
                snapped_container = true;
                snap_hud_text = Some(format!("{} bottom", container));
            }
        }
    }

    if let Some(track) = ctx.timeline.and_then(|t| t.get_track(actor)) {
        if let Some(ref pos_track) = track.position {
            let prev_kf_time = pos_track.keyframes().range(..time_ms).next_back().map(|(&t, _)| t);
            if let Some(kf_ms) = prev_kf_time {
                if let Some(kf_props) = ctx.get_actor_props_at_time(actor, kf_ms) {
                    if (nx - kf_props.position[0]).abs() < threshold {
                        nx = kf_props.position[0];
                        snapped_keyframe = true;
                        snap_hud_text =
                            Some(format!("prev keyframe ({:.2}s)", kf_ms as f64 / 1000.0));
                    }
                    if (ny - kf_props.position[1]).abs() < threshold {
                        ny = kf_props.position[1];
                        snapped_keyframe = true;
                        snap_hud_text =
                            Some(format!("prev keyframe ({:.2}s)", kf_ms as f64 / 1000.0));
                    }
                }
            }
        }
    }

    if snapped_guide_h || snapped_actor_h || snapped_container || snapped_keyframe {
        ctx.preview.snap.snap_lines_h.push(ny);
    }
    if snapped_guide_v || snapped_actor_v || snapped_container || snapped_keyframe {
        ctx.preview.snap.snap_lines_v.push(nx);
    }
    if snapped_guide_h
        || snapped_guide_v
        || snapped_actor_h
        || snapped_actor_v
        || snapped_container
        || snapped_keyframe
    {
        ctx.preview.snap.snap_line_color = Some(if snapped_guide_h || snapped_guide_v {
            status::WARNING
        } else if snapped_keyframe {
            accent::CYAN
        } else if snapped_container {
            accent::PRIMARY
        } else {
            status::SUCCESS
        });
        ctx.preview.snap.snap_hud_label = snap_hud_text.clone();
    }

    SnapResult {
        nx,
        ny,
        snapped_guide_h,
        snapped_guide_v,
        snapped_actor_h,
        snapped_actor_v,
        snapped_container,
        snapped_keyframe,
        snap_hud_text,
    }
}

// ─── Helper 4: Position-binding edit selection ──────────────────────────────

/// Emit the correct `PropertyEdit` for an actor's position based on its
/// `PositionBinding` at the current time.
pub(crate) fn emit_position_edit(actor: String, nx: f32, ny: f32, ctx: &mut PreviewContext<'_>) {
    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
    let binding = ctx
        .timeline
        .and_then(|t| t.get_track(&actor))
        .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
        .unwrap_or(PositionBinding::Absolute);

    match binding {
        PositionBinding::SceneAnchor { anchor, .. } => {
            let anchor_pt = animatix::timeline::scene_anchor_point(anchor, ctx.scene_dimensions);
            ctx.commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor,
                    property: "offset".into(),
                    value: PropertyValue::Vec2([nx - anchor_pt.x as f32, ny - anchor_pt.y as f32]),
                    create_keyframe: ctx.keyframe_mode,
                })
                .into(),
            );
        },
        PositionBinding::ScenePercent { .. } => {
            let w = ctx.scene_dimensions.width.max(1) as f32;
            let h = ctx.scene_dimensions.height.max(1) as f32;
            ctx.commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor,
                    property: "at".into(),
                    value: PropertyValue::Vec2([nx / w, ny / h]),
                    create_keyframe: ctx.keyframe_mode,
                })
                .into(),
            );
        },
        _ => {
            ctx.commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor,
                    property: "position".into(),
                    value: PropertyValue::Vec2([nx, ny]),
                    create_keyframe: ctx.keyframe_mode,
                })
                .into(),
            );
        },
    }
}

// ─── Helper 5: Drag-end keyframe creation ───────────────────────────────────

/// At the end of a drag, create keyframes for properties that were modified
/// if the timeline does not already have a keyframe at the current time.
pub(crate) fn finalize_drag_keyframes(old_drag_state: &DragState, ctx: &mut PreviewContext<'_>) {
    if let Some(tl) = ctx.timeline {
        let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
        match old_drag_state {
            DragState::Move {
                primary, actors, ..
            } => {
                if let Some(current_props) = ctx.get_actor_props(primary) {
                    if !tl.has_keyframe_at(primary, "position", time_ms) {
                        if let Some(start_pos) =
                            actors.iter().find(|(l, _)| l == primary).map(|(_, p)| *p)
                        {
                            if current_props.position != start_pos {
                                ctx.commands.push_back(
                                    DocumentCommand::PropertyEdit(PropertyEdit {
                                        time_s: None,
                                        actor: primary.clone(),
                                        property: "position".into(),
                                        value: PropertyValue::Vec2(current_props.position),
                                        create_keyframe: true,
                                    })
                                    .into(),
                                );
                            }
                        }
                    }
                }
            },
            DragState::Scale {
                actor,
                start_size,
                start_position,
                ..
            } => {
                if let Some(current_props) = ctx.get_actor_props(actor) {
                    if !tl.has_keyframe_at(actor, "size", time_ms)
                        && current_props.size != *start_size
                    {
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor.clone(),
                                property: "size".into(),
                                value: PropertyValue::Vec2(current_props.size),
                                create_keyframe: true,
                            })
                            .into(),
                        );
                    }
                    if !tl.has_keyframe_at(actor, "position", time_ms)
                        && current_props.position != *start_position
                    {
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor.clone(),
                                property: "position".into(),
                                value: PropertyValue::Vec2(current_props.position),
                                create_keyframe: true,
                            })
                            .into(),
                        );
                    }
                }
            },
            DragState::Rotate {
                actor,
                start_rotation,
                ..
            } => {
                if let Some(current_props) = ctx.get_actor_props(actor) {
                    if !tl.has_keyframe_at(actor, "rotation", time_ms)
                        && current_props.rotation != *start_rotation
                    {
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor.clone(),
                                property: "rotation".into(),
                                value: PropertyValue::Float(current_props.rotation),
                                create_keyframe: true,
                            })
                            .into(),
                        );
                    }
                }
            },
            _ => {},
        }
    }
}

// ─── Helper 6: Selected-actor start positions cache ─────────────────────────

/// Capture the initial scene-space positions of all selected actors before a
/// drag begins.  Falls back to hit-region centre when explicit props are
/// unavailable.
pub(crate) fn capture_start_positions(
    selected_actors: &HashSet<String>,
    get_actor_props: impl Fn(&str) -> Option<ActorProps>,
    hit_regions: &[(String, kurbo::Rect)],
) -> Vec<(String, [f32; 2])> {
    let mut actors = Vec::new();
    for sel in selected_actors.iter() {
        let pos = if let Some(p) = get_actor_props(sel) {
            p.position
        } else {
            hit_regions
                .iter()
                .find(|(l, _)| l == sel)
                .map(|(_, r)| [(r.x0 + r.x1) as f32 / 2.0, (r.y0 + r.y1) as f32 / 2.0])
                .unwrap_or([0.0, 0.0])
        };
        actors.push((sel.clone(), pos));
    }
    actors
}
