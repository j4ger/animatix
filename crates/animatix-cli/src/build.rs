use std::path::Path;

use animatix::composition::BuildTarget;
use animatix_syntax::diagnostics::{Diagnostic, format_diagnostic};
use animatix_syntax::module::ModuleGraph;
use tracing::{error, warn};

use crate::extensions::CliExtensions;

/// Split a `--set NAME=VALUE` argument on the first `=`.
pub fn parse_set_pair(raw: &str) -> Result<(String, String), String> {
    let (name, value) =
        raw.split_once('=').ok_or_else(|| format!("expected NAME=VALUE, got '{raw}'"))?;
    Ok((name.trim().to_string(), value.trim().to_string()))
}

/// Loads an Animatix program from disk, expands components, and builds the
/// appropriate target (single-scene `Timeline` or multi-scene `Composition`).
/// Prints build diagnostics and exits on load failure.
pub fn load_and_build(
    input: &Path,
    extensions: &CliExtensions,
    defines: &[(String, String)],
) -> (BuildTarget, Vec<Diagnostic>) {
    match try_load_and_build(input, extensions, defines) {
        Ok((target, diagnostics)) => {
            print_build_diagnostics(&diagnostics);
            if diagnostics.iter().any(|diagnostic| {
                diagnostic.is_error()
                    && matches!(
                        diagnostic.code,
                        animatix_syntax::diagnostics::DiagnosticCode::InvalidConfigValue
                            | animatix_syntax::diagnostics::DiagnosticCode::VideoClipFailure
                    )
            }) {
                std::process::exit(1);
            }
            (target, diagnostics)
        },
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        },
    }
}

/// Fallible variant of [`load_and_build`] for commands that own their exit
/// codes (`review` maps load failures to exit 2). Does not print diagnostics;
/// callers decide whether to surface them.
pub fn try_load_and_build(
    input: &Path,
    extensions: &CliExtensions,
    defines: &[(String, String)],
) -> Result<(BuildTarget, Vec<Diagnostic>), String> {
    try_load_and_build_in(&mut ModuleGraph::new(), input, extensions, defines)
}

pub fn try_load_and_build_in(
    graph: &mut ModuleGraph,
    input: &Path,
    extensions: &CliExtensions,
    defines: &[(String, String)],
) -> Result<(BuildTarget, Vec<Diagnostic>), String> {
    let mut program = graph.load_program(input).map_err(|e| format!("Error: {e}"))?;
    let mut ctx = animatix::extension_context::ExtensionContext::new();
    let disposers = extensions
        .install_into(&mut ctx)
        .map_err(|e| format!("Plugin install failed: {e}"))?;
    let (expanded, namespaces, function_context, type_diagnostics) = {
        let diagnostics = program.typecheck_with_callables(ctx.callable_descriptors());
        let expanded = program.expand_components().map_err(|error| {
            diagnostics
                .iter()
                .chain(error.diagnostics.iter())
                .map(format_diagnostic)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        (expanded, program.namespaces, program.function_context, diagnostics)
    };

    if type_diagnostics.iter().any(Diagnostic::is_error) {
        return Ok((
            BuildTarget::SingleScene(animatix::timeline::Timeline::new()),
            type_diagnostics,
        ));
    }
    let context = std::sync::Arc::new(ctx);
    let report = BuildTarget::build_with_options(
        &expanded.statements,
        &namespaces,
        Some(input),
        animatix::timeline::BuildOptions::default()
            .with_asset_cache(animatix_render::video_assets::native_asset_cache())
            .with_context(context)
            .with_defines(defines)
            .with_function_context(&function_context)
            .with_expanded(&expanded),
    );
    let mut all_diagnostics = type_diagnostics;
    all_diagnostics.extend(report.diagnostics);
    let _disposers = disposers;
    Ok((report.output, all_diagnostics))
}

pub fn print_build_diagnostics(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        let formatted = format_diagnostic(diagnostic);
        if diagnostic.is_error() {
            error!("{}", formatted);
        } else {
            warn!("{}", formatted);
        }
    }
}
