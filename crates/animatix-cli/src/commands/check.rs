use animatix::composition::BuildTarget;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use animatix_syntax::module::ModuleGraph;
use tracing::error;

use crate::diagnostics::{
    OutputFormat, emit_check_report, module_error_diagnostics, resolve_build_diagnostic_positions,
};
use crate::extensions::CliExtensions;

pub fn run(
    file: &str,
    render_smoke: bool,
    format: &OutputFormat,
    set: &[(String, String)],
    extensions: &CliExtensions,
) {
    let (source, file_label) = if file == "-" {
        let source = match std::io::read_to_string(std::io::stdin()) {
            Ok(s) => s,
            Err(e) => {
                error!("Cannot read from stdin: {}", e);
                std::process::exit(1);
            },
        };
        (source, "-".to_string())
    } else {
        let source = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                error!("Cannot read {}: {}", file, e);
                std::process::exit(1);
            },
        };
        (source, file.to_string())
    };
    let mut module_graph = ModuleGraph::new();
    let (ast, namespaces, type_diagnostics) = match module_graph
        .load_program_with_source(std::path::Path::new(&file_label), Some(&source))
    {
        Ok(mut program) => {
            let diagnostics = program.typecheck();
            let mut expansion_errors = Vec::new();
            let expanded = program.expand_components(&mut expansion_errors);
            let mut diagnostics = diagnostics;
            for error in expansion_errors {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::UnknownAction,
                    DiagnosticPhase::Build,
                    error,
                ));
            }
            (expanded, program.namespaces, diagnostics)
        },
        Err(e) => {
            // Report the parse failure through the normal diagnostic
            // channel (stdout, honouring --format) instead of exiting
            // on a tracing line. The branch only fires when the module
            // graph could not parse, so the whole report is the parse
            // failure — running the analyzer too would duplicate every
            // error, since it re-parses the same source.
            let parse_diagnostics = module_error_diagnostics(&e, &source);
            emit_check_report(&file_label, &source, parse_diagnostics, Vec::new(), format);
            // Every parse failure is an error, so this always exits 1.
            std::process::exit(1);
        },
    };
    let mut ctx = animatix::extension_context::ExtensionContext::new();
    let disposers = match extensions.install_into(&mut ctx) {
        Ok(disposers) => disposers,
        Err(e) => {
            error!("Plugin install failed: {e}");
            std::process::exit(1);
        },
    };
    let context = std::sync::Arc::new(ctx);
    let report = BuildTarget::from_ast_with_context_and_defines(
        &ast,
        &namespaces,
        if file_label == "-" {
            None
        } else {
            Some(std::path::Path::new(&file_label))
        },
        context,
        set,
    );
    let _disposers = disposers;
    let mut diagnostics = type_diagnostics;
    diagnostics.extend(report.diagnostics);

    if render_smoke {
        if let Err(e) = run_render_smoke(&report.output) {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::RenderFailure,
                DiagnosticPhase::Render,
                format!("Render smoke test failed: {e}"),
            ));
        }
    }

    // Run semantic analysis (mirrors lint command behavior)
    let mut analyzer = animatix_analyzer::Analyzer::new_with_path(
        &source,
        if file_label == "-" {
            None
        } else {
            Some(std::path::PathBuf::from(&file_label))
        },
    )
    .with_extension_manifest(extensions.merged_manifest());
    analyzer.merge_import_symbols();
    let lint_config = animatix_analyzer::LintConfig::from_source(&source);
    let semantic = analyzer.diagnostics_with_config(&lint_config);

    // Build-layer diagnostics that name an actor but carry no position
    // (they are produced from the runtime track table, which has no
    // source spans) are anchored at that actor's declaration, so the
    // report points somewhere useful instead of at 1:1.
    resolve_build_diagnostic_positions(&mut diagnostics, &analyzer);

    emit_check_report(&file_label, &source, diagnostics, semantic, format);
}

/// Render a single frame at time=0 to catch renderer bugs early.
pub fn run_render_smoke(target: &BuildTarget) -> Result<(), String> {
    use animatix::timeline::{DebugRenderOptions, SceneDimensions};
    use animatix_render::offscreen::OffscreenRenderer;

    let mut renderer = OffscreenRenderer::new().map_err(|e| e.to_string())?;
    let dims = SceneDimensions {
        width: 320,
        height: 180,
    };

    match target {
        BuildTarget::SingleScene(timeline) => {
            renderer
                .render_timeline_with_debug(timeline, 0.0, dims, DebugRenderOptions::default())
                .map_err(|e| e.to_string())?;
        },
        BuildTarget::MultiScene(composition) => {
            if !composition.has_scenes() {
                return Err("Composition has no scenes to render".into());
            }
            let (scene_name, local_time_s, transition_blend) = composition.evaluate(0.0);
            if let Some(blend) = transition_blend {
                let from_scene = composition
                    .scenes
                    .get(&blend.from_scene)
                    .ok_or_else(|| format!("From scene '{}' not found", blend.from_scene))?;
                let to_scene = composition
                    .scenes
                    .get(&blend.to_scene)
                    .ok_or_else(|| format!("To scene '{}' not found", blend.to_scene))?;
                renderer
                    .render_transition(
                        &from_scene.timeline,
                        blend.from_local,
                        &to_scene.timeline,
                        blend.to_local,
                        blend.progress as f32,
                        blend.id.clone(),
                        blend.easing,
                        dims,
                        DebugRenderOptions::default(),
                    )
                    .map_err(|e| e.to_string())?;
            } else {
                let scene = composition
                    .scenes
                    .get(&scene_name)
                    .ok_or_else(|| format!("Scene '{}' not found", scene_name))?;
                renderer
                    .render_timeline_with_debug(
                        &scene.timeline,
                        local_time_s,
                        dims,
                        DebugRenderOptions::default(),
                    )
                    .map_err(|e| e.to_string())?;
            }
        },
    }

    Ok(())
}
