use std::path::Path;

use animatix::composition::BuildTarget;
use animatix_syntax::module::ModuleGraph;
use tracing::error;

pub fn run(input: &Path) {
    let source = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            error!("Cannot read {}: {}", input.display(), e);
            std::process::exit(1);
        },
    };
    let mut module_graph = ModuleGraph::new();
    let (expanded, namespaces, function_context) =
        match module_graph.load_program_with_source(input, Some(&source)) {
            Ok(mut program) => {
                let _ = program.typecheck();
                let expansion = program.expand_components_with_diagnostics();
                if expansion.diagnostics.iter().any(|diagnostic| diagnostic.is_error()) {
                    for diagnostic in expansion.diagnostics {
                        error!("{}", diagnostic.message);
                    }
                    std::process::exit(1);
                }
                let expanded = expansion.output;
                (expanded, program.namespaces, program.function_context)
            },
            Err(e) => {
                error!("Error: {}", e);
                std::process::exit(1);
            },
        };
    let context = std::sync::Arc::new(animatix::extension_context::ExtensionContext::new());
    let report = BuildTarget::build_with_options(
        &expanded.statements,
        &namespaces,
        Some(input),
        animatix::timeline::BuildOptions::default()
            .with_context(context)
            .with_function_context(&function_context)
            .with_expanded(&expanded),
    );
    match report.output {
        BuildTarget::MultiScene(comp) => {
            let summary = comp.summary();
            println!(
                "{:<4} {:<14} {:>8} {:>9}  transition -> next",
                "#", "scene", "start(s)", "dur(s)"
            );
            for (i, (name, start, dur, explicit)) in summary.scenes.iter().enumerate() {
                let next = summary
                    .edges
                    .iter()
                    .find(|(from, _, _, _)| from == name)
                    .map(|(_, to, id, ms)| format!("{} [{}, {}ms]", to, id, ms))
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "{:<4} {:<14} {:>8.2} {:>9.2}  {}{}",
                    i + 1,
                    name,
                    start,
                    dur,
                    next,
                    if explicit.is_some() {
                        ""
                    } else {
                        " (inferred)"
                    }
                );
            }
            println!("total: {:.2}s", summary.total_duration_s);
        },
        BuildTarget::SingleScene(_) => {
            println!("single-timeline file (no `# Scene` declarations)");
        },
    }
}
