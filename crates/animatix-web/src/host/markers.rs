//! Timeline landmarks for the embed's scrubber.

use animatix::composition::BuildTarget;
use animatix_syntax::ast::Stmt;

use crate::dto::MarkerDto;

/// Timeline landmarks for the embed's scrubber: the `#2s` keyframe
/// declarations the author wrote (per scene, offset by the scene's global
/// start in compositions), plus scene starts and transition windows. Sorted
/// by time, deduplicated — deliberately *not* every property-track time,
/// which stagger/assignments would explode into noise.
pub(super) fn timeline_markers(stmts: &[Stmt], target: &BuildTarget) -> Vec<MarkerDto> {
    let mut markers: Vec<MarkerDto> = Vec::new();
    match target {
        BuildTarget::SingleScene(timeline) => {
            let mut times = Vec::new();
            collect_keyframe_stmt_times(stmts, timeline, 0.0, &mut times);
            for t in times {
                markers.push(MarkerDto {
                    t,
                    kind: "keyframe".to_string(),
                    id: None,
                    dur: 0.0,
                });
            }
            markers.extend(timeline.named_cues().map(|(id, t)| cue(id.to_owned(), t)));
        },
        BuildTarget::MultiScene(composition) => {
            for (name, start) in &composition.scene_start_times {
                if *start > 0.0 {
                    markers.push(MarkerDto {
                        t: *start,
                        kind: "scene".to_string(),
                        id: None,
                        dur: 0.0,
                    });
                }
                // A `play` edge into this scene carries its transition; the
                // blend window is `[to_scene_start, +duration]` (the from
                // scene's tail overlaps it — composition/time.rs).
                if let Some(edge) = composition.edges.get(name)
                    && edge.transition.duration_ms > 0
                {
                    markers.push(MarkerDto {
                        t: *start,
                        kind: "transition".to_string(),
                        id: None,
                        dur: edge.transition.duration_ms as f64 / 1000.0,
                    });
                }
                if let Some(scene) = composition.scenes.get(name) {
                    markers.extend(
                        scene
                            .timeline
                            .named_cues()
                            .map(|(id, t)| cue(format!("{name}::{id}"), start + t)),
                    );
                }
            }
            for stmt in stmts {
                let Stmt::Scene { name, body, .. } = stmt else {
                    continue;
                };
                let Some(start) = composition.scene_start_times.get(name) else {
                    continue;
                };
                let mut times = Vec::new();
                if let Some(scene) = composition.scenes.get(name) {
                    collect_keyframe_stmt_times(body, &scene.timeline, *start, &mut times);
                }
                for t in times {
                    markers.push(MarkerDto {
                        t,
                        kind: "keyframe".to_string(),
                        id: None,
                        dur: 0.0,
                    });
                }
            }
        },
    }
    for &ms in target.pause_points() {
        markers.push(MarkerDto {
            t: ms as f64 / 1000.0,
            kind: "step".to_string(),
            id: None,
            dur: 0.0,
        });
    }
    markers.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    markers.dedup_by(|a, b| a.kind == b.kind && a.id == b.id && (a.t - b.t).abs() < 1e-6);
    markers
}

fn cue(id: String, t: f64) -> MarkerDto {
    MarkerDto {
        t,
        kind: "cue".to_owned(),
        id: Some(id),
        dur: 0.0,
    }
}

/// Use the compiler's resolved clock rather than evaluating source expressions
/// again. This includes relative markers, tempo and lexical duration variables.
fn collect_keyframe_stmt_times(
    stmts: &[Stmt],
    timeline: &animatix::timeline::Timeline,
    base: f64,
    out: &mut Vec<f64>,
) {
    for stmt in stmts {
        match stmt {
            stmt if stmt.is_cue_only_keyframe() => {},
            Stmt::Keyframe { body, span, .. } | Stmt::RelativeKeyframe { body, span, .. } => {
                if let Some(time) = span.and_then(|span| timeline.marker_time(span)) {
                    out.push(base + time / 1000.0);
                }
                collect_keyframe_stmt_times(body, timeline, base, out);
            },
            Stmt::Scene { body, .. } => collect_keyframe_stmt_times(body, timeline, base, out),
            _ => {}, // Generated plan-local markers have no unique root source position.
        }
    }
}
