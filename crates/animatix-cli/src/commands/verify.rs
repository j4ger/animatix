use std::path::{Path, PathBuf};

use animatix::timeline::SceneDimensions;
use tracing::error;

use crate::build::{configured_resolution, load_and_build};
use crate::diagnostics::OutputFormat;
use crate::extensions::CliExtensions;
use crate::review::types::{Detail, FailOn, RenderMode};
use crate::verify::auto::run_auto;
use crate::verify::report::Verdict;
use crate::verify::{parse_verify_checks, run_verify_checks};

/// `animatix verify` entry point.
///
/// Mode selection is deliberately conservative: an existing `verify.txt`
/// beside the input keeps the legacy explicit mode, while a generated file
/// without a sidecar gets the zero-config auto verifier. `--auto` forces the
/// new path for a project that still carries an old sidecar.
#[allow(clippy::too_many_arguments)] // CLI surface mirrors the command flags.
pub(crate) fn run(
    input: &Path,
    checks: Option<&PathBuf>,
    auto: bool,
    render: RenderMode,
    max_issues: usize,
    max_frames: usize,
    fail_on: FailOn,
    detail: Detail,
    brief: bool,
    frames_dir: Option<&PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
    set: &[(String, String)],
    format: &OutputFormat,
    extensions: &CliExtensions,
) {
    if auto && checks.is_some() {
        error!("--auto and --checks are mutually exclusive");
        std::process::exit(2);
    }

    let default_checks = input.parent().unwrap_or_else(|| Path::new(".")).join("verify.txt");
    let checks_path = if auto {
        None
    } else {
        checks.cloned().or_else(|| default_checks.exists().then_some(default_checks))
    };

    if let Some(path) = checks_path {
        run_checks(input, &path, width, height, set, format, extensions);
    } else {
        run_auto_command(
            input, render, max_issues, max_frames, fail_on, detail, brief, frames_dir, width,
            height, set, format, extensions,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn run_auto_command(
    input: &Path,
    render: RenderMode,
    max_issues: usize,
    max_frames: usize,
    fail_on: FailOn,
    detail: Detail,
    brief: bool,
    frames_dir: Option<&PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
    set: &[(String, String)],
    format: &OutputFormat,
    extensions: &CliExtensions,
) {
    let report = run_auto(
        input, set, extensions, render, max_frames, max_issues, frames_dir, width, height,
    );
    match format {
        OutputFormat::Json => {
            if brief {
                println!("{}", report.to_brief_json());
            } else {
                println!("{}", report.to_json());
            }
        },
        OutputFormat::Text => report.print_text(detail),
    }
    let code = if report.verdict == Verdict::CannotRun {
        2
    } else {
        report.exit_code(fail_on)
    };
    if code != 0 {
        std::process::exit(code);
    }
}

fn run_checks(
    input: &Path,
    checks_path: &Path,
    width: Option<u32>,
    height: Option<u32>,
    set: &[(String, String)],
    format: &OutputFormat,
    extensions: &CliExtensions,
) {
    let specs = match parse_verify_checks(checks_path) {
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
