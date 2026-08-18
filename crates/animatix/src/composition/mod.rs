//! Multi-Scene Composition Engine
//!
//! `Composition` is the orchestration layer for multi-scene `.amx` files.
//! It manages per-scene `Timeline` instances, scene ordering via `play` edges,
//! global time mapping, and transition blending.
//!
//! Single-scene files (no `# SceneName` declarations) use the existing
//! `Timeline::build_with_diagnostics` path — this module is only activated
//! when `Stmt::Scene` markers are present in the parsed AST.

mod build;
mod time;

use std::collections::BTreeMap;

use crate::ast::{Property, Span, Stmt, Transition};
use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::module::Namespace;
use crate::timeline::Timeline;

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
        Self::from_ast_with_quality(
            statements,
            namespaces,
            source_path,
            crate::timeline::BuildQuality::Production,
        )
    }

    /// Build from AST with explicit build quality.
    pub fn from_ast_with_quality(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
        build_quality: crate::timeline::BuildQuality,
    ) -> BuildReport<Self> {
        Self::from_ast_with_quality_and_asset_cache(
            statements,
            namespaces,
            source_path,
            build_quality,
            None,
        )
    }

    /// Build from AST with an extension context.
    pub fn from_ast_with_context(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
        context: std::sync::Arc<crate::extension_context::ExtensionContext>,
    ) -> BuildReport<Self> {
        let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
        let has_scenes = statements.iter().any(|s| matches!(s, Stmt::Scene { .. }));
        let mut report = if has_scenes {
            let report = Composition::build_with_font_context_and_asset_cache_and_extension_context(
                statements,
                namespaces,
                font_context,
                crate::timeline::BuildQuality::Production,
                None,
                context.clone(),
            );
            BuildReport {
                output: BuildTarget::MultiScene(report.output),
                diagnostics: report.diagnostics,
            }
        } else {
            let report =
                Timeline::build_with_diagnostics_and_font_context_and_asset_cache_and_extension_context(
                    statements,
                    namespaces,
                    font_context,
                    crate::timeline::BuildQuality::Production,
                    None,
                    context,
                );
            let mut diags = report.diagnostics;
            for (label, &flag) in &report.output.persistence_flags {
                if flag {
                    diags.push(
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
                diagnostics: diags,
            }
        };
        if let Some(path) = source_path {
            for diag in &mut report.diagnostics {
                diag.location.path = Some(path.to_path_buf());
            }
        }
        report
    }

    /// Build from AST with explicit build quality and an existing asset cache.
    pub fn from_ast_with_quality_and_asset_cache(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
        build_quality: crate::timeline::BuildQuality,
        asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
    ) -> BuildReport<Self> {
        let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
        let has_scenes = statements.iter().any(|s| matches!(s, Stmt::Scene { .. }));
        let mut report = if has_scenes {
            let report = Composition::build_with_font_context_and_asset_cache(
                statements,
                namespaces,
                font_context,
                build_quality,
                asset_cache,
            );
            BuildReport {
                output: BuildTarget::MultiScene(report.output),
                diagnostics: report.diagnostics,
            }
        } else {
            let report = Timeline::build_with_diagnostics_and_font_context_and_asset_cache(
                statements,
                namespaces,
                font_context,
                build_quality,
                asset_cache,
            );
            // Warn about persistent actors in a truly single-scene file (no successor).
            let mut diags = report.diagnostics;
            for (label, &flag) in &report.output.persistence_flags {
                if flag {
                    diags.push(
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
                diagnostics: diags,
            }
        };
        if let Some(path) = source_path {
            for diag in &mut report.diagnostics {
                diag.location.path = Some(path.to_path_buf());
            }
        }
        report
    }

    /// Build from AST with explicit quality, asset cache, and extension context.
    #[allow(clippy::too_many_arguments)]
    pub fn from_ast_with_quality_and_asset_cache_and_extension_context(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
        build_quality: crate::timeline::BuildQuality,
        asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
        context: std::sync::Arc<crate::extension_context::ExtensionContext>,
    ) -> BuildReport<Self> {
        let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
        let has_scenes = statements.iter().any(|s| matches!(s, Stmt::Scene { .. }));
        let mut report = if has_scenes {
            let report = Composition::build_with_font_context_and_asset_cache_and_extension_context(
                statements,
                namespaces,
                font_context,
                build_quality,
                asset_cache,
                context,
            );
            BuildReport {
                output: BuildTarget::MultiScene(report.output),
                diagnostics: report.diagnostics,
            }
        } else {
            let report =
                Timeline::build_with_diagnostics_and_font_context_and_asset_cache_and_extension_context(
                    statements,
                    namespaces,
                    font_context,
                    build_quality,
                    asset_cache,
                    context,
                );
            let mut diags = report.diagnostics;
            for (label, &flag) in &report.output.persistence_flags {
                if flag {
                    diags.push(
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
                diagnostics: diags,
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

    /// Build a composition from AST statements with a shared `FontContext` and
    /// an existing asset cache carried from a previous build.
    pub fn build_with_font_context_and_asset_cache(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: crate::timeline::BuildQuality,
        asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
    ) -> BuildReport<Self> {
        Self::build_with_font_context_impl(
            statements,
            namespaces,
            font_context,
            build_quality,
            asset_cache,
        )
    }

    /// Build a composition with an extension context.
    pub fn build_with_font_context_and_asset_cache_and_extension_context(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: crate::timeline::BuildQuality,
        asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
        context: std::sync::Arc<crate::extension_context::ExtensionContext>,
    ) -> BuildReport<Self> {
        Self::build_with_font_context_impl_with_context(
            statements,
            namespaces,
            font_context,
            build_quality,
            asset_cache,
            Some(context),
        )
    }

    /// Returns true if the composition has at least one scene.
    pub fn has_scenes(&self) -> bool {
        !self.scenes.is_empty()
    }
}

#[cfg(test)]
mod tests;
