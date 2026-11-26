//! Timeline landmarks for the embed's scrubber.

use animatix::composition::BuildTarget;
use animatix_syntax::ast::Stmt;

use crate::dto::MarkerDto;
use animatix_syntax::ast::Span;
use std::collections::HashSet;

type SourcePosition = (usize, usize, usize, usize);

/// Only source-authored headers may select a line in the editable entry.
#[derive(Default)]
pub(super) struct EntryHeaders(std::collections::HashMap<Option<String>, HashSet<SourcePosition>>);

impl EntryHeaders {
    pub(super) fn collect(stmts: &[Stmt]) -> Self {
        fn collect_into(headers: &mut EntryHeaders, stmts: &[Stmt], scene: Option<&String>) {
            for stmt in stmts {
                match stmt {
                    Stmt::Scene { name, body, .. } => collect_into(headers, body, Some(name)),
                    Stmt::Keyframe {
                        span: Some(span),
                        body,
                        ..
                    }
                    | Stmt::RelativeKeyframe {
                        span: Some(span),
                        body,
                        ..
                    } => {
                        headers.0.entry(scene.cloned()).or_default().insert(position(*span));
                        collect_into(headers, body, scene);
                    },
                    _ => {},
                }
            }
        }
        let mut headers = Self::default();
        collect_into(&mut headers, stmts, None);
        headers
    }
}

fn position(span: Span) -> SourcePosition {
    (span.start_line, span.start_col, span.end_line, span.end_col)
}

/// Timeline landmarks for the embed's scrubber: the `#2s` keyframe
/// declarations the author wrote (per scene, offset by the scene's global
/// start in compositions), plus scene starts and transition windows. Sorted
/// by time, deduplicated — deliberately *not* every property-track time,
/// which stagger/assignments would explode into noise.
pub(super) fn timeline_markers(
    stmts: &[Stmt],
    target: &BuildTarget,
    entry: &EntryHeaders,
) -> Vec<MarkerDto> {
    let mut markers: Vec<MarkerDto> = Vec::new();
    match target {
        BuildTarget::SingleScene(timeline) => {
            collect_keyframe_stmt_times(stmts, timeline, 0.0, entry.0.get(&None), &mut markers);
            markers.extend(timeline.named_cues().map(|(id, t)| cue(id.to_owned(), t)));
        },
        BuildTarget::MultiScene(composition) => {
            for (name, start) in &composition.scene_start_times {
                if *start > 0.0 {
                    markers.push(MarkerDto {
                        t: *start,
                        kind: "scene".to_string(),
                        id: None,
                        line: None,
                        dur: 0.0,
                    });
                }
                // A `play` edge into this scene carries its transition; the
                // blend window is `[to_scene_start, +duration]` (the from
                // scene's tail overlaps it — composition/time.rs).
                if let Some(edge) = composition.edges.get(name)
                    && edge.transition.duration_ms > 0
                    && let Some(&incoming_start) = composition.scene_start_times.get(&edge.to_scene)
                {
                    markers.push(MarkerDto {
                        t: incoming_start,
                        kind: "transition".to_string(),
                        id: None,
                        line: None,
                        dur: edge.transition.duration_ms as f64 / 1000.0,
                    });
                }
                if let Some(scene) = composition.scenes.get(name) {
                    markers.extend(scene.timeline.named_cues().map(|(id, t)| {
                        cue(format!("{name}::{id}"), animatix::time::add_seconds(*start, t))
                    }));
                }
            }
            for stmt in stmts {
                let Stmt::Scene { name, body, .. } = stmt else {
                    continue;
                };
                let Some(start) = composition.scene_start_times.get(name) else {
                    continue;
                };
                if let Some(scene) = composition.scenes.get(name) {
                    collect_keyframe_stmt_times(
                        body,
                        &scene.timeline,
                        *start,
                        entry.0.get(&Some(name.clone())),
                        &mut markers,
                    );
                }
            }
        },
    }
    for seconds in target.pause_points_s() {
        markers.push(MarkerDto {
            t: seconds,
            kind: "step".to_string(),
            id: None,
            line: None,
            dur: 0.0,
        });
    }
    markers.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    markers.dedup_by(|a, b| {
        a.kind == b.kind && a.id == b.id && a.line == b.line && (a.t - b.t).abs() < 1e-6
    });
    markers
}

fn cue(id: String, t: f64) -> MarkerDto {
    MarkerDto {
        t,
        kind: "cue".to_owned(),
        id: Some(id),
        line: None,
        dur: 0.0,
    }
}

/// Use the compiler's resolved clock rather than evaluating source expressions
/// again. This includes relative markers, tempo and lexical duration variables.
fn collect_keyframe_stmt_times(
    stmts: &[Stmt],
    timeline: &animatix::timeline::Timeline,
    base: f64,
    entry: Option<&HashSet<SourcePosition>>,
    out: &mut Vec<MarkerDto>,
) {
    for stmt in stmts {
        match stmt {
            stmt if stmt.is_cue_only_keyframe() => {},
            Stmt::Keyframe { body, span, .. } | Stmt::RelativeKeyframe { body, span, .. } => {
                if let Some(span) = span
                    && let Some(time) = timeline.marker_time(*span)
                {
                    out.push(MarkerDto {
                        t: animatix::time::add_seconds(base, time / 1000.0),
                        kind: "keyframe".into(),
                        id: None,
                        dur: 0.0,
                        line: entry
                            .filter(|positions| positions.contains(&position(*span)))
                            .map(|_| span.start_line),
                    });
                }
                collect_keyframe_stmt_times(body, timeline, base, entry, out);
            },
            Stmt::Scene { body, .. } => {
                collect_keyframe_stmt_times(body, timeline, base, entry, out)
            },
            _ => {}, // Generated plan-local markers have no unique root source position.
        }
    }
}
