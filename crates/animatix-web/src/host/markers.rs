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
        BuildTarget::SingleScene(_) => {
            let mut times = Vec::new();
            collect_keyframe_stmt_times(stmts, 0.0, &mut times);
            for t in times {
                markers.push(MarkerDto {
                    t,
                    kind: "keyframe".to_string(),
                    dur: 0.0,
                });
            }
        },
        BuildTarget::MultiScene(composition) => {
            for (name, start) in &composition.scene_start_times {
                if *start > 0.0 {
                    markers.push(MarkerDto {
                        t: *start,
                        kind: "scene".to_string(),
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
                        dur: edge.transition.duration_ms as f64 / 1000.0,
                    });
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
                collect_keyframe_stmt_times(body, *start, &mut times);
                for t in times {
                    markers.push(MarkerDto {
                        t,
                        kind: "keyframe".to_string(),
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
            dur: 0.0,
        });
    }
    markers.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    markers.dedup_by(|a, b| a.kind == b.kind && (a.t - b.t).abs() < 1e-6);
    markers
}

/// Collect absolute `#Ns` keyframe-statement times, offset by `base` (the
/// owning scene's global start). Relative (`#+Ns`) keyframes are skipped:
/// their absolute time depends on playback order, and the author's beats are
/// the absolute marks.
fn collect_keyframe_stmt_times(stmts: &[Stmt], base: f64, out: &mut Vec<f64>) {
    for stmt in stmts {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                let t = match time {
                    animatix_syntax::ast::Time::Seconds(s) => *s,
                    animatix_syntax::ast::Time::Milliseconds(ms) => *ms as f64 / 1000.0,
                    // Marker placement only: beat stamps land at their
                    // default-tempo position, the same convention the outline
                    // and analyzer views use.
                    animatix_syntax::ast::Time::Beats(b) => {
                        b * animatix_syntax::ast::beat_seconds(animatix_syntax::ast::DEFAULT_BPM)
                    },
                    animatix_syntax::ast::Time::SpeechMarker { offset_ms, .. }
                    | animatix_syntax::ast::Time::SpeechWord { offset_ms, .. } => {
                        *offset_ms as f64 / 1000.0
                    },
                };
                out.push(base + t);
                collect_keyframe_stmt_times(body, base, out);
            },
            Stmt::Scene { body, .. } => collect_keyframe_stmt_times(body, base, out),
            _ => {},
        }
    }
}
