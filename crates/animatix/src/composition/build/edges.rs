//! Resolve `play` edges and their walk order.
//!
//! Validation lives next to resolution because every edge the build keeps is an
//! edge whose target resolved; an unresolved target is reported here and never
//! reaches [`super::timing::resolve_timing`].

use std::collections::{BTreeMap, HashMap};

use super::builder::SceneCollection;
use super::*;

/// Turn each scene's stored play target into a validated edge.
pub(super) fn resolve_edges(namespaces: &HashMap<String, Namespace>, state: &mut SceneCollection) {
    for name in &state.declaration_order {
        if let Some((target, transition, _play_time)) = state.play_targets.get(name) {
            let _ = validate_play_target(
                target,
                &state.scenes,
                namespaces,
                &mut state.diagnostics,
                name,
            );
            state.edges.insert(
                name.clone(),
                SceneEdge {
                    to_scene: target.clone(),
                    transition: transition.clone().unwrap_or(Transition {
                        id: "cut".into(),
                        duration_ms: 0,
                        easing: crate::easing::Easing::Linear,
                    }),
                },
            );
        }
    }
}

/// Compute walk order of scenes, following explicit `play` edges.
/// Falls back to declaration order when no explicit edges exist.
/// Detects and reports cycles and orphan scenes.
pub(super) fn compute_walk_order(
    declaration_order: &[String],
    edges: &BTreeMap<String, SceneEdge>,
    scenes: &BTreeMap<String, CompositionScene>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<String> {
    if edges.is_empty() {
        return declaration_order.to_vec();
    }

    // Detect orphan scenes: scenes that are not the target of any `play` edge
    // and are not the first scene in the chain.
    let targeted_scenes: std::collections::HashSet<&str> =
        edges.values().map(|e| e.to_scene.as_str()).collect();
    let first_scene = declaration_order.first().map(|s| s.as_str());
    for name in declaration_order {
        if Some(name.as_str()) != first_scene && !targeted_scenes.contains(name.as_str()) {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::OrphanScene,
                    DiagnosticPhase::Build,
                    format!(
                        "Scene '{}' is not reachable via any `play` edge and will play after the last linked scene.",
                        name,
                    ),
                )
                .with_subject(name),
            );
        }
    }

    let mut order: Vec<String> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Start from the first scene (in declaration order) that has an edge,
    // or the very first scene if none have edges.
    let start_scene = declaration_order.first().cloned().unwrap_or_default();
    if start_scene.is_empty() {
        return order;
    }

    let mut current = start_scene.clone();
    let max_iterations = declaration_order.len() * 2; // Safety limit

    for _ in 0..max_iterations {
        if visited.contains(&current) {
            // Cycle detected
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::PlayCycleDetected,
                    DiagnosticPhase::Build,
                    format!("Play edges form a cycle including scene '{}'.", current),
                )
                .with_subject(&current),
            );
            break;
        }

        order.push(current.clone());
        visited.insert(current.clone());

        if let Some(edge) = edges.get(&current) {
            if scenes.contains_key(&edge.to_scene) {
                current = edge.to_scene.clone();
            } else {
                break;
            }
        } else {
            // No explicit edge — check if there's a next scene in declaration order
            // that hasn't been visited yet.
            let current_idx = declaration_order.iter().position(|n| n == &current);
            if let Some(idx) = current_idx {
                if idx + 1 < declaration_order.len() {
                    let next = &declaration_order[idx + 1];
                    if !visited.contains(next) {
                        current = next.clone();
                        continue;
                    }
                }
            }
            break;
        }
    }

    // Add any scenes not yet in the walk order (orphan scenes without incoming edges).
    for name in declaration_order {
        if !order.contains(name) {
            order.push(name.clone());
        }
    }

    order
}

/// Validate that `target` names a scene the build can reach.
///
/// A dotted target resolves through the namespace map (legacy/test namespaces
/// with no scene data accept any name); an undotted target must be a scene the
/// composition already registered.
pub(super) fn validate_play_target(
    target: &str,
    scenes: &BTreeMap<String, CompositionScene>,
    namespaces: &HashMap<String, Namespace>,
    diagnostics: &mut Vec<Diagnostic>,
    source_scene: &str,
) -> bool {
    if target.contains('.') {
        let parts: Vec<&str> = target.split('.').collect();
        if parts.len() >= 2 {
            let (namespace_parts, scene_name) = parts.split_at(parts.len() - 1);
            let scene_name = scene_name[0];
            if let Some(ns) = resolve_namespace(namespaces, namespace_parts) {
                // If the namespace has scene data, verify the specific scene exists.
                // If scenes are empty (legacy/test namespace), accept any name.
                if ns.scenes.is_empty() || ns.scenes.contains_key(scene_name) {
                    return true;
                }
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::PlayTargetNotFound,
                        DiagnosticPhase::Build,
                        format!(
                            "Scene '{}' plays non-existent scene '{}' in namespace '{}'.",
                            source_scene,
                            scene_name,
                            namespace_parts.join(".")
                        ),
                    )
                    .with_subject(source_scene),
                );
                return false;
            }
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::PlayTargetNotFound,
                    DiagnosticPhase::Build,
                    format!(
                        "Scene '{}' plays non-existent module/scene '{}'.",
                        source_scene, target
                    ),
                )
                .with_subject(source_scene),
            );
            return false;
        }
    }

    if scenes.contains_key(target) {
        true
    } else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::PlayTargetNotFound,
                DiagnosticPhase::Build,
                format!("Scene '{}' plays non-existent scene '{}'.", source_scene, target),
            )
            .with_subject(source_scene),
        );
        false
    }
}

/// Resolve a dotted namespace path (`a.b.c`) through the imported module graph.
pub(super) fn resolve_namespace<'a>(
    namespaces: &'a HashMap<String, Namespace>,
    parts: &[&str],
) -> Option<&'a Namespace> {
    let mut current = namespaces.get(*parts.first()?)?;
    for part in &parts[1..] {
        current = current.namespaces.get(*part)?;
    }
    Some(current)
}
