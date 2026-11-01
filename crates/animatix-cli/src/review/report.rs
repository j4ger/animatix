//! Review orchestration and report serialization.

use std::path::Path;

use animatix::timeline::SceneDimensions;

use crate::build::{configured_resolution, print_build_diagnostics, try_load_and_build};
use crate::extensions::CliExtensions;

use super::facts::{FrameFact, ReviewFacts, TargetFacts, collect_facts};
use super::render::run_render_tier;
use super::rules::{self, RuleRun};
use super::samples;
use super::types::{Detail, FailOn, RenderFacts, RenderStatus, ReviewOptions, RuleIssue, Severity};

/// A complete review run.
pub(crate) struct ReviewReport {
    /// Input path as given on the command line.
    pub input: String,
    /// Target identity.
    pub target: TargetFacts,
    /// Render-tier result.
    pub render: RenderFacts,
    /// All measured sample times.
    pub sample_times_s: Vec<f64>,
    /// Structural frame facts.
    pub frames: Vec<FrameFact>,
    /// Rule findings.
    pub issues: Vec<RuleIssue>,
    /// Rule checks skipped because a tier was unavailable.
    pub skipped: usize,
}

impl ReviewReport {
    /// Count issues by severity.
    pub(crate) fn summary_counts(&self) -> (usize, usize, usize) {
        let mut errors = 0;
        let mut warnings = 0;
        let mut infos = 0;
        for issue in &self.issues {
            match issue.severity {
                Severity::Error => errors += 1,
                Severity::Warning => warnings += 1,
                Severity::Info => infos += 1,
            }
        }
        (errors, warnings, infos)
    }

    /// Exit code for the requested fail policy.
    pub(crate) fn exit_code(&self, fail_on: FailOn) -> i32 {
        let (errors, warnings, _) = self.summary_counts();
        match fail_on {
            FailOn::Error if errors > 0 => 1,
            FailOn::Warning if errors + warnings > 0 => 1,
            FailOn::Error | FailOn::Warning | FailOn::Never => 0,
        }
    }

    /// Serialize the stable JSON report schema. `--detail` only shortens the
    /// text report; JSON consumers always receive the complete frame list.
    pub(crate) fn to_json(&self, _detail: Detail) -> serde_json::Value {
        let issues: Vec<serde_json::Value> = self
            .issues
            .iter()
            .map(|issue| {
                serde_json::json!({
                    "rule_id": issue.rule_id,
                    "severity": issue.severity.as_str(),
                    "confidence": issue.confidence.as_str(),
                    "scene": issue.scene,
                    "time_s": issue.time_s,
                    "actor": issue.actor,
                    "source_line": issue.source_line,
                    "message": issue.message,
                    "evidence": issue.evidence,
                    "suggestion": issue.suggestion,
                })
            })
            .collect();
        let frames: Vec<serde_json::Value> = self.frames.iter().map(frame_json).collect();
        let (errors, warnings, infos) = self.summary_counts();
        serde_json::json!({
            "schema_version": 1,
            "input": self.input,
            "target": {
                "kind": self.target.kind.as_str(),
                "duration_s": self.target.duration_s,
                "resolution": [self.target.width, self.target.height],
            },
            "render": {
                "status": self.render.status.as_str(),
                "reason": self.render.reason,
            },
            "sample_times_s": self.sample_times_s,
            "frames": frames,
            "issues": issues,
            "summary": {
                "errors": errors,
                "warnings": warnings,
                "infos": infos,
                "skipped": self.skipped,
            },
        })
    }

    /// Print the text report.
    pub(crate) fn print_text(&self, detail: Detail) {
        println!("review: {}", self.input);
        println!(
            "target: {}, {:.2}s, {}x{}",
            self.target.kind.as_str(),
            self.target.duration_s,
            self.target.width,
            self.target.height
        );
        println!("render: {} ({})", self.render.status.as_str(), self.render.reason);
        let (errors, warnings, infos) = self.summary_counts();
        println!(
            "summary: {errors} error(s), {warnings} warning(s), {infos} info(s), {} skipped",
            self.skipped
        );
        if detail == Detail::Full {
            for frame in &self.frames {
                let ink = frame
                    .ink_fraction
                    .map(|fraction| format!("{:.3}%", fraction * 100.0))
                    .unwrap_or_else(|| "n/a".to_string());
                println!(
                    "  frame {:.3}s [{}] cast={} ink={}",
                    frame.time_s, frame.scene, frame.cast, ink
                );
            }
        }
        if self.issues.is_empty() {
            println!("issues: none");
            return;
        }
        for issue in &self.issues {
            let time = issue
                .time_s
                .map(|time| format!("{time:.2}s"))
                .unwrap_or_else(|| "-".to_string());
            let actor = issue.actor.as_deref().unwrap_or("-");
            let line = issue
                .source_line
                .map(|line| format!("L{line}"))
                .unwrap_or_else(|| "L-".to_string());
            println!(
                "  [{}] {} @ {} {} {}: {}",
                issue.severity.as_str(),
                issue.rule_id,
                time,
                actor,
                line,
                issue.message
            );
            if detail == Detail::Full {
                println!("      suggestion: {}", issue.suggestion);
            }
        }
    }

    /// Suggested `verify.txt` lines from measured facts. Render-dependent
    /// lines are omitted when the render tier was skipped.
    pub(crate) fn suggested_verify_lines(&self) -> Vec<String> {
        let mut lines = vec![
            "# Generated by `animatix review` from measured facts.".to_string(),
            "# Review before using as a gate; thresholds are 80% of measured values.".to_string(),
        ];
        let rendered = self.render.status == RenderStatus::Rendered;
        if rendered {
            for (index, sample) in self.render.samples.iter().enumerate() {
                let threshold = if sample.ink_fraction > 0.0 {
                    (sample.ink_fraction * 0.8).clamp(0.0001, 0.5)
                } else {
                    0.0001
                };
                lines.push(format!("ink {:.3} {:.4}", sample.time_s, threshold));
                let mut visible = 0usize;
                for ((_scene, label), fraction) in &sample.actor_ink {
                    if *fraction >= 0.01 && visible < 8 {
                        lines.push(format!("visible {:.3} {label}", sample.time_s));
                        visible += 1;
                    }
                }
                if index > 0 {
                    let previous = &self.render.samples[index - 1];
                    if sample.changed_from_prev.unwrap_or(0.0) > 0.001 {
                        lines.push(format!(
                            "differs {:.3} {:.3} 0.001",
                            previous.time_s, sample.time_s
                        ));
                    }
                }
            }
            if let Some(rest) = &self.render.site_rest {
                lines.push(format!("ink {:.3} 0.0001", rest.held_time_s));
            }
        } else {
            lines.push("# render tier skipped: only structural `invisible` hints below.".into());
            for issue in &self.issues {
                if issue.rule_id == "actor.opacity-never-positive" {
                    if let Some(actor) = &issue.actor {
                        lines.push(format!("invisible 0.000 {actor}"));
                    }
                }
            }
        }
        lines
    }
}

fn frame_json(frame: &FrameFact) -> serde_json::Value {
    serde_json::json!({
        "time_s": frame.time_s,
        "scene": frame.scene,
        "ink_fraction": frame_ink(frame),
        "cast": frame.cast,
    })
}

fn frame_ink(frame: &FrameFact) -> Option<f32> {
    // Filled from the render map by `run_review` before reporting; structural
    // frames without a render measurement report null.
    frame.ink_fraction
}

/// Run one review.
pub(crate) fn run_review(
    input: &Path,
    set: &[(String, String)],
    extensions: &CliExtensions,
    options: &ReviewOptions,
) -> Result<ReviewReport, String> {
    let (target, diagnostics) = try_load_and_build(input, extensions, set)?;
    if diagnostics.iter().any(|diagnostic| diagnostic.is_error()) {
        print_build_diagnostics(&diagnostics);
        let errors = diagnostics.iter().filter(|diagnostic| diagnostic.is_error()).count();
        return Err(format!("build produced {errors} error diagnostic(s)"));
    }

    let (width, height) = configured_resolution(&target).unwrap_or((1280, 720));
    let dimensions = SceneDimensions { width, height };
    let base_times = samples::compute_sample_times(&target, &options.samples, options.max_frames);
    let mut facts = collect_facts(&target, dimensions, base_times, &diagnostics)?;
    resolve_actor_source_lines(input, &mut facts);
    let render = run_render_tier(
        &target,
        dimensions,
        &facts.frames.iter().map(|frame| frame.time_s).collect::<Vec<_>>(),
        options.policy,
        options.render,
        options.frames_dir.as_deref(),
    )?;

    apply_render_facts(&mut facts, &render);
    let RuleRun {
        issues, skipped, ..
    } = rules::evaluate(&facts, options.policy);
    Ok(ReviewReport {
        input: input.display().to_string(),
        target: facts.target.clone(),
        render,
        sample_times_s: facts.frames.iter().map(|frame| frame.time_s).collect(),
        frames: facts.frames.clone(),
        issues,
        skipped,
    })
}

/// Fill missing declaration lines from the analyzer's label index. Expanded
/// component actors may not carry a declaration span on the built entity; the
/// analyzer can still point at the source declaration the label came from.
pub(crate) fn resolve_actor_source_lines(input: &Path, facts: &mut ReviewFacts) {
    let Ok(source) = std::fs::read_to_string(input) else {
        return;
    };
    let analyzer = animatix_analyzer::Analyzer::new_with_path(&source, Some(input.to_path_buf()));
    for actor in &mut facts.actors {
        if actor.source_line.is_some() {
            continue;
        }
        if let Some((line, _, _, _)) = analyzer.declaration_range(&actor.label) {
            actor.source_line = Some(line + 1);
        }
    }
}

/// Attach render measurements to structural frames and site facts.
fn apply_render_facts(facts: &mut ReviewFacts, render: &RenderFacts) {
    facts.site_rest = render.site_rest.clone();
    facts.render = Some(render.clone());
    for frame in &mut facts.frames {
        frame.ink_fraction = render
            .samples
            .iter()
            .find(|sample| (sample.time_s - frame.time_s).abs() < 0.001)
            .map(|sample| sample.ink_fraction);
    }
}

/// Write `--emit-verify` output. Refuses to overwrite an existing file: the
/// suggested lines are review material, never a silent gate update.
pub(crate) fn write_verify_file(path: &Path, report: &ReviewReport) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "refusing to overwrite existing verify file {}; pass a new path",
            path.display()
        ));
    }
    let mut text = report.suggested_verify_lines().join("\n");
    text.push('\n');
    std::fs::write(path, text).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::facts::TargetFacts;
    use crate::review::types::{
        Confidence, RenderFacts, RenderSampleFacts, RenderStatus, TargetKind,
    };

    fn report_with_issue(severity: Severity) -> ReviewReport {
        ReviewReport {
            input: "demo.amx".into(),
            target: TargetFacts {
                kind: TargetKind::SingleScene,
                duration_s: 1.0,
                width: 64,
                height: 32,
            },
            render: RenderFacts {
                status: RenderStatus::Skipped,
                reason: "--render never".into(),
                samples: Vec::new(),
                site_rest: None,
            },
            sample_times_s: vec![0.0, 1.0],
            frames: vec![FrameFact {
                time_s: 0.0,
                scene: "main".into(),
                cast: 1,
                actors: Vec::new(),
                ink_fraction: None,
                state_digest: None,
            }],
            issues: vec![RuleIssue {
                rule_id: "test.rule",
                severity,
                confidence: Confidence::High,
                scene: None,
                time_s: Some(0.0),
                actor: Some("box".into()),
                source_line: Some(1),
                message: "test".into(),
                evidence: serde_json::json!({}),
                suggestion: "fix it".into(),
            }],
            skipped: 3,
        }
    }

    #[test]
    fn json_schema_has_stable_top_level_keys() {
        let report = report_with_issue(Severity::Warning);
        let json = report.to_json(Detail::Summary);
        for key in [
            "schema_version",
            "input",
            "target",
            "render",
            "sample_times_s",
            "frames",
            "issues",
            "summary",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["summary"]["warnings"], 1);
        assert_eq!(json["frames"].as_array().expect("frames array").len(), 1);
        let full = report.to_json(Detail::Full);
        assert_eq!(full["frames"].as_array().expect("frames array").len(), 1);
    }

    #[test]
    fn exit_policy_distinguishes_warnings_from_errors() {
        let warning = report_with_issue(Severity::Warning);
        assert_eq!(warning.exit_code(FailOn::Error), 0);
        assert_eq!(warning.exit_code(FailOn::Warning), 1);
        assert_eq!(warning.exit_code(FailOn::Never), 0);
        let error = report_with_issue(Severity::Error);
        assert_eq!(error.exit_code(FailOn::Error), 1);
    }

    #[test]
    fn suggested_verify_lines_use_measured_ink() {
        let mut report = report_with_issue(Severity::Warning);
        report.render = RenderFacts {
            status: RenderStatus::Rendered,
            reason: "test".into(),
            samples: vec![RenderSampleFacts {
                time_s: 0.0,
                ink_fraction: 0.05,
                changed_from_prev: None,
                actor_ink: Default::default(),
                transition: None,
            }],
            site_rest: None,
        };
        let lines = report.suggested_verify_lines();
        assert!(lines.iter().any(|line| line.starts_with("ink 0.000 0.0400")));
    }
}
