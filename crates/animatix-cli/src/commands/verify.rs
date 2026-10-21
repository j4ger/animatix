use std::path::{Path, PathBuf};

use animatix::timeline::SceneDimensions;
use tracing::error;

use crate::build::{configured_resolution, load_and_build};
use crate::diagnostics::OutputFormat;
use crate::extensions::CliExtensions;
use crate::verify::{parse_verify_checks, run_verify_checks};

pub fn run(
    input: &Path,
    checks: Option<&PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
    set: &[(String, String)],
    format: &OutputFormat,
    extensions: &CliExtensions,
) {
    let checks_path = checks
        .cloned()
        .unwrap_or_else(|| input.parent().unwrap_or_else(|| Path::new(".")).join("verify.txt"));
    if !checks_path.exists() {
        error!(
            "Checks file not found: {} (pass --checks or add verify.txt beside the input)",
            checks_path.display()
        );
        std::process::exit(1);
    }
    let specs = match parse_verify_checks(&checks_path) {
        Ok(specs) => specs,
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        },
    };
    let (target, _) = load_and_build(input, extensions, set);
    let configured = configured_resolution(&target);
    let dimensions = SceneDimensions {
        width: width.or(configured.map(|(w, _)| w)).unwrap_or(1280),
        height: height.or(configured.map(|(_, h)| h)).unwrap_or(720),
    };
    let mut renderer = match animatix_render::offscreen::OffscreenRenderer::new() {
        Ok(renderer) => renderer,
        Err(e) => {
            error!("Failed to create offscreen renderer: {e}");
            std::process::exit(1);
        },
    };
    let outcomes = match run_verify_checks(&mut renderer, &target, &specs, dimensions) {
        Ok(outcomes) => outcomes,
        Err(e) => {
            error!("Verification could not render: {e}");
            std::process::exit(1);
        },
    };
    let failed = outcomes.iter().filter(|outcome| !outcome.passed).count();

    match format {
        OutputFormat::Json => {
            let results: Vec<serde_json::Value> = outcomes
                .iter()
                .map(|outcome| {
                    serde_json::json!({
                        "line": outcome.line,
                        "check": outcome.raw,
                        "passed": outcome.passed,
                        "detail": outcome.detail,
                    })
                })
                .collect();
            let value = serde_json::json!({
                "input": input.display().to_string(),
                "passed": failed == 0,
                "failed": failed,
                "total": outcomes.len(),
                "results": results,
            });
            println!("{value}");
        },
        OutputFormat::Text => {
            println!("verify: {}", input.display());
            for outcome in &outcomes {
                let status = if outcome.passed { "PASS" } else { "FAIL" };
                println!(
                    "  L{:<4} {:<4} {} — {}",
                    outcome.line, status, outcome.raw, outcome.detail
                );
            }
            println!("verify: {} of {} checks passed", outcomes.len() - failed, outcomes.len());
        },
    }
    if failed > 0 {
        std::process::exit(1);
    }
}
