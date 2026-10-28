//! Multi-Scene Composition Engine
//!
//! `Composition` is the orchestration layer for multi-scene `.amx` files.
//! It manages per-scene `Timeline` instances, scene ordering via `play` edges,
//! global time mapping, and transition blending.
//!
//! Single-scene files (no `# SceneName` declarations) build through the plain
//! [`Timeline::build_with_options`] path — this module is only activated when
//! `Stmt::Scene` markers are present in the parsed AST.
//!
//! [`BuildTarget::build_with_options`] is the canonical entry point: it detects
//! the mode and returns either a [`Timeline`] or a [`Composition`].

mod build;
mod time;

use std::collections::BTreeMap;

use crate::ast::{Property, Span, Stmt, Transition};
use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::module::Namespace;
use crate::timeline::{BuildOptions, Timeline};

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
    /// Interactive pause points across the entire composition in global milliseconds.
    pub pause_points: Vec<u64>,
}

/// Read-only summary of a composition's scene graph — the data a human (or
/// tooling) needs to answer "what plays when, and how do scenes connect".
/// Powers the `animatix timeline` command and composition unit tests.
#[derive(Clone, Debug)]
pub struct CompositionSummary {
    /// Scenes in walk order: (name, start_time_s, duration_s, explicit_duration_s).
    pub scenes: Vec<(String, f64, f64, Option<f64>)>,
    /// Play edges in walk order: (from_scene, to_scene, transition_id, duration_ms).
    pub edges: Vec<(String, String, String, u64)>,
    /// Total duration of the composition in seconds.
    pub total_duration_s: f64,
}

impl Composition {
    /// Build a [`CompositionSummary`] from this composition's scene graph.
    pub fn summary(&self) -> CompositionSummary {
        // Playback order = scenes sorted by their global start time.
        let mut by_start: Vec<(&String, f64)> =
            self.scene_start_times.iter().map(|(name, t)| (name, *t)).collect();
        by_start.sort_by(|a, b| a.1.total_cmp(&b.1));
        let scenes = by_start
            .iter()
            .filter_map(|(name, start)| {
                let scene = self.scenes.get(*name)?;
                Some(((*name).clone(), *start, scene.duration_s, scene.explicit_duration_s))
            })
            .collect();
        let edges = by_start
            .iter()
            .filter_map(|(name, _)| {
                let edge = self.edges.get(*name)?;
                Some((
                    (*name).clone(),
                    edge.to_scene.clone(),
                    edge.transition.id.clone(),
                    edge.transition.duration_ms,
                ))
            })
            .collect();
        CompositionSummary {
            scenes,
            edges,
            total_duration_s: self.global_duration_s,
        }
    }

    /// Interactive pause points across the entire composition in global milliseconds.
    pub fn pause_points(&self) -> &[u64] {
        &self.pause_points
    }

    /// The next pause point strictly after `current_time_ms`.
    pub fn step_next(&self, current_time_ms: u64) -> Option<u64> {
        self.pause_points.iter().copied().find(|&t| t > current_time_ms)
    }

    /// The previous pause point strictly before `current_time_ms`.
    pub fn step_prev(&self, current_time_ms: u64) -> Option<u64> {
        self.pause_points.iter().copied().rfind(|&t| t < current_time_ms)
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
    /// The mutation epoch of the whole document, for a driver keying rendered
    /// frames by content (`timeline::frame_signature`).
    ///
    /// Summed across scenes rather than maxed: epochs only ever increase, so a
    /// sum strictly increases when any scene is edited, while a max would keep
    /// reporting the same number after an edit to a scene whose epoch is not the
    /// largest — a multi-scene document whose dedup never noticed the change.
    pub fn content_epoch(&self) -> u64 {
        match self {
            Self::SingleScene(timeline) => timeline.content_epoch(),
            Self::MultiScene(composition) => {
                composition.scenes.values().map(|scene| scene.timeline.content_epoch()).sum()
            },
        }
    }

    /// Build the appropriate target from parsed AST statements with explicit
    /// options.
    ///
    /// Detects `Stmt::Scene` markers to choose single- or multi-scene mode.
    /// This is the one build implementation: every other entry point either
    /// builds [`BuildOptions`] for it or reports a composition-only result.
    ///
    /// An asset cache is created from `source_path`'s directory when the caller
    /// did not supply one, and every scene inherits it through `options`.
    /// Single-scene programs get the `PersistTargetNotCarried` warning because a
    /// lone timeline has no successor to carry into.
    pub fn build_with_options(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
        options: BuildOptions<'_>,
    ) -> BuildReport<Self> {
        let asset_cache = options.asset_cache.clone().or_else(|| {
            source_path.and_then(|path| path.parent()).map(|dir| {
                let mut cache = crate::timeline::assets::AssetCache::new();
                cache.set_base_dirs(Some(dir), None);
                std::sync::Arc::new(cache)
            })
        });
        let options = BuildOptions {
            asset_cache,
            ..options
        };

        let has_scenes = statements.iter().any(|s| matches!(s, Stmt::Scene { .. }));
        let mut report = if has_scenes {
            if !options.defines.is_empty() {
                tracing::warn!(
                    "--set overrides are not applied to a multi-scene program; the composition builds each scene from its own source"
                );
            }
            let report = Composition::build_with_options(statements, namespaces, options);
            BuildReport {
                output: BuildTarget::MultiScene(report.output),
                diagnostics: report.diagnostics,
            }
        } else {
            let report = Timeline::build_with_options(statements, namespaces, options);
            let mut diagnostics = report.diagnostics;
            // Warn about persistent actors in a truly single-scene file (no successor).
            for (label, flag) in report.output.persistence_flags_iter() {
                if flag {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::PersistTargetNotCarried,
                            DiagnosticPhase::Build,
                            format!(
                                "Actor '{}' is persisted but there is no successor scene to carry into.",
                                label,
                            ),
                        )
                        .with_subject(label),
                    );
                }
            }
            BuildReport {
                output: BuildTarget::SingleScene(report.output),
                diagnostics,
            }
        };
        if let Some(path) = source_path {
            for diag in &mut report.diagnostics {
                diag.location.path = Some(path.to_path_buf());
            }
            report.output.set_asset_source_base(path);
        }
        report
    }

    /// Build from AST with an extension context and `--set NAME=VALUE`
    /// overrides applied to the build environment.
    ///
    /// Convenience for the CLI, whose documents are always built under an
    /// extension context and may carry `--set` overrides. Overrides reach a
    /// single-scene program; a multi-scene composition reports that they do not
    /// reach it yet rather than applying them to some scenes only.
    pub fn from_ast_with_context_and_defines(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
        context: std::sync::Arc<crate::extension_context::ExtensionContext>,
        defines: &[(String, String)],
    ) -> BuildReport<Self> {
        // Compatibility: a single-scene `--set` build used to bypass the
        // extension context, because the old defines-only entry point had no
        // place to pass one. Preserved across the entry-point collapse; the
        // follow-up `fix(cli)` applies the whole option set instead.
        let has_scenes = statements.iter().any(|s| matches!(s, Stmt::Scene { .. }));
        let options = if !has_scenes && !defines.is_empty() {
            BuildOptions::default().with_defines(defines)
        } else {
            BuildOptions::default().with_context(context).with_defines(defines)
        };
        Self::build_with_options(statements, namespaces, source_path, options)
    }

    /// Returns the total duration in seconds, regardless of target type.
    ///
    /// This is the length the document *plays* for, so a single scene honours
    /// its declared `config { duration: N }` through
    /// [`Timeline::playback_duration_seconds`] — the GUI preview range and every
    /// export frame count come from here.
    pub fn duration_s(&self) -> f64 {
        match self {
            BuildTarget::SingleScene(timeline) => timeline.playback_duration_seconds(),
            BuildTarget::MultiScene(composition) => composition.global_duration_s,
        }
    }

    /// Returns all interactive pause points in global milliseconds.
    pub fn pause_points(&self) -> &[u64] {
        match self {
            BuildTarget::SingleScene(timeline) => timeline.pause_points(),
            BuildTarget::MultiScene(composition) => composition.pause_points(),
        }
    }

    /// The next pause point strictly after `current_time_ms`.
    pub fn step_next(&self, current_time_ms: u64) -> Option<u64> {
        match self {
            BuildTarget::SingleScene(timeline) => timeline.step_next(current_time_ms),
            BuildTarget::MultiScene(composition) => composition.step_next(current_time_ms),
        }
    }

    /// The previous pause point strictly before `current_time_ms`.
    pub fn step_prev(&self, current_time_ms: u64) -> Option<u64> {
        match self {
            BuildTarget::SingleScene(timeline) => timeline.step_prev(current_time_ms),
            BuildTarget::MultiScene(composition) => composition.step_prev(current_time_ms),
        }
    }

    fn set_asset_source_base(&mut self, source_path: &std::path::Path) {
        let document_dir = source_path.parent();
        match self {
            BuildTarget::SingleScene(timeline) => {
                std::sync::Arc::make_mut(&mut timeline.asset_cache)
                    .set_base_dirs(document_dir, None);
            },
            BuildTarget::MultiScene(composition) => {
                for scene in composition.scenes.values_mut() {
                    std::sync::Arc::make_mut(&mut scene.timeline.asset_cache)
                        .set_base_dirs(document_dir, None);
                }
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

impl Composition {
    /// Returns true if the composition has at least one scene.
    pub fn has_scenes(&self) -> bool {
        !self.scenes.is_empty()
    }

    /// Configure document/workspace base directories for relative asset URLs.
    pub fn set_asset_base_dirs(
        &mut self,
        document_dir: Option<&std::path::Path>,
        workspace_root: Option<&std::path::Path>,
    ) {
        for scene in self.scenes.values_mut() {
            scene.timeline.set_asset_base_dirs(document_dir, workspace_root);
        }
    }
}

/// Test-only shortcuts that keep the composition tests on the canonical build
/// path without repeating the option set at every call site.
#[cfg(test)]
impl Composition {
    fn build_test(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
    ) -> BuildReport<Self> {
        Self::build_with_options(statements, namespaces, BuildOptions::default())
    }
}

#[cfg(test)]
impl BuildTarget {
    fn build_test(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
    ) -> BuildReport<Self> {
        Self::build_with_options(statements, namespaces, None, BuildOptions::default())
    }
}

#[cfg(test)]
mod tests;
