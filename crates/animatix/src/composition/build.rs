use std::collections::{BTreeMap, HashMap};

use crate::ast::{Expr, Property, Stmt, Transition};
use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::module::Namespace;
use crate::timeline::Timeline;

use super::{Composition, CompositionScene, SceneEdge};

fn build_timeline_with_context(
    statements: &[Stmt],
    namespaces: &HashMap<String, Namespace>,
    font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    build_quality: crate::timeline::BuildQuality,
    asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
    context: Option<std::sync::Arc<crate::extension_context::ExtensionContext>>,
) -> BuildReport<Timeline> {
    match context {
        Some(ctx) => {
            Timeline::build_with_diagnostics_and_font_context_and_asset_cache_and_extension_context(
                statements,
                namespaces,
                font_context,
                build_quality,
                asset_cache,
                ctx,
            )
        },
        None => Timeline::build_with_diagnostics_and_font_context_and_asset_cache(
            statements,
            namespaces,
            font_context,
            build_quality,
            asset_cache,
        ),
    }
}

fn build_timeline_with_carry_context(
    statements: &[Stmt],
    namespaces: &HashMap<String, Namespace>,
    font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    build_quality: crate::timeline::BuildQuality,
    carry: Option<&crate::timeline::persistence::CarryBag>,
    source_timeline: Option<&Timeline>,
    source_duration_ms: u64,
    dims: [f64; 2],
    asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
    context: Option<std::sync::Arc<crate::extension_context::ExtensionContext>>,
) -> BuildReport<Timeline> {
    match context {
        Some(ctx) => Timeline::build_with_carry_and_asset_cache_and_extension_context(
            statements,
            namespaces,
            font_context,
            build_quality,
            carry,
            source_timeline,
            source_duration_ms,
            dims,
            asset_cache,
            ctx,
        ),
        None => Timeline::build_with_carry_and_asset_cache(
            statements,
            namespaces,
            font_context,
            build_quality,
            carry,
            source_timeline,
            source_duration_ms,
            dims,
            asset_cache,
        ),
    }
}

fn validate_play_target(
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

fn resolve_namespace<'a>(
    namespaces: &'a HashMap<String, Namespace>,
    parts: &[&str],
) -> Option<&'a Namespace> {
    let mut current = namespaces.get(*parts.first()?)?;
    for part in &parts[1..] {
        current = current.namespaces.get(*part)?;
    }
    Some(current)
}

impl Composition {
    /// Build a `Composition` from parsed AST statements with a shared `FontContext`.
    pub fn build_with_font_context(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: crate::timeline::BuildQuality,
    ) -> BuildReport<Self> {
        Self::build_with_font_context_impl(
            statements,
            namespaces,
            font_context,
            build_quality,
            None,
        )
    }

    pub(crate) fn build_with_font_context_impl(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: crate::timeline::BuildQuality,
        asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
    ) -> BuildReport<Self> {
        Self::build_with_font_context_impl_with_context(
            statements,
            namespaces,
            font_context,
            build_quality,
            asset_cache,
            None,
        )
    }

    pub(crate) fn build_with_font_context_impl_with_context(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: crate::timeline::BuildQuality,
        asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
        context: Option<std::sync::Arc<crate::extension_context::ExtensionContext>>,
    ) -> BuildReport<Self> {
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut scenes: BTreeMap<String, CompositionScene> = BTreeMap::new();
        let mut declaration_order: Vec<String> = Vec::new();
        let mut edges: BTreeMap<String, SceneEdge> = BTreeMap::new();
        let mut shared_prelude: Vec<Stmt> = Vec::new();
        // Temporary storage: scene_name → intermediate play target extracted from raw body.
        let mut play_targets: BTreeMap<String, (String, Option<Transition>, Option<f64>)> =
            BTreeMap::new();
        // Cache merged bodies for carry-injection rebuild (scene_name → merged AST).
        let mut merged_bodies: BTreeMap<String, Vec<Stmt>> = BTreeMap::new();

        // 1. Separate shared prelude from scene blocks
        let mut in_scene = false;

        for stmt in statements {
            match stmt {
                Stmt::Scene {
                    name,
                    config,
                    body,
                    span,
                } => {
                    if scenes.contains_key(name) {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::DuplicateSceneName,
                                DiagnosticPhase::Build,
                                format!("Duplicate scene name '{}'.", name),
                            )
                            .with_subject(name),
                        );
                        continue;
                    }

                    // Extract play target from the scene body BEFORE merging with prelude.
                    // Warn on multiple play statements (only the first is used).
                    let play_target = Self::extract_play_stmt(body, name, &mut diagnostics);

                    // Warn on scene-level config keys that are composition-scoped
                    // and cannot meaningfully override the prelude.
                    Self::validate_scene_config(config, name, &mut diagnostics);

                    // Build the per-scene timeline: merge prelude + scene body
                    let mut merged_body = shared_prelude.clone();
                    // Insert scene config as a Stmt::Config before the scene body
                    merged_body.push(Stmt::Config {
                        settings: config.clone(),
                        span: None,
                    });
                    merged_body.extend(body.clone());
                    merged_bodies.insert(name.clone(), merged_body.clone());

                    let build_report = build_timeline_with_context(
                        &merged_body,
                        namespaces,
                        font_context.clone(),
                        build_quality,
                        asset_cache.clone(),
                        context.clone(),
                    );
                    diagnostics.extend(
                        build_report
                            .diagnostics
                            .into_iter()
                            .map(|d| d.with_subject(format!("scene '{}'", name))),
                    );
                    let timeline = build_report.output;
                    let inferred_duration = timeline.duration_seconds();

                    // Check for explicit duration in scene config
                    let explicit_duration_s = Self::extract_duration_from_config(config);
                    let play_time_s = play_target.as_ref().and_then(|(_, _, t)| *t);
                    let duration_s = explicit_duration_s.unwrap_or_else(|| {
                        // The scene must run long enough for its `play` to
                        // fire; keyframe-only inference cuts it short.
                        play_time_s.map(|t| inferred_duration.max(t)).unwrap_or(inferred_duration)
                    });

                    if let Some(target) = play_target {
                        play_targets.insert(name.clone(), target);
                    }

                    scenes.insert(
                        name.clone(),
                        CompositionScene {
                            name: name.clone(),
                            config: config.clone(),
                            timeline,
                            duration_s,
                            explicit_duration_s,
                            source_span: *span,
                        },
                    );

                    declaration_order.push(name.clone());
                    in_scene = true;
                },
                other => {
                    if !in_scene {
                        // Top-level statements before the first scene are the shared prelude.
                        match other {
                            Stmt::Import { .. }
                            | Stmt::TypeAlias { .. }
                            | Stmt::LetDecl { is_pub: true, .. }
                            | Stmt::Config { .. }
                            | Stmt::ComponentDef(..)
                            | Stmt::Comment(..) => {
                                shared_prelude.push(other.clone());
                            },
                            _ => {
                                // Other top-level statements before any scene are unusual.
                                // We keep them in the prelude but warn.
                            },
                        }
                    }
                    // Statements already inside scene bodies are handled by
                    // the per-scene timeline build.
                },
            }
        }

        // 2b. Collect cross-file scenes from namespaces, transitively.
        // When a play target references an imported scene (e.g. "play alias.SceneName"),
        // build a timeline from the imported scene's data and register it — then
        // process the newly registered scene's OWN play target so multi-hop
        // chains (Hub -> logo.Logo -> slogan.Slogan -> ...) resolve fully.
        // (The collection used to be single-hop: chains broke after one hop.)
        let mut cross_queue: Vec<String> = play_targets
            .values()
            .map(|(target, _, _)| target.clone())
            .filter(|t| t.contains('.') && !scenes.contains_key(t))
            .collect();
        let mut queued: std::collections::HashSet<String> =
            cross_queue.iter().cloned().collect();
        while let Some(target) = cross_queue.pop() {
            let parts: Vec<&str> = target.split('.').collect();
            if parts.len() < 2 {
                continue;
            }
            let (namespace_parts, scene_name) = parts.split_at(parts.len() - 1);
            let scene_name = scene_name[0];
            let Some(ns) = resolve_namespace(namespaces, namespace_parts) else {
                continue;
            };
            if let Some(scene_data) = ns.scenes.get(scene_name) {
                // Build timeline from the cross-file scene's prelude + body
                let mut merged = scene_data.file_prelude.clone();
                // Insert scene config as a Stmt::Config before the scene body
                merged.push(Stmt::Config {
                    settings: scene_data.config.clone(),
                    span: scene_data.span,
                });
                merged.extend(scene_data.body.clone());
                merged_bodies.insert(target.clone(), merged.clone());
                let build_report = build_timeline_with_context(
                    &merged,
                    namespaces,
                    font_context.clone(),
                    build_quality,
                    asset_cache.clone(),
                    context.clone(),
                );
                diagnostics.extend(
                    build_report
                        .diagnostics
                        .into_iter()
                        .map(|d| d.with_subject(format!("scene '{}'", target))),
                );
                let timeline = build_report.output;
                let explicit_duration_s = Self::extract_duration_from_config(&scene_data.config);
                let duration_s = explicit_duration_s.unwrap_or_else(|| timeline.duration_seconds());
                scenes.insert(
                    target.clone(),
                    CompositionScene {
                        name: target.clone(),
                        config: scene_data.config.clone(),
                        timeline,
                        duration_s,
                        explicit_duration_s,
                        source_span: scene_data.span,
                    },
                );
                declaration_order.push(target.clone());

                // Chain: queue the newly registered scene's own play target.
                if let Some((next_target, next_transition, next_play_time)) =
                    Self::extract_play_stmt(&scene_data.body, &target, &mut diagnostics)
                {
                    play_targets.insert(
                        target.clone(),
                        (next_target.clone(), next_transition, next_play_time),
                    );
                    if next_target.contains('.')
                        && !scenes.contains_key(&next_target)
                        && !queued.contains(&next_target)
                    {
                        queued.insert(next_target.clone());
                        cross_queue.push(next_target);
                    }
                }
            }
        }

        // 2c-edges. Resolve play edges from stored play_targets (entry scenes
        // and every cross-file scene registered above).
        for name in &declaration_order {
            if let Some((target, transition, _play_time)) = play_targets.get(name) {
                let _ = validate_play_target(target, &scenes, namespaces, &mut diagnostics, name);
                edges.insert(
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

        // 2c. Floor inferred scene durations so a scene with no keyframes still
        // lasts long enough for its incoming transition to complete. Without
        // this, a zero-duration target scene collapses the composition's global
        // timeline and clamps playback before outgoing actions can finish.
        let mut reverse_edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (from, edge) in &edges {
            reverse_edges.entry(edge.to_scene.clone()).or_default().push(from.clone());
        }
        for scene in scenes.values_mut() {
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

        // 3. Compute walk order (following edges, with cycle detection)
        let walk_order =
            Self::compute_walk_order(&declaration_order, &edges, &scenes, &mut diagnostics);

        // 3.5: Walk-order carry injection
        //
        // For each scene (starting from the second) in walk order, compute the
        // predecessor's carry bag and rebuild the scene timeline with carry
        // injection.  Scenes without any persistent predecessor actors are
        // rebuilt identically to the first pass (carry bag is empty → fast path).
        {
            // Default scene dimensions used for Phase 3 layout re-rooting.
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
                        diagnostics.push(
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
                    if let Some(pred) = scenes.get(&pred_name) {
                        for (label, &flag) in &pred.timeline.persistence_flags {
                            if flag {
                                diagnostics.push(
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
                    let pred = match scenes.get(&pred_name) {
                        Some(s) => s,
                        None => continue,
                    };
                    let ms = (pred.duration_s * 1000.0) as u64;
                    let bag = pred.timeline.compute_carry_bag(ms, has_successor);
                    // Clone only when there are entries to carry.
                    if bag.entries.is_empty() {
                        continue; // nothing to carry — keep existing timeline
                    }
                    (bag, ms, pred.timeline.clone())
                };

                // Retrieve the merged AST body for this scene.
                let merged_body = match merged_bodies.get(&scene_name) {
                    Some(b) => b.clone(),
                    None => continue,
                };

                // Discard first-pass diagnostics for this scene; the carry-
                // aware rebuild will produce the authoritative diagnostics.
                let scene_subject = format!("scene '{}'", scene_name);
                diagnostics
                    .retain(|d| d.location.subject.as_deref() != Some(scene_subject.as_str()));

                // Rebuild with carry injection.
                let build_report = build_timeline_with_carry_context(
                    &merged_body,
                    namespaces,
                    font_context.clone(),
                    build_quality,
                    Some(&carry_bag),
                    Some(&pred_timeline_clone),
                    pred_duration_ms,
                    default_dims,
                    asset_cache.clone(),
                    context.clone(),
                );

                diagnostics.extend(
                    build_report
                        .diagnostics
                        .into_iter()
                        .map(|d| d.with_subject(format!("scene '{}'", scene_name))),
                );

                let new_timeline = build_report.output;
                let inferred = new_timeline.duration_seconds();

                if let Some(scene) = scenes.get_mut(&scene_name) {
                    let duration_s = scene.explicit_duration_s.unwrap_or(inferred);
                    scene.timeline = new_timeline;
                    scene.duration_s = duration_s;
                }
            }
        }

        // 3.6: Warn when the last scene in the walk order itself has persistent actors
        // (persistence flags set but no successor to carry them into).
        if let Some(last_scene_name) = walk_order.last() {
            if let Some(last_scene) = scenes.get(last_scene_name) {
                for (label, &flag) in &last_scene.timeline.persistence_flags {
                    if flag {
                        // Only warn if this scene really has no outgoing edge.
                        let has_outgoing = edges.contains_key(last_scene_name);
                        if !has_outgoing {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::PersistTargetNotCarried,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "Actor '{}' is persisted in scene '{}' but has no \
                                         successor scene to carry into.",
                                        label, last_scene_name,
                                    ),
                                )
                                .with_subject(last_scene_name),
                            );
                        }
                    }
                }
            }
        }

        // 4. Compute global timeline
        let mut scene_start_times: BTreeMap<String, f64> = BTreeMap::new();
        let mut current_time: f64 = 0.0;

        for name in &walk_order {
            if let Some(scene) = scenes.get(name) {
                scene_start_times.insert(name.clone(), current_time);
                current_time += scene.duration_s;

                // Apply transition overlap
                if let Some(edge) = edges.get(name) {
                    if edge.transition.duration_ms > 0 {
                        let overlap_s =
                            (edge.transition.duration_ms as f64 / 1000.0).min(scene.duration_s); // Clamp to scene duration
                        current_time -= overlap_s;
                    }
                }
            }
        }

        let global_duration_s = current_time;

        BuildReport::new(
            Composition {
                scenes,
                declaration_order,
                edges,
                global_duration_s,
                scene_start_times,
            },
            diagnostics,
        )
    }

    /// Extract the first `play` statement from a scene body.
    /// Emits a diagnostic if multiple `play` statements are found (only the first is used).
    fn extract_play_stmt(
        body: &[Stmt],
        scene_name: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<(String, Option<Transition>, Option<f64>)> {
        let mut result: Option<(String, Option<Transition>, Option<f64>)> = None;
        // The play's time comes from the keyframe marker that precedes it
        // (`#4s` + `play Next`). The parser emits the empty keyframe and the
        // play as sibling statements, so track the running keyframe time.
        let mut current_time: f64 = 0.0;
        for stmt in body {
            match stmt {
                Stmt::Keyframe { time, .. } | Stmt::RelativeKeyframe { offset: time, .. } => {
                    current_time = match time {
                        crate::ast::Time::Seconds(s) => *s,
                        crate::ast::Time::Milliseconds(ms) => *ms as f64 / 1000.0,
                    };
                },
                Stmt::Play {
                    scene_name: target,
                    transition,
                    ..
                } => {
                    if let Some(ref prev_target) = result {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::MultiplePlayTargets,
                                DiagnosticPhase::Build,
                                format!(
                                    "Scene '{}' has multiple `play` statements; only the first target '{}' is used.",
                                    scene_name,
                                    prev_target.0,
                                ),
                            )
                            .with_subject(scene_name),
                        );
                    } else {
                        result = Some((target.clone(), transition.clone(), Some(current_time)));
                    }
                },
                _ => {},
            }
        }
        result
    }

    /// Extract explicit duration from scene config properties.
    /// Looks for a `duration` property with a numeric value (in seconds).
    fn extract_duration_from_config(config: &[Property]) -> Option<f64> {
        for prop in config {
            if prop.name == "duration" {
                if let Expr::Num(val) = prop.value {
                    return Some(val);
                }
            }
        }
        None
    }

    /// Warn on scene-level config keys that are composition-scoped or
    /// program-scoped and cannot meaningfully be overridden per-scene.
    ///
    /// Scene-scoped keys: `colorscheme`, `dynamic_layout`, `duration`.
    /// Everything else is either composition-scoped (inherited from the
    /// shared prelude) or unknown.
    fn validate_scene_config(
        config: &[Property],
        scene_name: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Keys that are valid at scene level.
        const SCENE_SCOPED_KEYS: &[&str] = &["colorscheme", "dynamic_layout", "duration"];

        for prop in config {
            if SCENE_SCOPED_KEYS.contains(&prop.name.as_str()) {
                continue;
            }
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidConfigValue,
                    DiagnosticPhase::Build,
                    format!(
                        "Config key '{}' in scene '{}' is composition-scoped and will be ignored; set it in the top-level config block instead.",
                        prop.name, scene_name,
                    ),
                )
                .with_subject(scene_name),
            );
        }
    }

    /// Compute walk order of scenes, following explicit `play` edges.
    /// Falls back to declaration order when no explicit edges exist.
    /// Detects and reports cycles and orphan scenes.
    fn compute_walk_order(
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
}
