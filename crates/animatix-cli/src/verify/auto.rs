//! Zero-configuration verification for generated `.amx` files.
//!
//! The auto mode derives its contract from the built scene and the shared
//! `review` facts: visible windows, draw presence, per-actor ink, resting
//! composition, and dead beats. It never requires a `verify.txt`.

use std::path::{Path, PathBuf};

use animatix::timeline::SceneDimensions;
use animatix_syntax::diagnostics::Diagnostic;

use crate::build::{configured_resolution, try_load_and_build};
use crate::extensions::CliExtensions;
use crate::review;
use crate::review::facts::TargetFacts;
use crate::review::render::run_render_tier;
use crate::review::report::resolve_actor_source_lines;
use crate::review::samples;
use crate::review::types::{
    Confidence, Policy, RenderFacts, RenderMode, RenderStatus, SampleSpec, Severity, TargetKind,
};
use crate::review::verify_rules::evaluate_verify;

use super::report::{FixHint, VerifyIssue, VerifyMode, VerifyReport, contract_summary};

/// Run the auto verifier and return a complete report. Build/source failures
/// are represented in the report, not as an early error, so an LLM always
/// receives structured feedback.
#[allow(clippy::too_many_arguments)] // CLI surface mirrors the command flags.
pub(crate) fn run_auto(
    input: &Path,
    set: &[(String, String)],
    extensions: &CliExtensions,
    render: RenderMode,
    max_frames: usize,
    max_issues: usize,
    frames_dir: Option<&PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
) -> VerifyReport {
    let (target, diagnostics) = match try_load_and_build(input, extensions, set) {
        Ok(result) => result,
        Err(message) => {
            return VerifyReport::cannot_run(
                input.display().to_string(),
                message.clone(),
                Some(build_issue("build.error", Severity::Error, message, None)),
            );
        },
    };
    if diagnostics.iter().any(Diagnostic::is_error) {
        let issues = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.is_error())
            .map(|diagnostic| diagnostic_issue(diagnostic, Severity::Error))
            .collect();
        return build_failed_report(input, issues, "build produced error diagnostic(s)");
    }

    let configured = configured_resolution(&target).unwrap_or((1280, 720));
    let dimensions = SceneDimensions {
        width: width.unwrap_or(configured.0),
        height: height.unwrap_or(configured.1),
    };
    let base_times =
        samples::compute_sample_times(&target, &SampleSpec::Keyframes, max_frames.max(2));
    let mut facts =
        match review::facts::collect_facts(&target, dimensions, base_times, &diagnostics) {
            Ok(facts) => facts,
            Err(error) => {
                return VerifyReport::cannot_run(
                    input.display().to_string(),
                    error.clone(),
                    Some(build_issue("build.error", Severity::Error, error, None)),
                );
            },
        };
    resolve_actor_source_lines(input, &mut facts);

    let mut render_times: Vec<f64> = facts
        .frames
        .iter()
        .chain(facts.probe_frames.iter())
        .map(|frame| frame.time_s)
        .collect();
    render_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    render_times.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    let render_facts = match run_render_tier(
        &target,
        dimensions,
        &render_times,
        Policy::Site,
        render,
        frames_dir.map(PathBuf::as_path),
    ) {
        Ok(render) => render,
        Err(error) => {
            return VerifyReport::cannot_run(
                input.display().to_string(),
                error.clone(),
                Some(build_issue("verify.render", Severity::Error, error, None)),
            );
        },
    };
    facts.render = Some(render_facts.clone());
    facts.site_rest = render_facts.site_rest.clone();
    for frame in &mut facts.frames {
        frame.ink_fraction = render_facts
            .samples
            .iter()
            .find(|sample| (sample.time_s - frame.time_s).abs() < 0.001)
            .map(|sample| sample.ink_fraction);
    }

    let mut rule_run = evaluate_verify(&facts);
    for issue in &mut rule_run.issues {
        if issue.scene.is_some() {
            continue;
        }
        let Some(actor) = issue.actor.as_deref() else {
            continue;
        };
        let matches: Vec<_> = facts.actors.iter().filter(|fact| fact.label == actor).collect();
        if matches.len() == 1 {
            issue.scene = Some(matches[0].scene.clone());
        }
    }
    let contract = contract_summary(&facts);
    let checked = rule_run.checked;
    let mut report = VerifyReport::from_review(
        input.display().to_string(),
        VerifyMode::Auto,
        facts.target.clone(),
        render_facts,
        rule_run.issues,
        rule_run.skipped,
        Some(contract),
    );
    report.coverage.checks_run = checked;
    report.summary.checks_run = checked;
    report.apply_max_issues(max_issues);
    report
}

/// Convert a diagnostic into a verify issue with a stable id.
fn diagnostic_issue(diagnostic: &Diagnostic, severity: Severity) -> VerifyIssue {
    let rule_id = if diagnostic.code.to_string() == "never-revealed" {
        "build.never-revealed"
    } else if diagnostic.code.to_string().contains("duplicate") {
        "build.duplicate-label"
    } else {
        "build.error"
    };
    VerifyIssue {
        rule_id: rule_id.to_string(),
        severity,
        confidence: Confidence::High,
        scene: None,
        time_s: None,
        actor: crate::diagnostics::backticked_label(&diagnostic.message),
        source_line: diagnostic.location.line,
        message: diagnostic.message.clone(),
        evidence: serde_json::json!({
            "code": diagnostic.code.to_string(),
            "phase": diagnostic.phase.to_string(),
        }),
        fix: Some(FixHint {
            kind: "fix-diagnostic",
            target: None,
            source_line: diagnostic.location.line,
            snippet: None,
        }),
        suggestion: "Fix the reported build diagnostic before running visual verification."
            .to_string(),
    }
}

fn build_issue(
    rule_id: &str,
    severity: Severity,
    message: String,
    source_line: Option<usize>,
) -> VerifyIssue {
    VerifyIssue {
        rule_id: rule_id.to_string(),
        severity,
        confidence: Confidence::High,
        scene: None,
        time_s: None,
        actor: None,
        source_line,
        message,
        evidence: serde_json::json!({}),
        fix: Some(FixHint {
            kind: "fix-diagnostic",
            target: None,
            source_line,
            snippet: None,
        }),
        suggestion: "Fix the source/build error and rerun verify.".to_string(),
    }
}

fn build_failed_report(input: &Path, issues: Vec<VerifyIssue>, reason: &str) -> VerifyReport {
    let errors = issues.iter().filter(|issue| issue.severity == Severity::Error).count();
    let warnings = issues.iter().filter(|issue| issue.severity == Severity::Warning).count();
    let target = TargetFacts {
        kind: TargetKind::SingleScene,
        duration_s: 0.0,
        width: 0,
        height: 0,
    };
    VerifyReport {
        schema_version: 1,
        input: input.display().to_string(),
        mode: VerifyMode::Auto,
        verdict: if errors > 0 {
            super::report::Verdict::Fail
        } else {
            super::report::Verdict::Warn
        },
        target,
        render: RenderFacts {
            status: RenderStatus::Skipped,
            reason: reason.to_string(),
            samples: Vec::new(),
            site_rest: None,
        },
        coverage: super::report::Coverage {
            structural: "unavailable".to_string(),
            render: "skipped".to_string(),
            complete: false,
            skipped_rules: Vec::new(),
            checks_run: 0,
            checks_skipped: 0,
        },
        contract: None,
        issues,
        summary: super::report::VerifySummary {
            errors,
            warnings,
            ..Default::default()
        },
    }
}
