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
    let mut ctx = animatix::extension_context::ExtensionContext::new();
    let (expanded, namespaces, function_context, type_diagnostics, disposers) = match module_graph
        .load_program_with_source(std::path::Path::new(&file_label), Some(&source))
    {
        Ok(mut program) => {
            let disposers = match extensions.install_into(&mut ctx) {
                Ok(disposers) => disposers,
                Err(e) => {
                    error!("Plugin install failed: {e}");
                    std::process::exit(1);
                },
            };
            let diagnostics = program.typecheck_with_callables(ctx.callable_descriptors());
            let expanded = match program.expand_components() {
                Ok(expanded) => expanded,
                Err(error) => {
                    let mut diagnostics = diagnostics;
                    diagnostics.extend(error.diagnostics);
                    emit_check_report(&file_label, &source, diagnostics, Vec::new(), format);
                    return;
                },
            };
            (expanded, program.namespaces, program.function_context, diagnostics, disposers)
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
    if type_diagnostics.iter().any(Diagnostic::is_error) {
        emit_check_report(&file_label, &source, type_diagnostics, Vec::new(), format);
        return;
    }
    let context = std::sync::Arc::new(ctx);
    let report = BuildTarget::build_with_options(
        &expanded.statements,
        &namespaces,
        if file_label == "-" {
            None
        } else {
            Some(std::path::Path::new(&file_label))
        },
        animatix::timeline::BuildOptions::default()
            .with_asset_cache(animatix_render::video_assets::native_asset_cache())
            .with_context(context)
            .with_defines(set)
            .with_function_context(&function_context)
            .with_expanded(&expanded),
    );
    let _disposers = disposers;
    let mut diagnostics = type_diagnostics;
    diagnostics.extend(report.diagnostics);

    if render_smoke
        && !diagnostics.iter().any(Diagnostic::is_error)
        && let Err(e) = run_render_smoke(&report.output)
    {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::RenderFailure,
            DiagnosticPhase::Render,
            format!("Render smoke test failed: {e}"),
        ));
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
    let semantic = analyzer
        .diagnostics_with_config(&lint_config)
        .into_iter()
        .filter(|diagnostic| {
            !diagnostics.iter().any(|build| {
                build.code == DiagnosticCode::InvalidConfigValue
                    && build.message == diagnostic.message
            })
        })
        .collect();

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
                .render_canvas(timeline, 0.0, dims, DebugRenderOptions::default())
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
                    .render_canvas_transition(
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
                    .render_canvas(
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
