//! Parse → typecheck → expand → build the document the player installs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use animatix::composition::{BuildTarget, Composition};
use animatix::extension_context::ExtensionContext;
use animatix::renderer::text::FontContext;
use animatix::timeline::assets::AssetCache;
use animatix::timeline::{BuildQuality, SceneDimensions, Timeline};
use animatix_syntax::ast::Stmt;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use animatix_syntax::module::{ModuleError, ModuleGraph, Namespace, SourceAccess};

use crate::dto::{DiagnosticDto, LoadResultDto};

use super::{ENTRY_PATH, MIN_DURATION_S, bundled_library, markers::timeline_markers};

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
/// in the source map.
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
    let path = PathBuf::from(ENTRY_PATH);
    let mut diagnostics: Vec<DiagnosticDto> = Vec::new();

    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.add_source(path.clone(), source);
    for (lib_path, lib_source) in bundled_library() {
        graph.add_source(PathBuf::from(lib_path), *lib_source);
    }
    for (module_path, module_source) in modules {
        graph.add_source(module_path.clone(), module_source.clone());
    }

    let mut program = match graph.load_program_with_source(&path, Some(source)) {
        Ok(program) => program,
        // A missing file is not a diagnostic to render — it is the shell's
        // cue to fetch one more module and retry. Report the resolved key so
        // the fetch target is unambiguous.
        Err(ModuleError::FileNotFound(missing)) => {
            return BuiltDocument {
                target: None,
                result: LoadResultDto {
                    ok: false,
                    duration_s: MIN_DURATION_S,
                    width: SceneDimensions::default().width,
                    height: SceneDimensions::default().height,
                    diagnostics: vec![DiagnosticDto {
                        severity: "error".to_string(),
                        code: "module-error".to_string(),
                        message: format!(
                            "Imported module '{}' is not loaded yet; fetch it relative to the \
                             scene, register it with add_module, and call load_source again.",
                            missing.display()
                        ),
                        subject: None,
                        line: None,
                        column: None,
                        span: None,
                    }],
                    missing_imports: vec![missing.display().to_string()],
                    markers: Vec::new(),
                    pause_points: Vec::new(),
                },
            };
        },
        Err(err) => return built_failure(module_error_diagnostics(&err, &path)),
    };

    diagnostics.extend(program.typecheck().iter().map(DiagnosticDto::from_diagnostic));

    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    for error in &expansion_errors {
        diagnostics.push(DiagnosticDto::from_diagnostic(&Diagnostic::error(
            DiagnosticCode::UnknownAction,
            DiagnosticPhase::Build,
            error.clone(),
        )));
    }

    let namespaces: HashMap<String, Namespace> = program.namespaces;
    let context = Arc::new(ExtensionContext::new());
    let has_scenes = expanded.iter().any(|s| matches!(s, Stmt::Scene { .. }));

    let (target, build_diagnostics) = if has_scenes {
        let report = Composition::build_with_font_context_and_asset_cache_and_extension_context(
            &expanded,
            &namespaces,
            font_context,
            quality,
            assets,
            context,
        );
        (BuildTarget::MultiScene(report.output), report.diagnostics)
    } else {
        let report =
            Timeline::build_with_diagnostics_and_font_context_and_asset_cache_and_extension_context(
                &expanded,
                &namespaces,
                font_context,
                quality,
                assets,
                context,
            );
        // `BuildTarget::from_ast*` adds a `PersistTargetNotCarried` warning
        // here for persisted actors with no successor scene, but reads the
        // private `persistence_flags` to do it. The web driver skips that
        // nicety; persistence in a single-scene web document degrades to
        // "state simply resets between loops".
        (BuildTarget::SingleScene(report.output), report.diagnostics)
    };

    diagnostics.extend(build_diagnostics.iter().map(DiagnosticDto::from_diagnostic));

    let (duration_s, width, height) = document_extent(&target);
    let markers = timeline_markers(&expanded, &target);
    let pause_points: Vec<f64> =
        target.pause_points().iter().map(|&ms| ms as f64 / 1000.0).collect();
    let ok = !diagnostics.iter().any(|d| d.severity == "error");
    // A build that produced error diagnostics still parses, so a `BuildTarget`
    // exists — but it is not a document the shell may install. Handing it over
    // would replace the figure with the half-built scene (black, at whatever
    // default resolution the broken source implied) even though the caller
    // treats `ok == false` as "keep the last known good document": that is the
    // live editor's "a typo never blanks the figure". `ok == false` therefore
    // yields no target, matching this module's contract. The extent and
    // markers still describe what the build saw, so a caller can report where
    // it stopped.
    let target = ok.then_some(target);
    BuiltDocument {
        target,
        result: LoadResultDto {
            ok,
            duration_s,
            width,
            height,
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

fn built_failure(diagnostics: Vec<DiagnosticDto>) -> BuiltDocument {
    BuiltDocument {
        target: None,
        result: LoadResultDto {
            ok: false,
            duration_s: MIN_DURATION_S,
            width: SceneDimensions::default().width,
            height: SceneDimensions::default().height,
            diagnostics,
            missing_imports: Vec::new(),
            markers: Vec::new(),
            pause_points: Vec::new(),
        },
    }
}

fn module_error_diagnostics(err: &ModuleError, path: &Path) -> Vec<DiagnosticDto> {
    match err {
        ModuleError::ParseErrors(errors) => errors
            .iter()
            .map(|e| DiagnosticDto::from_diagnostic(&e.to_diagnostic().with_path(path)))
            .collect(),
        other => vec![DiagnosticDto {
            severity: "error".to_string(),
            code: "module-error".to_string(),
            message: other.to_string(),
            subject: None,
            line: None,
            column: None,
            span: None,
        }],
    }
}
