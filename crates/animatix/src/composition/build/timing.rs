//! Timing: duration floors, walk order, carry, global map.
//!
//! Turns the collected scene graph into playback timing. Two things here are
//! load-bearing beyond arithmetic: the duration floor (a zero-duration scene
//! would collapse the global timeline before its incoming transition finishes)
//! and the carry rebuild (a scene whose predecessor persists actors is built a
//! second time with the predecessor's bag injected).

use std::collections::BTreeMap;

use super::builder::SceneCollection;
use super::edges::compute_walk_order;
use super::*;

/// Floor durations, order the walk, inject carried state.
///
/// Mutates `state.scenes` in place: durations are floored so every transition
/// fits, and a scene with carried state is rebuilt (its first-pass diagnostics
/// are dropped in favour of the rebuild's authoritative ones).
pub(super) fn resolve_timing(
    namespaces: &std::collections::HashMap<String, Namespace>,
    options: &BuildOptions<'_>,
    state: &mut SceneCollection,
) {
    // Floor inferred scene durations so a scene with no keyframes still
    // lasts long enough for its incoming transition to complete. Without
    // this, a zero-duration target scene collapses the composition's global
    // timeline and clamps playback before outgoing actions can finish.
    let mut reverse_edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (from, edge) in &state.edges {
        reverse_edges.entry(edge.to_scene.clone()).or_default().push(from.clone());
    }
    let edges = &state.edges;
    for scene in state.scenes.values_mut() {
        if scene.explicit_duration_s.is_some() {
            continue;
        }
        let incoming_s = reverse_edges
            .get(&scene.name)
            .and_then(|preds| preds.first())
            .and_then(|pred| edges.get(pred))
            .map(|edge| edge.transition.duration_ms as f64 / 1000.0)
            .unwrap_or(0.0);
        let floor_s = incoming_s.max(1.0 / 60.0); // at least one frame
        if scene.duration_s < floor_s {
            scene.duration_s = floor_s;
        }
    }

    // Compute walk order (following edges, with cycle detection)
    let walk_order = compute_walk_order(
        &state.declaration_order,
        &state.edges,
        &state.scenes,
        &mut state.diagnostics,
    );
    state.walk_order = walk_order.clone();

    inject_carried_state(namespaces, options, &walk_order, &reverse_edges, state);

    // Warn when the last scene in the walk order itself has persistent actors
    // (persistence flags set but no successor to carry them into).
    if let Some(last_scene_name) = walk_order.last() {
        if let Some(last_scene) = state.scenes.get(last_scene_name) {
            for (label, flag) in last_scene.timeline.persistence_flags_iter() {
                if flag {
                    // Only warn if this scene really has no outgoing edge.
                    let has_outgoing = state.edges.contains_key(last_scene_name);
                    if !has_outgoing {
                        // The camera is not an actor, so the warning must not
                        // call it one — `persist camera` is a reserved target.
                        let subject = if label == animatix_core::property::CAMERA_TARGET {
                            "The scene camera".to_string()
                        } else {
                            format!("Actor '{}'", label)
                        };
                        state.diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::PersistTargetNotCarried,
                                DiagnosticPhase::Build,
                                format!(
                                    "{} is persisted in scene '{}' but has no \
                                     successor scene to carry into.",
                                    subject, last_scene_name,
                                ),
                            )
                            .with_subject(last_scene_name),
                        );
                    }
                }
            }
        }
    }
}

/// Walk-order carry injection.
///
/// For each scene (starting from the second) in walk order, compute the
/// predecessor's carry bag and rebuild the scene timeline with carry injection.
/// Scenes without any persistent predecessor actors are not rebuilt: the carry
/// bag is empty, so the first-pass timeline already is the answer.
fn inject_carried_state(
    namespaces: &std::collections::HashMap<String, Namespace>,
    options: &BuildOptions<'_>,
    walk_order: &[String],
    reverse_edges: &BTreeMap<String, Vec<String>>,
    state: &mut SceneCollection,
) {
    // Default scene dimensions used for layout re-rooting.
    let default_dims = [
        crate::timeline::SceneDimensions::default().width as f64,
        crate::timeline::SceneDimensions::default().height as f64,
    ];

    for i in 1..walk_order.len() {
        let scene_name = walk_order[i].clone();
        let pred_name = walk_order[i - 1].clone();

        // Determine if this scene has a successor in walk order.
        let has_successor = i + 1 < walk_order.len();

        // Warn when a scene has more than one predecessor (diamond topology).
        if let Some(preds) = reverse_edges.get(&scene_name) {
            if preds.len() > 1 {
                state.diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::CarryAmbiguousPredecessor,
                        DiagnosticPhase::Build,
                        format!(
                            "Scene '{}' is targeted by {} predecessor scenes; \
                             carry bag uses walk-order predecessor '{}' only.",
                            scene_name,
                            preds.len(),
                            pred_name,
                        ),
                    )
                    .with_subject(&scene_name),
                );
            }
        }

        // Emit PersistTargetNotCarried for the predecessor if it's the last scene.
        if !has_successor {
            if let Some(pred) = state.scenes.get(&pred_name) {
                for (label, flag) in pred.timeline.persistence_flags_iter() {
                    if flag {
                        state.diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::PersistTargetNotCarried,
                                DiagnosticPhase::Build,
                                format!(
                                    "Actor '{}' is persisted in scene '{}' but has no \
                                     successor scene to carry into.",
                                    label, pred_name,
                                ),
                            )
                            .with_subject(&pred_name),
                        );
                    }
                }
            }
        }

        // Compute carry bag from predecessor timeline.
        let (carry_bag, pred_duration_ms, pred_timeline_clone) = {
            let pred = match state.scenes.get(&pred_name) {
                Some(s) => s,
                None => continue,
            };
            let ms = (pred.duration_s * 1000.0) as u64;
            let bag = pred.timeline.compute_carry_bag(ms, has_successor);
            // Clone only when there is something to carry. The camera is
            // not an entry, so `persist camera` alone has to count here
            // too — otherwise a scene that carries only the transform
            // keeps the timeline built without it.
            if bag.entries.is_empty() && bag.camera.is_none() {
                continue; // nothing to carry — keep existing timeline
            }
            (bag, ms, pred.timeline.clone())
        };

        // Retrieve the merged AST body for this scene.
        let merged_body = match state.merged_bodies.get(&scene_name) {
            Some(body) => body.clone(),
            None => continue,
        };

        // Discard first-pass diagnostics for this scene; the carry-aware
        // rebuild will produce the authoritative diagnostics.
        let scene_subject = format!("scene '{}'", scene_name);
        state
            .diagnostics
            .retain(|d| d.location.subject.as_deref() != Some(scene_subject.as_str()));

        // Rebuild with carry injection.
        let scene_options = BuildOptions {
            carry: Some(crate::timeline::CarryInput {
                bag: &carry_bag,
                source_timeline: &pred_timeline_clone,
                source_duration_ms: pred_duration_ms,
                dims: default_dims,
            }),
            ..options.clone()
        };
        let build_report = Timeline::build_with_options(&merged_body, namespaces, scene_options);

        state.diagnostics.extend(
            build_report
                .diagnostics
                .into_iter()
                .map(|d| d.with_subject(format!("scene '{}'", scene_name))),
        );

        let new_timeline = build_report.output;
        let inferred = new_timeline.duration_seconds();

        if let Some(scene) = state.scenes.get_mut(&scene_name) {
            let duration_s = scene.explicit_duration_s.unwrap_or(inferred);
            scene.timeline = new_timeline;
            scene.duration_s = duration_s;
        }
    }
}

/// Compute global start times, total duration and pause points.
pub(super) fn finalize_global_timeline(
    scenes: BTreeMap<String, CompositionScene>,
    walk_order: &[String],
    declaration_order: &[String],
    edges: &BTreeMap<String, SceneEdge>,
) -> Composition {
    let mut scene_start_times: BTreeMap<String, f64> = BTreeMap::new();
    let mut current_time: f64 = 0.0;

    for name in walk_order {
        if let Some(scene) = scenes.get(name) {
            scene_start_times.insert(name.clone(), current_time);
            current_time += scene.duration_s;

            // Apply transition overlap
            if let Some(edge) = edges.get(name) {
                if edge.transition.duration_ms > 0 {
                    let overlap_s =
                        (edge.transition.duration_ms as f64 / 1000.0).min(scene.duration_s);
                    current_time -= overlap_s;
                }
            }
        }
    }

    let global_duration_s = current_time;

    let mut pause_points: Vec<u64> = Vec::new();
    for (name, start_s) in &scene_start_times {
        if let Some(scene) = scenes.get(name) {
            let start_ms = (*start_s * 1000.0).round() as u64;
            for &local_p in &scene.timeline.pause_points {
                pause_points.push(start_ms + local_p);
            }
        }
    }
    pause_points.sort_unstable();
    pause_points.dedup();

    Composition {
        scenes,
        declaration_order: declaration_order.to_vec(),
        edges: edges.clone(),
        global_duration_s,
        scene_start_times,
        pause_points,
    }
}
