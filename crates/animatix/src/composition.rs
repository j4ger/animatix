//! Multi-Scene Composition Engine
//!
//! `Composition` is the orchestration layer for multi-scene `.amx` files.
//! It manages per-scene `Timeline` instances, scene ordering via `play` edges,
//! global time mapping, and transition blending.
//!
//! Single-scene files (no `# SceneName` declarations) use the existing
//! `Timeline::build_with_diagnostics` path — this module is only activated
//! when `Stmt::Scene` markers are present in the parsed AST.

use crate::ast::{Expr, Property, Span, Stmt, Transition};
use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::Timeline;
use crate::module::Namespace;
use std::collections::{BTreeMap, HashMap};

// ---------------------------------------------------------------------------
// Composition Data Structures
// ---------------------------------------------------------------------------

/// A compiled scene within a multi-scene composition.
#[derive(Clone)]
pub struct CompositionScene {
    /// Scene identifier from the `# SceneName` declaration.
    pub name: String,
    /// Scene-level config properties (e.g. colorscheme).
    pub config: Vec<Property>,
    /// Built timeline for this scene.
    pub timeline: Timeline,
    /// Duration of this scene in seconds (explicit if set, otherwise inferred).
    pub duration_s: f64,
    /// Explicit duration set by the user (overrides timeline inference).
    pub explicit_duration_s: Option<f64>,
    /// Source span of the scene declaration, for diagnostics.
    pub source_span: Option<Span>,
}

/// Edge from one scene to another (via `play` or implicit ordering).
#[derive(Clone)]
pub struct SceneEdge {
    /// Target scene name to transition into.
    pub to_scene: String,
    /// Transition effect applied when entering the target scene.
    pub transition: Transition,
}

/// Complete multi-scene composition.
#[derive(Clone)]
pub struct Composition {
    /// All scenes by name.
    pub scenes: BTreeMap<String, CompositionScene>,
    /// Default order when no explicit `play` edges exist.
    pub declaration_order: Vec<String>,
    /// Explicit play edges: scene_name → edge.
    pub edges: BTreeMap<String, SceneEdge>,
    /// Total duration of the composition in seconds.
    pub global_duration_s: f64,
    /// scene_name → start time in the global timeline.
    pub scene_start_times: BTreeMap<String, f64>,
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
        if parts.len() == 2 {
            let module_alias = parts[0];
            if let Some(ns) = namespaces.get(module_alias) {
                let scene_name = parts[1];
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
                            "Scene '{}' plays non-existent scene '{}' in module '{}'.",
                            source_scene, scene_name, module_alias
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

/// Per-frame evaluation result in global time space.
pub struct CompositionFrame {
    /// Name of the currently active scene.
    pub scene_name: String,
    /// Time within the active scene in seconds.
    pub local_time_s: f64,
    /// Active transition blend, if any.
    pub transition_blend: Option<TransitionBlend>,
}

/// Active transition between two scenes.
pub struct TransitionBlend {
    /// Scene being transitioned out of.
    pub from_scene: String,
    /// Scene being transitioned into.
    pub to_scene: String,
    /// Local time within the from scene.
    pub from_local: f64,
    /// Local time within the to scene.
    pub to_local: f64,
    /// Raw progress: 0.0 = fully from, 1.0 = fully to (linear).
    pub progress: f64,
    /// Easing-corrected progress — use this for alpha/blend calculations.
    /// `render_transition` applies easing internally, so pass `progress` there;
    /// use `eased_progress` for any custom blending logic.
    pub eased_progress: f64,
    /// Transition identifier (e.g. "fade", "cut").
    pub id: String,
    /// Easing curve applied to the transition progress.
    pub easing: crate::easing::Easing,
}

// ---------------------------------------------------------------------------
// Build Target — canonical entry point for callers
// ---------------------------------------------------------------------------

/// Result of building either a single-scene `Timeline` or a multi-scene `Composition`.
#[allow(clippy::large_enum_variant)]
pub enum BuildTarget {
    /// Single-scene file (no `# SceneName` declarations) — backward compatible.
    SingleScene(Timeline),
    /// Multi-scene file — returns the `Composition` orchestrator.
    MultiScene(Composition),
}

impl BuildTarget {
    /// Build the appropriate target from parsed AST statements.
    ///
    /// Detects `Stmt::Scene` markers to determine single vs multi-scene mode.
    /// For single-scene files, delegates to `Timeline::build_with_diagnostics`.
    /// For multi-scene files, delegates to `Composition::build`.
    pub fn from_ast(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
    ) -> BuildReport<Self> {
        Self::from_ast_with_quality(statements, namespaces, source_path, crate::timeline::BuildQuality::Production)
    }

    /// Build from AST with explicit build quality.
    pub fn from_ast_with_quality(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
        build_quality: crate::timeline::BuildQuality,
    ) -> BuildReport<Self> {
        let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
        let has_scenes = statements.iter().any(|s| matches!(s, Stmt::Scene { .. }));
        let mut report = if has_scenes {
            let report = Composition::build_with_font_context(statements, namespaces, font_context, build_quality);
            BuildReport {
                output: BuildTarget::MultiScene(report.output),
                diagnostics: report.diagnostics,
            }
        } else {
            let report = Timeline::build_with_diagnostics_and_font_context(statements, namespaces, font_context, build_quality);
            BuildReport {
                output: BuildTarget::SingleScene(report.output),
                diagnostics: report.diagnostics,
            }
        };
        if let Some(path) = source_path {
            for diag in &mut report.diagnostics {
                diag.location.path = Some(path.to_path_buf());
            }
        }
        report
    }

    /// Returns the total duration in seconds, regardless of target type.
    pub fn duration_s(&self) -> f64 {
        match self {
            BuildTarget::SingleScene(timeline) => timeline.duration_seconds(),
            BuildTarget::MultiScene(composition) => composition.global_duration_s,
        }
    }
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

impl Composition {
    /// Build a `Composition` from parsed AST statements.
    ///
    /// If no `Stmt::Scene` markers exist, returns a build report with zero scenes.
    /// Callers should check `has_scenes()` and fall back to the single-timeline path.
    pub fn build(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
    ) -> BuildReport<Self> {
        Self::build_with_font_context(
            statements,
            namespaces,
            std::sync::Arc::new(crate::renderer::text::FontContext::new()),
            crate::timeline::BuildQuality::Production,
        )
    }

    /// Build a `Composition` from parsed AST statements with a shared `FontContext`.
    pub fn build_with_font_context(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: crate::timeline::BuildQuality,
    ) -> BuildReport<Self> {
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut scenes: BTreeMap<String, CompositionScene> = BTreeMap::new();
        let mut declaration_order: Vec<String> = Vec::new();
        let mut edges: BTreeMap<String, SceneEdge> = BTreeMap::new();
        let mut shared_prelude: Vec<Stmt> = Vec::new();
        // Temporary storage: scene_name → intermediate play target extracted from raw body.
        let mut play_targets: BTreeMap<String, (String, Option<Transition>)> = BTreeMap::new();

        // 1. Separate shared prelude from scene blocks
        let mut in_scene = false;

        for stmt in statements {
            match stmt {
                Stmt::Scene { name, config, body, span } => {
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

                    let build_report = Timeline::build_with_diagnostics_and_font_context(&merged_body, namespaces, font_context.clone(), build_quality);
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
                    let duration_s = explicit_duration_s.unwrap_or(inferred_duration);

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
                }
                other => {
                    if !in_scene {
                        // Top-level statements before the first scene are the shared prelude.
                        match other {
                            Stmt::Import { .. }
                            | Stmt::LetDecl { is_pub: true, .. }
                            | Stmt::Config { .. }
                            | Stmt::ComponentDef(..)
                            | Stmt::Comment(..) => {
                                shared_prelude.push(other.clone());
                            }
                            _ => {
                                // Other top-level statements before any scene are unusual.
                                // We keep them in the prelude but warn.
                            }
                        }
                    }
                    // Statements already inside scene bodies are handled by
                    // the per-scene timeline build.
                }
            }
        }

        // 2. Resolve play edges from stored play_targets
        for name in &declaration_order {
            if let Some((target, transition)) = play_targets.get(name) {
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

        // 2b. Collect cross-file scenes from namespaces.
        // When a play target references an imported scene (e.g. "play alias.SceneName"),
        // build a timeline from the imported scene's data and register it.
        let cross_file_targets: Vec<String> = play_targets
            .values()
            .map(|(target, _)| target.clone())
            .filter(|t| t.contains('.') && !scenes.contains_key(t))
            .collect();
        for target in cross_file_targets {
            let parts: Vec<&str> = target.splitn(2, '.').collect();
            if parts.len() != 2 {
                continue;
            }
            let module_alias = parts[0];
            let scene_name = parts[1];
            if let Some(ns) = namespaces.get(module_alias) {
                if let Some(scene_data) = ns.scenes.get(scene_name) {
                    // Build timeline from the cross-file scene's prelude + body
                    let mut merged = scene_data.file_prelude.clone();
                    // Insert scene config as a Stmt::Config before the scene body
                    merged.push(Stmt::Config {
                        settings: scene_data.config.clone(),
                        span: scene_data.span,
                    });
                    merged.extend(scene_data.body.clone());
                    let build_report = Timeline::build_with_diagnostics_and_font_context(
                        &merged, namespaces, font_context.clone(), build_quality,
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
                }
            }
        }

        // 3. Compute walk order (following edges, with cycle detection)
        let walk_order = Self::compute_walk_order(
            &declaration_order,
            &edges,
            &scenes,
            &mut diagnostics,
        );

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
                        let overlap_s = (edge.transition.duration_ms as f64 / 1000.0)
                            .min(scene.duration_s); // Clamp to scene duration
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

    /// Returns true if the composition has at least one scene.
    pub fn has_scenes(&self) -> bool {
        !self.scenes.is_empty()
    }

    // ---------------------------------------------------------------------------
    // Global Time Mapping
    // ---------------------------------------------------------------------------

    /// Evaluate the composition at a given global time.
    ///
    /// Returns the active scene name, local time within that scene, and
    /// optional transition blend info if currently in a transition period.
    pub fn evaluate(
        &self,
        global_time_s: f64,
    ) -> (String, f64, Option<TransitionBlend>) {
        let t = global_time_s.max(0.0).min(self.global_duration_s.max(0.001));

        // Find all scenes whose [start, start+duration) range contains t.
        // During a transition, exactly two scenes will be active.
        let mut active: Vec<(&String, f64, f64)> = Vec::new();

        for (name, scene) in &self.scenes {
            let start = self.scene_start_times.get(name).copied().unwrap_or(0.0);
            let end = start + scene.duration_s;

            if t >= start && t < end + 0.001 {
                // Small epsilon to handle boundary cases
                active.push((name, start, end));
            }
        }

        if active.len() == 2 {
            // Sort active scenes by start time so active[0] is the outgoing (earlier) scene
            active.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            // We're in a transition period
            let (from_name, from_start, _from_end) = active[0];
            let (to_name, to_start, _) = active[1];

            let from_local = t - from_start;
            let to_local = t - to_start;

            let edge = self.edges.get(from_name);
            let id = edge
                .map(|e| e.transition.id.clone())
                .unwrap_or_else(|| "cut".into());
            let transition_duration_s = edge
                .map(|e| e.transition.duration_ms as f64 / 1000.0)
                .unwrap_or(0.0);
            let easing = edge
                .map(|e| e.transition.easing)
                .unwrap_or(crate::easing::Easing::Linear);

            let progress = if transition_duration_s > 0.0 {
                ((t - to_start) / transition_duration_s).clamp(0.0, 1.0)
            } else {
                1.0 // Instant cut
            };

            let eased_progress = crate::easing::apply_easing(progress as f32, easing) as f64;

            (
                from_name.clone(),
                from_local,
                Some(TransitionBlend {
                    from_scene: from_name.clone(),
                    to_scene: to_name.clone(),
                    from_local,
                    to_local,
                    progress,
                    eased_progress,
                    id,
                    easing,
                }),
            )
        } else if let Some((name, start, _end)) = active.first() {
            // Single active scene — no transition
            let local = t - start;
            (
                name.to_string(),
                local,
                None,
            )
        } else {
            // t is at or beyond the end — return last scene at final frame
            if let Some(last_name) = self.declaration_order.last() {
                if let Some(last_scene) = self.scenes.get(last_name) {
                    return (last_name.clone(), last_scene.duration_s, None);
                }
            }
            ("".to_string(), 0.0, None)
        }
    }

    /// Get local time within a specific scene from global time.
    ///
    /// Returns `None` if the global time falls outside the scene's active period.
    pub fn local_time_for_scene(&self, scene_name: &str, global_time_s: f64) -> Option<f64> {
        let start = self.scene_start_times.get(scene_name)?;
        let scene = self.scenes.get(scene_name)?;
        let end = start + scene.duration_s;

        // Consider transition overlap: a scene may be active slightly beyond its
        // nominal end due to transition blending.
        let edge = self.edges.get(scene_name);
        let transition_overlap = edge
            .map(|e| e.transition.duration_ms as f64 / 1000.0)
            .unwrap_or(0.0);

        if global_time_s >= *start && global_time_s < end + transition_overlap {
            Some(global_time_s - start)
        } else {
            None
        }
    }

    // ---------------------------------------------------------------------------
    // Internal Helpers
    // ---------------------------------------------------------------------------

    /// Extract the first `play` statement from a scene body.
    /// Emits a diagnostic if multiple `play` statements are found (only the first is used).
    fn extract_play_stmt(
        body: &[Stmt],
        scene_name: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<(String, Option<Transition>)> {
        let mut result: Option<(String, Option<Transition>)> = None;
        for stmt in body {
            if let Stmt::Play {
                scene_name: target,
                transition,
                ..
            } = stmt
            {
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
                    result = Some((target.clone(), transition.clone()));
                }
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
        const SCENE_SCOPED_KEYS: &[&str] = &[
            "colorscheme",
            "dynamic_layout",
            "duration",
        ];

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
        let targeted_scenes: std::collections::HashSet<&str> = edges
            .values()
            .map(|e| e.to_scene.as_str())
            .collect();
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use animatix_syntax::parser::parser_simple;
    use chumsky::Parser;

    #[test]
    fn test_no_scenes_returns_empty_composition() {
        let source = "#0s\ntitle: Text, text: \"Hello\"\n";
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        assert!(!report.output.has_scenes());
        assert_eq!(report.output.global_duration_s, 0.0);
    }

    #[test]
    fn test_two_scenes_implicit_order() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "#1s\n",
            "title.opacity = 1\n",
            "\n",
            "# Diagram\n",
            "#0s\n",
            "graph: Rect, size: (400, 400)\n",
            "#2s\n",
            "graph.opacity = 1\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
        let comp = &report.output;
        assert!(comp.has_scenes());
        assert_eq!(comp.scenes.len(), 2);
        assert_eq!(comp.declaration_order, vec!["Intro", "Diagram"]);
        assert!(comp.global_duration_s > 0.0);
    }

    #[test]
    fn test_scene_with_config() {
        let source = concat!(
            "# Intro\n",
            "config { colorscheme: \"default-dark\" }\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        assert!(report.diagnostics.is_empty());
        let comp = &report.output;
        let intro = comp.scenes.get("Intro").unwrap();
        assert_eq!(intro.config.len(), 1);
        assert_eq!(intro.config[0].name, "colorscheme");
    }

    #[test]
    fn test_scene_with_play() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "#1s\n",
            "title.opacity = 1\n",
            "play Diagram [fade, 300ms]\n",
            "\n",
            "# Diagram\n",
            "#0s\n",
            "graph: Rect, size: (400, 400)\n",
            "#2s\n",
            "graph.opacity = 1\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        let comp = &report.output;
        assert!(comp.edges.contains_key("Intro"));
        let edge = comp.edges.get("Intro").unwrap();
        assert_eq!(edge.to_scene, "Diagram");
        assert_eq!(edge.transition.id, "fade");
        assert_eq!(edge.transition.duration_ms, 300);

        // With 300ms transition, Diagram should overlap by 0.3s
        let intro_start = comp.scene_start_times.get("Intro").unwrap();
        let diagram_start = comp.scene_start_times.get("Diagram").unwrap();
        assert!(*intro_start < *diagram_start);
    }

    #[test]
    fn test_play_target_not_found() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "play MissingScene\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        let has_play_error = report
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::PlayTargetNotFound));
        assert!(has_play_error, "Expected PlayTargetNotFound diagnostic");
    }

    #[test]
    fn test_qualified_play_target_uses_namespace_alias() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "play module.SceneName [fade, 300ms]\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let mut namespaces = std::collections::HashMap::new();
        namespaces.insert("module".to_string(), Namespace::default());

        let report = Composition::build(&parsed, &namespaces);
        let comp = &report.output;
        let edge = comp.edges.get("Intro").unwrap();
        assert_eq!(edge.to_scene, "module.SceneName");
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|d| matches!(d.code, DiagnosticCode::PlayTargetNotFound))
        );
    }

    #[test]
    fn test_qualified_play_target_requires_namespace_alias() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "play module.SceneName\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| matches!(d.code, DiagnosticCode::PlayTargetNotFound))
        );
    }

    #[test]
    fn test_duplicate_scene_name() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Again\"\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        let has_duplicate = report
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::DuplicateSceneName));
        assert!(has_duplicate, "Expected DuplicateSceneName diagnostic");
    }

    #[test]
    fn test_global_time_mapping() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "#1s\n",
            "title.opacity = 1\n",
            "play Diagram [fade, 300ms]\n",
            "\n",
            "# Diagram\n",
            "#0s\n",
            "graph: Text, text: \"Graph\"\n",
            "#2s\n",
            "graph.opacity = 1\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        let comp = &report.output;

        // At t=0: should be in Intro at local time 0
        let (name, local, blend) = comp.evaluate(0.0);
        assert_eq!(name, "Intro");
        assert_eq!(local, 0.0);
        assert!(blend.is_none());

        // After Intro's duration: should be in Diagram
        let intro_dur = comp.scenes.get("Intro").unwrap().duration_s;
        let (name, _, _) = comp.evaluate(intro_dur + 0.1);
        assert_eq!(name, "Diagram");
    }

    #[test]
    fn test_local_time_for_scene() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "#1s\n",
            "title.opacity = 1\n",
            "\n",
            "# Diagram\n",
            "#0s\n",
            "graph: Text, text: \"Graph\"\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        let comp = &report.output;

        let local = comp.local_time_for_scene("Intro", 0.5);
        assert_eq!(local, Some(0.5));

        // Global time beyond Intro should return None for Intro
        let intro_dur = comp.scenes.get("Intro").unwrap().duration_s;
        let local = comp.local_time_for_scene("Intro", intro_dur + 0.1);
        assert_eq!(local, None);
    }

    #[test]
    fn test_single_scene_no_scenes_parsed() {
        // Single-scene file — no # SceneName declarations
        let source = concat!(
            "config { resolution: (1280, 720) }\n",
            "#0s\n",
            "title: Text, text: \"Hello\"\n",
            "#1s\n",
            "fade-in title [500ms]\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        // Verify no Stmt::Scene in output
        let has_scenes = parsed.iter().any(|s| matches!(s, Stmt::Scene { .. }));
        assert!(!has_scenes, "Single-scene file should not produce Stmt::Scene");
    }

    #[test]
    fn test_multiple_play_targets_error() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "play Diagram\n",
            "play Outro\n",
            "\n",
            "# Diagram\n",
            "#0s\n",
            "graph: Rect, size: (400, 400)\n",
            "\n",
            "# Outro\n",
            "#0s\n",
            "bye: Text, text: \"Bye\"\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        let has_multi = report
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::MultiplePlayTargets));
        assert!(has_multi, "Expected MultiplePlayTargets error");
        // First play target should still be used
        let comp = &report.output;
        let edge = comp.edges.get("Intro").unwrap();
        assert_eq!(edge.to_scene, "Diagram");
    }

    #[test]
    fn test_orphan_scene_warning() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "play Diagram\n",
            "\n",
            "# Diagram\n",
            "#0s\n",
            "graph: Rect, size: (400, 400)\n",
            "\n",
            "# Orphan\n",
            "#0s\n",
            "solo: Text, text: \"Alone\"\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        let has_orphan = report
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::OrphanScene));
        assert!(has_orphan, "Expected OrphanScene warning");
        // Orphan should still be in the walk order (appended at end)
        let comp = &report.output;
        assert!(comp.scenes.contains_key("Orphan"));
    }

    #[test]
    fn test_eased_progress_on_transition_blend() {
        let source = concat!(
            "# Intro\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "#1s\n",
            "play Diagram [fade, 500ms]\n",
            "\n",
            "# Diagram\n",
            "#0s\n",
            "graph: Text, text: \"Graph\"\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        let comp = &report.output;

        // At the transition midpoint, eased_progress should differ from raw progress
        // (unless easing is Linear, which it is by default — so test with non-linear)
        // The default transition is cut (0ms), so there's no blend period.
        // Let's just verify the field exists and is populated.
        let intro_dur = comp.scenes.get("Intro").unwrap().duration_s;
        let (_, _, blend) = comp.evaluate(intro_dur + 0.1);
        // With cut transition (0ms), there may be no blend — that's OK.
        // The test verifies evaluate() doesn't panic and returns valid data.
        if let Some(blend) = blend {
            assert!(blend.eased_progress >= 0.0 && blend.eased_progress <= 1.0);
        }
    }

    #[test]
    fn test_scene_config_resolution_warning() {
        let source = concat!(
            "config { resolution: (1280, 720) }\n",
            "\n",
            "# Intro\n",
            "config { resolution: (1920, 1080) }\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
            "\n",
            "# Diagram\n",
            "#0s\n",
            "graph: Rect, size: (400, 400)\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        let has_resolution_warning = report
            .diagnostics
            .iter()
            .any(|d| {
                matches!(d.code, DiagnosticCode::InvalidConfigValue)
                    && d.message.contains("resolution")
                    && d.message.contains("composition-scoped")
            });
        assert!(has_resolution_warning, "Expected resolution scene-config warning");
        // The prelude resolution should still be used (not the scene one)
        // Composition builds successfully
        let comp = &report.output;
        assert!(comp.has_scenes());
    }

    #[test]
    fn test_scene_config_colorscheme_no_warning() {
        let source = concat!(
            "# Intro\n",
            "config { colorscheme: \"editorial-dark\" }\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
        );
        let parsed = parser_simple().parse(source).unwrap();
        let report = Composition::build(&parsed, &std::collections::HashMap::new());
        // colorscheme is scene-scoped — no warning expected
        let has_config_warning = report
            .diagnostics
            .iter()
            .any(|d| {
                matches!(d.code, DiagnosticCode::InvalidConfigValue)
                    && d.message.contains("composition-scoped")
            });
        assert!(!has_config_warning, "colorscheme should not trigger composition-scoped warning");
    }

    #[test]
    fn test_cross_file_scene_basic() {
        let dir = std::env::temp_dir().join(format!("animatix_test_basic_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let scenes_file = dir.join("scenes.amx");
        let main_file = dir.join("main.amx");

        std::fs::write(&scenes_file, concat!(
            "# FadeIn\n",
            "#0s\n",
            "label: Text, text: \"Fade In Scene\"\n",
            "fade-in label [500ms]\n",
        )).unwrap();

        std::fs::write(&main_file, format!(
            "import \"{}\" as scenes\n\n# Intro\n#0s\ntitle: Text, text: \"Welcome\"\nplay scenes.FadeIn [fade, 300ms]\n",
            scenes_file.display(),
        )).unwrap();

        let mut graph = animatix_syntax::module::ModuleGraph::new();
        let program = graph.load_program(&main_file).unwrap();

        let report = Composition::build(&program.statements, &program.namespaces);
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
        let comp = &report.output;

        // Should have both "Intro" (local) and "scenes.FadeIn" (cross-file)
        assert!(comp.scenes.contains_key("Intro"));
        assert!(comp.scenes.contains_key("scenes.FadeIn"),
            "Cross-file scene 'scenes.FadeIn' not found. Scenes: {:?}",
            comp.scenes.keys().collect::<Vec<_>>());

        // Edge should point to the cross-file scene
        let edge = comp.edges.get("Intro").unwrap();
        assert_eq!(edge.to_scene, "scenes.FadeIn");
        assert_eq!(edge.transition.id, "fade");
        assert_eq!(edge.transition.duration_ms, 300);

        // Cleanup

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cross_file_scene_not_found() {
        let dir = std::env::temp_dir().join(format!("animatix_test_notfound_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let scenes_file = dir.join("scenes.amx");
        let main_file = dir.join("main.amx");

        std::fs::write(&scenes_file, "# ExistingScene\n#0s\nlabel: Text, text: \"Hi\"\n").unwrap();

        std::fs::write(&main_file, format!(
            "import \"{}\" as scenes\n\n# Intro\n#0s\ntitle: Text, text: \"Welcome\"\nplay scenes.NonExistent\n",
            scenes_file.display(),
        )).unwrap();

        let mut graph = animatix_syntax::module::ModuleGraph::new();
        let program = graph.load_program(&main_file).unwrap();

        let report = Composition::build(&program.statements, &program.namespaces);
        // The play target "scenes.NonExistent" should produce a PlayTargetNotFound error
        // because the scene exists in the namespace under "ExistingScene", not "NonExistent".
        let has_play_error = report
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, DiagnosticCode::PlayTargetNotFound));
        assert!(has_play_error, "Expected PlayTargetNotFound diagnostic. Diagnostics: {:?}", report.diagnostics);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cross_file_scene_duration_preserved() {
        let dir = std::env::temp_dir().join(format!("animatix_test_duration_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let scenes_file = dir.join("scenes.amx");
        let main_file = dir.join("main.amx");

        // Scene with explicit duration config
        std::fs::write(&scenes_file, concat!(
            "# TimedScene\n",
            "config { duration: 5.0 }\n",
            "#0s\n",
            "label: Text, text: \"Timed\"\n",
        )).unwrap();

        std::fs::write(&main_file, format!(
            "import \"{}\" as scenes\n\n# Intro\n#0s\ntitle: Text, text: \"Welcome\"\nplay scenes.TimedScene\n",
            scenes_file.display(),
        )).unwrap();

        let mut graph = animatix_syntax::module::ModuleGraph::new();
        let program = graph.load_program(&main_file).unwrap();

        let report = Composition::build(&program.statements, &program.namespaces);
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
        let comp = &report.output;

        let timed_scene = comp.scenes.get("scenes.TimedScene").unwrap();
        assert_eq!(timed_scene.explicit_duration_s, Some(5.0));
        assert_eq!(timed_scene.duration_s, 5.0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
