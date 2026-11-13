//! Parse → typecheck → expand → build the document the player installs.

use std::path::PathBuf;
use std::sync::Arc;

use animatix::composition::BuildTarget;
use animatix::renderer::text::FontContext;
use animatix::timeline::assets::AssetCache;
use animatix::timeline::{BuildOptions, BuildQuality, SceneDimensions};

use crate::dto::{DiagnosticDto, LoadResultDto};

use super::{MIN_DURATION_S, markers::timeline_markers};

/// A parsed-and-built document plus everything the JS shell needs to update
/// the UI. `target` is `None` when the current source cannot be built; the
/// caller keeps rendering the previous document (last-known-good behaviour).
pub struct BuiltDocument {
    pub target: Option<BuildTarget>,
    pub result: LoadResultDto,
}

/// Parse, typecheck, expand, and build `source`. Never fails hard: failures
/// come back as diagnostics with `ok == false` and no target.
pub fn build_document(
    source: &str,
    font_context: Arc<FontContext>,
    quality: BuildQuality,
) -> BuiltDocument {
    build_document_with_assets(source, font_context, quality, None)
}

/// `assets` pre-seeds the scene's asset cache (the web player fetches the
/// bytes), so build-time asset loads resolve from memory instead of the
/// filesystem.
pub fn build_document_with_assets(
    source: &str,
    font_context: Arc<FontContext>,
    quality: BuildQuality,
    assets: Option<Arc<AssetCache>>,
) -> BuiltDocument {
    build_document_with_modules(source, &[], font_context, quality, assets)
}

/// Parse, typecheck, expand, and build `source` with `modules` pre-registered
/// in the source map, and no build-time variable overrides.
///
/// `modules` are the `.amx` files the shell fetched for this scene beyond the
/// bundled library, keyed by the *resolved* import path (what the module
/// system would join + normalize — the shell learns the keys from
/// `LoadResultDto::missing_imports`). When the load stops on a file that was
/// never supplied, the result reports that key in `missing_imports` instead of
/// surfacing only a generic not-found error: the shell fetches it and retries.
pub fn build_document_with_modules(
    source: &str,
    modules: &[(PathBuf, String)],
    font_context: Arc<FontContext>,
    quality: BuildQuality,
    assets: Option<Arc<AssetCache>>,
) -> BuiltDocument {
    build_document_with_vars(source, modules, font_context, quality, assets, &[])
}

/// [`build_document_with_modules`] plus `vars`, the web shape of the CLI's
/// repeatable `--set NAME=VALUE`: `(name, expression text)` pairs seeded into
/// the build environment, where each shadows the scene's own `let NAME = …`
/// default. An empty slice reproduces the no-override build exactly.
///
/// Rendering a page's JSON values into that expression text is
/// [`super::vars`]'s job; parsing and evaluating it stays the engine's, so a
/// bad name or value comes back as a build diagnostic rather than a second
/// parser's guess.
pub fn build_document_with_vars(
    source: &str,
    modules: &[(PathBuf, String)],
    font_context: Arc<FontContext>,
    quality: BuildQuality,
    assets: Option<Arc<AssetCache>>,
    vars: &[(String, String)],
) -> BuiltDocument {
    let prepared = super::prepare_document(source, modules, vars, None);
    build_prepared(&prepared, font_context, quality, assets)
}

/// Build one prepared immutable program without reparsing or re-expanding it.
pub fn build_prepared(
    prepared: &super::PreparedProgram,
    font_context: Arc<FontContext>,
    quality: BuildQuality,
    assets: Option<Arc<AssetCache>>,
) -> BuiltDocument {
    let analysis = &prepared.analysis;
    if !analysis.ok {
        return BuiltDocument {
            target: None,
            result: LoadResultDto {
                ok: false,
                duration_s: MIN_DURATION_S,
                width: SceneDimensions::default().width,
                height: SceneDimensions::default().height,
                required_features: analysis.required_features,
                diagnostics: analysis.diagnostics.clone(),
                missing_imports: analysis.missing_imports.clone(),
                markers: Vec::new(),
                pause_points: Vec::new(),
            },
        };
    }
    let mut diagnostics = analysis.diagnostics.clone();
    let report = BuildTarget::build_with_options(
        &prepared.syntax.expanded,
        &prepared.syntax.namespaces,
        None,
        BuildOptions {
            font_context: Some(font_context),
            quality: Some(quality),
            asset_cache: assets,
            extensions: Some(Arc::new(super::resources::extensions())),
            skip_persist_target_warning: true,
            defines: &prepared.vars,
            ..Default::default()
        },
    );
    let target = report.output;
    diagnostics.extend(report.diagnostics.iter().map(DiagnosticDto::from_diagnostic));
    if !prepared.vars.is_empty() && matches!(target, BuildTarget::MultiScene(_)) {
        diagnostics.push(DiagnosticDto {
            severity: "warning".to_string(), code: "vars-not-applied".to_string(),
            message: "Build-time variable overrides reach a single-scene document only; this composition keeps each scene's own defaults.".to_string(),
            subject: None, line: None, column: None, span: None,
        });
    }
    let (duration_s, width, height) = document_extent(&target);
    let markers = timeline_markers(&prepared.syntax.expanded, &target);
    let pause_points = target.pause_points().iter().map(|&ms| ms as f64 / 1000.0).collect();
    let ok = !diagnostics.iter().any(|d| d.severity == "error");
    BuiltDocument {
        target: ok.then_some(target),
        result: LoadResultDto {
            ok,
            duration_s,
            width,
            height,
            required_features: analysis.required_features,
            diagnostics,
            missing_imports: Vec::new(),
            markers,
            pause_points,
        },
    }
}

/// Duration + dimensions of a built target, with the same floors as the GUI.
fn document_extent(target: &BuildTarget) -> (f64, u32, u32) {
    let default_dims = SceneDimensions::default();
    let default_dims = (default_dims.width, default_dims.height);
    match target {
        BuildTarget::SingleScene(timeline) => {
            let (w, h) = timeline.resolution().unwrap_or(default_dims);
            // Playback length, not the inferred keyframe extent: a declared
            // `config { duration: N }` overrides it, so the player stops there.
            (timeline.playback_duration_seconds().max(MIN_DURATION_S), w, h)
        },
        BuildTarget::MultiScene(composition) => {
            let (w, h) = composition
                .scenes
                .values()
                .next()
                .and_then(|scene| scene.timeline.resolution())
                .unwrap_or(default_dims);
            (composition.global_duration_s.max(MIN_DURATION_S), w, h)
        },
    }
}
