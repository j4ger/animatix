//! Composition assembly: scene collection and the build entry point.
//!
//! The build is split into named phases over one [`SceneCollection`]:
//! [`collect_scenes`] and [`collect_cross_file_scenes`] gather the scene ASTs
//! and their first-pass timelines, [`super::edges`] resolves `play` edges, and
//! [`super::timing`] turns the graph into start times and pause points.

use super::edges::resolve_namespace;
use super::*;

/// Mutable state accumulated while a composition is assembled.
pub(super) struct SceneCollection {
    pub diagnostics: Vec<Diagnostic>,
    pub scenes: BTreeMap<String, CompositionScene>,
    pub declaration_order: Vec<String>,
    pub edges: BTreeMap<String, SceneEdge>,
    pub shared_prelude: Vec<Stmt>,
    /// `scene_name → (target, transition, play_time)` extracted from raw bodies.
    pub play_targets: BTreeMap<String, (String, Option<Transition>, Option<f64>)>,
    /// Merged AST bodies, cached for the carry-aware rebuild.
    pub merged_bodies: BTreeMap<String, Vec<Stmt>>,
    /// Playback order computed from the edges (declaration order when there
    /// are none).
    pub walk_order: Vec<String>,
}

impl SceneCollection {
    fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            scenes: BTreeMap::new(),
            declaration_order: Vec::new(),
            edges: BTreeMap::new(),
            shared_prelude: Vec::new(),
            play_targets: BTreeMap::new(),
            merged_bodies: BTreeMap::new(),
            walk_order: Vec::new(),
        }
    }
}

impl Composition {
    /// Build a `Composition` from parsed AST statements with explicit options.
    ///
    /// Every scene shares one font context (materialized here if the caller did
    /// not supply one). `--set` overrides are dropped for scenes: a multi-scene
    /// program reports that they do not reach it rather than applying them to
    /// some scenes only — [`BuildTarget::build_with_options`] emits the warning.
    pub(crate) fn build_with_options(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        options: BuildOptions<'_>,
    ) -> BuildReport<Self> {
        let options =
            BuildOptions {
                font_context: Some(options.font_context.clone().unwrap_or_else(|| {
                    std::sync::Arc::new(crate::renderer::text::FontContext::new())
                })),
                ..options
            };
        let scene_options = BuildOptions {
            defines: &[],
            ..options
        };

        let mut state = SceneCollection::new();
        collect_scenes(statements, namespaces, &scene_options, &mut state);
        collect_cross_file_scenes(namespaces, &scene_options, &mut state);
        edges::resolve_edges(namespaces, &mut state);
        timing::resolve_timing(namespaces, &scene_options, &mut state);

        let SceneCollection {
            scenes,
            walk_order,
            declaration_order,
            edges,
            diagnostics,
            ..
        } = state;
        let composition =
            timing::finalize_global_timeline(scenes, &walk_order, &declaration_order, &edges);

        BuildReport::new(composition, diagnostics)
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
        // Plays nested inside keyframe bodies never reach the composition's
        // scene graph — warn so authors move them to the scene top level.
        for stmt in body {
            if let Stmt::Keyframe { body: kf_body, .. }
            | Stmt::RelativeKeyframe { body: kf_body, .. } = stmt
            {
                for inner in kf_body {
                    if matches!(inner, Stmt::Play { .. }) {
                        diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::PlayInsideKeyframe,
                                DiagnosticPhase::Build,
                                format!(
                                    "Scene '{scene_name}' has a `play` inside a keyframe body; \
                                     it is ignored. Move the `play` to the scene's top level."
                                ),
                            )
                            .with_subject(scene_name),
                        );
                    }
                }
            }
        }
        for stmt in body {
            match stmt {
                Stmt::Keyframe { time, .. } | Stmt::RelativeKeyframe { offset: time, .. } => {
                    current_time = match time {
                        crate::ast::Time::Seconds(s) => *s,
                        crate::ast::Time::Milliseconds(ms) => *ms as f64 / 1000.0,
                        crate::ast::Time::Beats(b) => {
                            b * crate::ast::beat_seconds(crate::ast::DEFAULT_BPM)
                        },
                        crate::ast::Time::SpeechMarker { offset_ms, .. }
                        | crate::ast::Time::SpeechWord { offset_ms, .. } => {
                            *offset_ms as f64 / 1000.0
                        },
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
            if prop.name == "duration"
                && let Expr::Num(val) = prop.value
            {
                return Some(val);
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
        let scene_scoped_keys = animatix_syntax::config_keys::scene_scoped_config_key_names();

        for prop in config {
            if scene_scoped_keys.contains(&prop.name.as_str()) {
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
}

/// Separate the shared prelude from the entry file's scene blocks and build
/// each scene's first-pass timeline.
///
/// The first pass is not throwaway: it produces the diagnostics the user sees
/// and the duration/carry source for scenes that never get rebuilt. A scene
/// that carries state is rebuilt in [`super::timing::resolve_timing`], which
/// discards this pass's diagnostics for that scene and replaces them.
fn collect_scenes(
    statements: &[Stmt],
    namespaces: &std::collections::HashMap<String, Namespace>,
    options: &BuildOptions<'_>,
    state: &mut SceneCollection,
) {
    let mut in_scene = false;

    for stmt in statements {
        match stmt {
            Stmt::Scene {
                name,
                config,
                body,
                span,
            } => {
                if state.scenes.contains_key(name) {
                    state.diagnostics.push(
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
                let play_target =
                    Composition::extract_play_stmt(body, name, &mut state.diagnostics);

                // Warn on scene-level config keys that are composition-scoped
                // and cannot meaningfully override the prelude.
                Composition::validate_scene_config(config, name, &mut state.diagnostics);

                // Build the per-scene timeline: merge prelude + scene body
                let mut merged_body = state.shared_prelude.clone();
                // Insert scene config as a Stmt::Config before the scene body
                merged_body.push(Stmt::Config {
                    settings: config.clone(),
                    span: None,
                });
                merged_body.extend(body.clone());
                state.merged_bodies.insert(name.clone(), merged_body.clone());

                let build_report =
                    Timeline::build_with_options(&merged_body, namespaces, options.clone());
                state.diagnostics.extend(
                    build_report
                        .diagnostics
                        .into_iter()
                        .map(|d| d.with_subject(format!("scene '{}'", name))),
                );
                let timeline = build_report.output;
                let inferred_duration = timeline.duration_seconds();

                // Check for explicit duration in scene config
                let explicit_duration_s = Composition::extract_duration_from_config(config);
                let play_time_s = play_target.as_ref().and_then(|(_, _, t)| *t);
                let duration_s = explicit_duration_s.unwrap_or_else(|| {
                    // The scene must run long enough for its `play` to
                    // fire; keyframe-only inference cuts it short.
                    play_time_s.map(|t| inferred_duration.max(t)).unwrap_or(inferred_duration)
                });
                if let Some(target) = play_target {
                    state.play_targets.insert(name.clone(), target);
                }

                state.scenes.insert(
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

                state.declaration_order.push(name.clone());
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
                            state.shared_prelude.push(other.clone());
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
}

/// Collect scenes from imported namespaces, transitively.
///
/// When a play target references an imported scene (e.g. `play alias.SceneName`),
/// build a timeline from the imported scene's data and register it — then
/// process the newly registered scene's own play target so multi-hop chains
/// (`Hub -> logo.Logo -> slogan.Slogan -> ...`) resolve fully.
fn collect_cross_file_scenes(
    namespaces: &std::collections::HashMap<String, Namespace>,
    options: &BuildOptions<'_>,
    state: &mut SceneCollection,
) {
    let mut cross_queue: Vec<String> = state
        .play_targets
        .values()
        .map(|(target, _, _)| target.clone())
        .filter(|t| t.contains('.') && !state.scenes.contains_key(t))
        .collect();
    let mut queued: std::collections::HashSet<String> = cross_queue.iter().cloned().collect();
    while let Some(target) = cross_queue.pop() {
        let parts: Vec<&str> = target.split('.').collect();
        if parts.len() < 2 {
            continue;
        }
        let (namespace_parts, scene_name) = parts.split_at(parts.len() - 1);
        let scene_name = scene_name[0];
        let Some(ns) = resolve_namespace(namespaces, namespace_parts) else {
            state.diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::PlayTargetNotFound,
                    DiagnosticPhase::Build,
                    format!(
                        "Play target '{target}' references namespace '{}' which was not found; \
                         the scene is skipped.",
                        namespace_parts.join(".")
                    ),
                )
                .with_subject(&target),
            );
            continue;
        };
        // Legacy/test namespaces may carry no scenes at all — accept any
        // name there (same rule as validate_play_target).
        if !ns.scenes.is_empty() && !ns.scenes.contains_key(scene_name) {
            state.diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::PlayTargetNotFound,
                    DiagnosticPhase::Build,
                    format!(
                        "Play target '{target}' references scene '{scene_name}' which the \
                         imported module does not declare; the scene is skipped."
                    ),
                )
                .with_subject(&target),
            );
            continue;
        }
        let Some(scene_data) = ns.scenes.get(scene_name) else {
            continue;
        };
        {
            // Build timeline from the cross-file scene's prelude + body.
            // The entry's shared prelude is prepended too so imported
            // theme modules (colorscheme declarations) register for
            // cross-file scenes, matching entry-declared scenes.
            let mut merged = state.shared_prelude.clone();
            merged.extend(scene_data.file_prelude.clone());
            // Insert scene config as a Stmt::Config before the scene body
            merged.push(Stmt::Config {
                settings: scene_data.config.clone(),
                span: scene_data.span,
            });
            merged.extend(scene_data.body.clone());
            state.merged_bodies.insert(target.clone(), merged.clone());
            let build_report = Timeline::build_with_options(&merged, namespaces, options.clone());
            state.diagnostics.extend(
                build_report
                    .diagnostics
                    .into_iter()
                    .map(|d| d.with_subject(format!("scene '{}'", target))),
            );
            let timeline = build_report.output;
            // Scene files declare their config at file level (it lands in
            // the prelude), so also look there for an explicit duration.
            let explicit_duration_s = Composition::extract_duration_from_config(&scene_data.config)
                .or_else(|| {
                    scene_data.file_prelude.iter().find_map(|s| match s {
                        Stmt::Config { settings, .. } => {
                            Composition::extract_duration_from_config(settings)
                        },
                        _ => None,
                    })
                });
            let duration_s = explicit_duration_s.unwrap_or_else(|| timeline.duration_seconds());
            state.scenes.insert(
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
            state.declaration_order.push(target.clone());

            // Chain: queue the newly registered scene's own play target.
            if let Some((next_target, next_transition, next_play_time)) =
                Composition::extract_play_stmt(&scene_data.body, &target, &mut state.diagnostics)
            {
                state
                    .play_targets
                    .insert(target.clone(), (next_target.clone(), next_transition, next_play_time));
                if next_target.contains('.')
                    && !state.scenes.contains_key(&next_target)
                    && !queued.contains(&next_target)
                {
                    queued.insert(next_target.clone());
                    cross_queue.push(next_target);
                }
            }
        }
    }
}
