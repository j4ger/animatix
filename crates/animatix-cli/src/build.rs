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
    let (ast, namespaces, type_diagnostics) = match ModuleGraph::new().load_program(input) {
        Ok(mut program) => {
            let diagnostics = program.typecheck();
            let mut expansion_errors = Vec::new();
            let expanded = program.expand_components(&mut expansion_errors);
            let mut diagnostics = diagnostics;
            for error in expansion_errors {
                diagnostics.push(Diagnostic::error(
                    animatix_syntax::diagnostics::DiagnosticCode::UnknownAction,
                    animatix_syntax::diagnostics::DiagnosticPhase::Build,
                    error,
                ));
            }
            (expanded, program.namespaces, diagnostics)
        },
        Err(e) => {
            error!("Error: {}", e);
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
        Some(input),
        context,
        defines,
    );
    let mut all_diagnostics = type_diagnostics;
    all_diagnostics.extend(report.diagnostics);
    print_build_diagnostics(&all_diagnostics);
    let _disposers = disposers;
    (report.output, all_diagnostics)
}

/// Resolution declared by the file via top-level `config { resolution: (w, h) }`,
/// used as the default export canvas when `--width`/`--height` are omitted.
pub fn configured_resolution(target: &BuildTarget) -> Option<(u32, u32)> {
    match target {
        BuildTarget::SingleScene(timeline) => timeline.resolution(),
        BuildTarget::MultiScene(composition) => {
            composition.scenes.values().next().and_then(|scene| scene.timeline.resolution())
        },
    }
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
