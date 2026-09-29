//! Source → diagnostics → buildable document, shared by the wasm driver and
//! native tests.
//!
//! This is the web analogue of the GUI's `DocumentSession::rebuild` pipeline,
//! minus GUI-specific caching: parse/typecheck/expand through the module
//! system in `SourcesOnly` mode (no disk access — the editor owns the source
//! text), then build through the font-context-aware engine entry points.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use animatix::composition::{BuildTarget, Composition};
use animatix::extension_context::ExtensionContext;
use animatix::renderer::text::FontContext;
use animatix::timeline::{BuildQuality, SceneDimensions, Timeline};
use animatix_syntax::ast::Stmt;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use animatix_syntax::module::{ModuleError, ModuleGraph, Namespace, SourceAccess};

use crate::dto::{DiagnosticDto, LoadResultDto};

/// Virtual path the single edited document is registered under. Imports of
/// other files resolve relative to it inside the in-memory source map; the
/// editor only edits this one file, so imports outside the bundled library
/// surface as `FileNotFound` diagnostics instead of disk reads.
pub const ENTRY_PATH: &str = "main.amx";

/// The repo's shared example library (`examples/lib/*.amx`), embedded at
/// compile time and registered in the in-memory source map so example
/// imports (`import "../lib/theme.amx"`) resolve without any disk access.
///
/// Keys are the *resolved* import paths: `resolve_import` joins the import
/// string onto the importing file's directory and normalizes, which maps
/// every library reference onto the `../lib/<name>.amx` shape.
pub fn bundled_library() -> &'static [(&'static str, &'static str)] {
    &[
        ("../lib/actions.amx", include_str!("../../../examples/lib/actions.amx")),
        ("../lib/card.amx", include_str!("../../../examples/lib/card.amx")),
        ("../lib/charts.amx", include_str!("../../../examples/lib/charts.amx")),
        (
            "../lib/colorschemes.amx",
            include_str!("../../../examples/lib/colorschemes.amx"),
        ),
        ("../lib/components.amx", include_str!("../../../examples/lib/components.amx")),
        ("../lib/palette.amx", include_str!("../../../examples/lib/palette.amx")),
        ("../lib/reexport.amx", include_str!("../../../examples/lib/reexport.amx")),
        ("../lib/slide.amx", include_str!("../../../examples/lib/slide.amx")),
        ("../lib/theme.amx", include_str!("../../../examples/lib/theme.amx")),
        ("../lib/tokens.amx", include_str!("../../../examples/lib/tokens.amx")),
        ("../lib/ui.amx", include_str!("../../../examples/lib/ui.amx")),
    ]
}

/// Duration floor for scrubbing/looping behaviour, mirroring the GUI.
const MIN_DURATION_S: f64 = 0.1;

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
    let path = PathBuf::from(ENTRY_PATH);
    let mut diagnostics: Vec<DiagnosticDto> = Vec::new();

    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.add_source(path.clone(), source);
    for (lib_path, lib_source) in bundled_library() {
        graph.add_source(PathBuf::from(lib_path), *lib_source);
    }

    let mut program = match graph.load_program_with_source(&path, Some(source)) {
        Ok(program) => program,
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
            None,
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
                None,
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
    let ok = !diagnostics.iter().any(|d| d.severity == "error");
    BuiltDocument {
        target: Some(target),
        result: LoadResultDto {
            ok,
            duration_s,
            width,
            height,
            diagnostics,
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
            (timeline.duration_seconds().max(MIN_DURATION_S), w, h)
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

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = include_str!("../../../examples/basics/00_hello.amx");
    const MOTION: &str = include_str!("../../../examples/basics/03_timing.amx");
    const MULTISCENE: &str = include_str!("../../../examples/composition/14_multiscene.amx");

    #[test]
    fn builds_self_contained_example_without_errors() {
        let doc = build_document(HELLO, Arc::new(FontContext::new()), BuildQuality::Draft);
        assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
        assert!(doc.result.diagnostics.is_empty());
        assert!(matches!(doc.target, Some(BuildTarget::SingleScene(_))));
        assert!(doc.result.duration_s >= MIN_DURATION_S);
        assert!(doc.result.width > 0 && doc.result.height > 0);
    }

    #[test]
    fn builds_animation_example_with_duration() {
        let doc = build_document(MOTION, Arc::new(FontContext::new()), BuildQuality::Draft);
        assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
        assert!(
            doc.result.duration_s > MIN_DURATION_S,
            "timed example should have real duration"
        );
    }

    #[test]
    fn builds_multiscene_example_as_composition() {
        let doc = build_document(MULTISCENE, Arc::new(FontContext::new()), BuildQuality::Draft);
        assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
        assert!(matches!(doc.target, Some(BuildTarget::MultiScene(_))));
    }

    #[test]
    fn parse_failure_reports_diagnostics_without_target() {
        let doc = build_document(
            "actor this is not valid amx {{{",
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
        );
        assert!(!doc.result.ok);
        assert!(doc.target.is_none());
        assert!(doc.result.diagnostics.iter().any(|d| d.severity == "error"));
    }

    // The transformer demo scenes (web/demos/transformer/scenes) must always
    // build cleanly — they are embedded in the shipped demo page.
    #[test]
    fn transformer_demo_scenes_build_cleanly() {
        const SCENES: &[(&str, &str)] = &[
            ("tokens", include_str!("../../../web/demos/transformer/scenes/tokens.amx")),
            ("positional", include_str!("../../../web/demos/transformer/scenes/positional.amx")),
            ("attention", include_str!("../../../web/demos/transformer/scenes/attention.amx")),
            ("multihead", include_str!("../../../web/demos/transformer/scenes/multihead.amx")),
            ("feedforward", include_str!("../../../web/demos/transformer/scenes/feedforward.amx")),
            ("pipeline", include_str!("../../../web/demos/transformer/scenes/pipeline.amx")),
        ];
        for (name, source) in SCENES {
            let doc = build_document(source, Arc::new(FontContext::new()), BuildQuality::Draft);
            assert!(
                doc.result.ok,
                "demo scene '{name}' failed to build: {:?}",
                doc.result.diagnostics
            );
            assert!(
                doc.result.diagnostics.is_empty(),
                "demo scene '{name}' has diagnostics: {:?}",
                doc.result.diagnostics
            );
            assert!(doc.result.duration_s > MIN_DURATION_S, "scene '{name}' has no duration");
        }
    }

    #[test]
    fn missing_import_is_reported_not_read_from_disk() {
        let doc = build_document(
            "import \"../lib/nonexistent.amx\" as ghost\n",
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
        );
        assert!(!doc.result.ok);
        assert!(
            doc.result
                .diagnostics
                .iter()
                .any(|d| d.message.to_lowercase().contains("not found")),
            "expected a not-found diagnostic, got: {:?}",
            doc.result.diagnostics
        );
    }
}
