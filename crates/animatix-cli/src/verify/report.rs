//! Serialization for the strict `animatix verify` report.
//!
//! `verify` shares the review fact set but has its own small report contract:
//! a verdict, coverage of each tier, a compact issue list, and a structured
//! repair hint for generated content.

use crate::review::facts::{ReviewFacts, TargetFacts};
use crate::review::types::{Confidence, Detail, RenderFacts, RenderStatus, RuleIssue, Severity};

/// Verification mode selected by the command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VerifyMode {
    /// Zero-config contract derived from the scene.
    Auto,
    /// Declared line-based checks from `verify.txt`.
    #[allow(dead_code)]
    // Kept for the unified report migration; legacy text output still owns this mode.
    Checks,
}

impl VerifyMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Checks => "checks",
        }
    }
}

/// Overall result of a verify run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// No issues at any severity.
    Pass,
    /// Only warnings; may still exit 0 under `--fail-on error`.
    Warn,
    /// At least one error.
    Fail,
    /// The command could not inspect the target.
    CannotRun,
}

impl Verdict {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::CannotRun => "cannot_run",
        }
    }
}

/// Which tiers actually ran.
#[derive(Clone, Debug)]
pub(crate) struct Coverage {
    /// `complete`, `partial`, or `unavailable`.
    pub structural: String,
    /// `rendered`, `skipped`, or `failed`.
    pub render: String,
    /// True only when every configured check ran.
    pub complete: bool,
    /// Rule ids skipped because a tier was unavailable.
    pub skipped_rules: Vec<String>,
    /// Number of rule/check evaluations that ran.
    pub checks_run: usize,
    /// Number of rule/check evaluations that were skipped.
    pub checks_skipped: usize,
}

impl Coverage {
    pub(crate) fn new(render: &RenderFacts, skipped: usize, checks_run: usize) -> Self {
        let render_str = render.status.as_str().to_string();
        let complete = render.status == RenderStatus::Rendered && skipped == 0;
        Self {
            structural: "complete".to_string(),
            render: render_str,
            complete,
            skipped_rules: Vec::new(),
            checks_run,
            checks_skipped: skipped,
        }
    }
}

/// One actionable verify finding.
#[derive(Clone, Debug)]
pub(crate) struct VerifyIssue {
    /// Stable rule identifier.
    pub rule_id: String,
    /// Severity used by the exit policy.
    pub severity: Severity,
    /// Rule confidence.
    pub confidence: Confidence,
    /// Scene owning the finding, when known.
    pub scene: Option<String>,
    /// Sample time, when the finding refers to one.
    pub time_s: Option<f64>,
    /// Actor label, when known.
    pub actor: Option<String>,
    /// 1-based source line, when known.
    pub source_line: Option<usize>,
    /// Human-readable message.
    pub message: String,
    /// Structured measurements.
    pub evidence: serde_json::Value,
    /// Optional repair hint for an LLM/tool.
    pub fix: Option<FixHint>,
    /// Longer suggestion text.
    pub suggestion: String,
}

impl VerifyIssue {
    pub(crate) fn from_rule(issue: RuleIssue) -> Self {
        let fix = fix_for_rule(issue.rule_id, issue.actor.as_deref(), issue.source_line);
        Self {
            rule_id: issue.rule_id.to_string(),
            severity: issue.severity,
            confidence: issue.confidence,
            scene: issue.scene,
            time_s: issue.time_s,
            actor: issue.actor,
            source_line: issue.source_line,
            message: issue.message,
            evidence: issue.evidence,
            fix,
            suggestion: issue.suggestion,
        }
    }
}

/// A machine-readable repair suggestion. It is advisory and never executed.
#[derive(Clone, Debug)]
pub(crate) struct FixHint {
    /// Stable kind from the verify report contract.
    pub kind: &'static str,
    /// Target actor label, if any.
    pub target: Option<String>,
    /// Source line to edit, if known.
    pub source_line: Option<usize>,
    /// Suggested source snippet, if one can be produced safely.
    pub snippet: Option<String>,
}

fn fix_for_rule(rule_id: &str, actor: Option<&str>, source_line: Option<usize>) -> Option<FixHint> {
    let target = actor.map(str::to_string);
    let snippet = actor.map(|actor| format!("fade-in {actor} [600ms]"));
    match rule_id {
        "build.never-revealed"
        | "actor.opacity-never-positive"
        | "actor.never-on-frame"
        | "actor.ink-missing" => Some(FixHint {
            kind: "add-entrance",
            target,
            source_line,
            snippet,
        }),
        "actor.draw-missing" => Some(FixHint {
            kind: "fix-draw",
            target,
            source_line,
            snippet: None,
        }),
        "actor.reveal-missing" => Some(FixHint {
            kind: "fix-opacity",
            target,
            source_line,
            snippet: None,
        }),
        "actor.clipped" => Some(FixHint {
            kind: "move-on-frame",
            target,
            source_line,
            snippet: None,
        }),
        "text.overflow" => Some(FixHint {
            kind: "resize-text",
            target,
            source_line,
            snippet: None,
        }),
        "rest.ink-floor" | "rest.cast-erased" => Some(FixHint {
            kind: "keep-rest",
            target: None,
            source_line: None,
            snippet: None,
        }),
        "beat.static" => Some(FixHint {
            kind: "add-change",
            target,
            source_line,
            snippet: None,
        }),
        "scene.duration" => Some(FixHint {
            kind: "fix-source",
            target: None,
            source_line: None,
            snippet: None,
        }),
        "build.error" | "build.parse" | "build.type" => Some(FixHint {
            kind: "fix-diagnostic",
            target: None,
            source_line,
            snippet: None,
        }),
        _ => None,
    }
}

/// A compact summary of the inferred contract.
#[derive(Clone, Debug, Default)]
pub(crate) struct ContractSummary {
    /// Number of scenes.
    pub scenes: usize,
    /// Number of actors.
    pub actors: usize,
    /// Number of visual actors.
    pub visual_actors: usize,
    /// Number of visible windows found.
    pub visible_windows: usize,
    /// Number of keyframe beats.
    pub beats: usize,
}

/// Counts for the report footer.
#[derive(Clone, Debug, Default)]
pub(crate) struct VerifySummary {
    /// Error count.
    pub errors: usize,
    /// Warning count.
    pub warnings: usize,
    /// Info count.
    pub infos: usize,
    /// Issues dropped by `--max-issues`.
    pub issues_omitted: usize,
    /// Checks/rule evaluations run.
    pub checks_run: usize,
    /// Checks/rule evaluations skipped.
    pub checks_skipped: usize,
}

/// Complete verify report.
#[derive(Clone, Debug)]
pub(crate) struct VerifyReport {
    /// Schema version.
    pub schema_version: u32,
    /// Input path as supplied.
    pub input: String,
    /// Auto or checks mode.
    pub mode: VerifyMode,
    /// Verdict.
    pub verdict: Verdict,
    /// Target identity.
    pub target: TargetFacts,
    /// Render tier facts.
    pub render: RenderFacts,
    /// Tier coverage.
    pub coverage: Coverage,
    /// Contract summary, present in auto mode.
    pub contract: Option<ContractSummary>,
    /// Issues, already sorted and capped.
    pub issues: Vec<VerifyIssue>,
    /// Summary counts.
    pub summary: VerifySummary,
}

impl VerifyReport {
    pub(crate) fn from_review(
        input: String,
        mode: VerifyMode,
        target: TargetFacts,
        render: RenderFacts,
        issues: Vec<RuleIssue>,
        skipped: usize,
        contract: Option<ContractSummary>,
    ) -> Self {
        let mut converted: Vec<VerifyIssue> =
            issues.into_iter().map(VerifyIssue::from_rule).collect();
        converted.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| a.time_s.unwrap_or(-1.0).total_cmp(&b.time_s.unwrap_or(-1.0)))
                .then_with(|| a.rule_id.cmp(&b.rule_id))
                .then_with(|| a.actor.cmp(&b.actor))
        });
        let (errors, warnings, infos) = count_issues(&converted);
        let verdict = if errors > 0 {
            Verdict::Fail
        } else if warnings > 0 {
            Verdict::Warn
        } else {
            Verdict::Pass
        };
        let checks_run = converted.len();
        let coverage = Coverage::new(&render, skipped, checks_run);
        let summary = VerifySummary {
            errors,
            warnings,
            infos,
            issues_omitted: 0,
            checks_run,
            checks_skipped: skipped,
        };
        Self {
            schema_version: 1,
            input,
            mode,
            verdict,
            target,
            render,
            coverage,
            contract,
            issues: converted,
            summary,
        }
    }

    /// Cap the issue list without ever dropping errors.
    pub(crate) fn apply_max_issues(&mut self, max_issues: usize) {
        if max_issues == 0 || self.issues.len() <= max_issues {
            return;
        }
        let mut kept = Vec::new();
        let mut omitted = 0;
        for issue in self.issues.drain(..) {
            if issue.severity == Severity::Error || kept.len() < max_issues {
                kept.push(issue);
            } else {
                omitted += 1;
            }
        }
        self.issues = kept;
        self.summary.issues_omitted = omitted;
        fill_counts(&mut self.summary, &self.issues);
    }

    /// Compact report for an LLM repair loop. It intentionally drops frame
    /// facts and suggestions; the full report remains available with
    /// `--format json` without `--brief`.
    pub(crate) fn to_brief_json(&self) -> serde_json::Value {
        let issues: Vec<serde_json::Value> = self
            .issues
            .iter()
            .map(|issue| {
                serde_json::json!({
                    "rule_id": issue.rule_id,
                    "severity": issue.severity.as_str(),
                    "scene": issue.scene,
                    "time_s": issue.time_s,
                    "actor": issue.actor,
                    "source_line": issue.source_line,
                    "message": issue.message,
                    "fix": issue.fix.as_ref().map(|fix| serde_json::json!({
                        "kind": fix.kind,
                        "target": fix.target,
                        "snippet": fix.snippet,
                    })),
                })
            })
            .collect();
        serde_json::json!({
            "schema_version": self.schema_version,
            "kind": "verify.brief",
            "verdict": self.verdict.as_str(),
            "summary": {
                "errors": self.summary.errors,
                "warnings": self.summary.warnings,
                "issues_omitted": self.summary.issues_omitted,
            },
            "issues": issues,
        })
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
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
                    "fix": issue.fix.as_ref().map(|fix| serde_json::json!({
                        "kind": fix.kind,
                        "target": fix.target,
                        "source_line": fix.source_line,
                        "snippet": fix.snippet,
                    })),
                    "suggestion": issue.suggestion,
                })
            })
            .collect();
        serde_json::json!({
            "schema_version": self.schema_version,
            "kind": "verify",
            "input": self.input,
            "mode": self.mode.as_str(),
            "verdict": self.verdict.as_str(),
            "target": {
                "kind": self.target.kind.as_str(),
                "duration_s": self.target.duration_s,
                "resolution": [self.target.width, self.target.height],
            },
            "render": {
                "status": self.render.status.as_str(),
                "reason": self.render.reason,
            },
            "coverage": {
                "structural": self.coverage.structural,
                "render": self.coverage.render,
                "complete": self.coverage.complete,
                "skipped_rules": self.coverage.skipped_rules,
                "checks_run": self.coverage.checks_run,
                "checks_skipped": self.coverage.checks_skipped,
            },
            "contract": self.contract.as_ref().map(|contract| serde_json::json!({
                "scenes": contract.scenes,
                "actors": contract.actors,
                "visual_actors": contract.visual_actors,
                "visible_windows": contract.visible_windows,
                "beats": contract.beats,
            })),
            "issues": issues,
            "summary": {
                "errors": self.summary.errors,
                "warnings": self.summary.warnings,
                "infos": self.summary.infos,
                "issues_omitted": self.summary.issues_omitted,
                "checks_run": self.summary.checks_run,
                "checks_skipped": self.summary.checks_skipped,
            },
        })
    }

    pub(crate) fn print_text(&self, detail: Detail) {
        println!(
            "verify: {} ({} mode, {})",
            self.input,
            self.mode.as_str(),
            self.verdict.as_str()
        );
        println!(
            "target: {}, {:.2}s, {}x{}",
            self.target.kind.as_str(),
            self.target.duration_s,
            self.target.width,
            self.target.height
        );
        println!(
            "coverage: structural {}, render {}, complete {}",
            self.coverage.structural, self.coverage.render, self.coverage.complete
        );
        if self.issues.is_empty() {
            println!("issues: none");
        } else {
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
                    if let Some(fix) = &issue.fix
                        && let Some(snippet) = &fix.snippet
                    {
                        println!("      fix: {snippet}");
                    }
                    println!("      hint: {}", issue.suggestion);
                }
            }
        }
        println!(
            "verify: {} error(s), {} warning(s), {} info(s), {} omitted",
            self.summary.errors,
            self.summary.warnings,
            self.summary.infos,
            self.summary.issues_omitted
        );
    }

    pub(crate) fn exit_code(&self, fail_on: crate::review::types::FailOn) -> i32 {
        use crate::review::types::FailOn;
        match fail_on {
            FailOn::Error if self.summary.errors > 0 => 1,
            FailOn::Warning if self.summary.errors + self.summary.warnings > 0 => 1,
            FailOn::Error | FailOn::Warning | FailOn::Never => 0,
        }
    }

    /// Build a report for a target that could not be inspected at all.
    pub(crate) fn cannot_run(input: String, message: String, issue: Option<VerifyIssue>) -> Self {
        let issues: Vec<VerifyIssue> = issue.into_iter().collect();
        let (errors, warnings, infos) = count_issues(&issues);
        let summary = VerifySummary {
            errors,
            warnings,
            infos,
            checks_run: 0,
            checks_skipped: 0,
            issues_omitted: 0,
        };
        let render = RenderFacts {
            status: RenderStatus::Skipped,
            reason: message,
            samples: Vec::new(),
            site_rest: None,
        };
        let target = TargetFacts {
            kind: crate::review::types::TargetKind::SingleScene,
            duration_s: 0.0,
            width: 0,
            height: 0,
        };
        Self {
            schema_version: 1,
            input,
            mode: VerifyMode::Auto,
            verdict: Verdict::CannotRun,
            target,
            render,
            coverage: Coverage {
                structural: "unavailable".to_string(),
                render: "skipped".to_string(),
                complete: false,
                skipped_rules: Vec::new(),
                checks_run: 0,
                checks_skipped: 0,
            },
            contract: None,
            issues,
            summary,
        }
    }
}

fn count_issues(issues: &[VerifyIssue]) -> (usize, usize, usize) {
    let mut errors = 0;
    let mut warnings = 0;
    let mut infos = 0;
    for issue in issues {
        match issue.severity {
            Severity::Error => errors += 1,
            Severity::Warning => warnings += 1,
            Severity::Info => infos += 1,
        }
    }
    (errors, warnings, infos)
}

fn fill_counts(summary: &mut VerifySummary, issues: &[VerifyIssue]) {
    let (errors, warnings, infos) = count_issues(issues);
    summary.errors = errors;
    summary.warnings = warnings;
    summary.infos = infos;
}

/// Build a compact contract summary from facts. This is intentionally
/// descriptive only; rules do their own measurement lookups.
pub(crate) fn contract_summary(facts: &ReviewFacts) -> ContractSummary {
    let beats = facts.frames.iter().filter(|frame| !frame.scene.contains(" -> ")).count();
    let visible_windows = facts
        .actors
        .iter()
        .filter(|actor| {
            actor.visual
                && !actor.is_group_like
                && !actor.is_effect_scope
                && facts.all_frames().any(|frame| {
                    facts.actor_frame(frame, &actor.scene, &actor.label).is_some_and(
                        |frame_actor| {
                            frame_actor.opacity.unwrap_or(0.0) > 0.01
                                && frame_actor.padded_on_frame_fraction > 0.0
                        },
                    )
                })
        })
        .count();
    ContractSummary {
        scenes: facts
            .actors
            .iter()
            .map(|actor| actor.scene.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        actors: facts.actors.len(),
        visual_actors: facts.actors.iter().filter(|actor| actor.visual).count(),
        visible_windows,
        beats,
    }
}
