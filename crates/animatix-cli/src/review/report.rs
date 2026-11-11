//! Audit orchestration and report serialization.

use std::path::Path;

use animatix::timeline::SceneDimensions;

use crate::build::{configured_resolution, print_build_diagnostics, try_load_and_build};
use crate::extensions::CliExtensions;

use super::facts::{FrameFact, ReviewFacts, TargetFacts, collect_facts};
use super::render::run_render_tier;
use super::rules::{self, RuleRun};
use super::samples;
use super::types::{Detail, FailOn, RenderFacts, RenderStatus, ReviewOptions, RuleIssue, Severity};

/// A complete audit run.
pub(crate) struct AuditReport {
    /// Input path as given on the command line.
    pub input: String,
    /// Audit kind.
    pub kind: &'static str,
    /// Target identity.
    pub target: TargetFacts,
    /// Render-tier result.
    pub render: RenderFacts,
    /// All measured sample times.
    pub sample_times_s: Vec<f64>,
    /// Structural frame facts.
    pub frames: Vec<FrameFact>,
    /// Rule findings.
    pub findings: Vec<RuleIssue>,
    /// Aggregate audit summary.
    pub summary: Summary,
    /// Rule checks skipped because a tier was unavailable.
    pub skipped: usize,
    /// Coverage metrics.
    pub coverage: Coverage,
    pub delta: Option<crate::review::delta::DeltaReport>,
    pub budget: Option<crate::review::budget::BudgetReport>,
}

impl AuditReport {
    /// Count issues by severity.
    #[allow(dead_code)]
    pub(crate) fn summary_counts(&self) -> (usize, usize, usize) {
        (self.summary.errors, self.summary.warnings, self.summary.infos)
    }

    pub(crate) fn exit_code(&self, fail_on: FailOn) -> i32 {
        if self.budget.as_ref().is_some_and(|budget| !budget.exceeded.is_empty()) {
            return 1;
        }
        // With `--delta`, only findings introduced since the baseline can fail
        // the run; existing debt is the budget/ratchet's concern.
        let (errors, warnings) = match &self.delta {
            Some(delta) => (delta.new_errors, delta.new_warnings),
            None => (self.summary.errors, self.summary.warnings),
        };
        match fail_on {
            FailOn::Error if errors > 0 => 1,
            FailOn::Warning if errors + warnings > 0 => 1,
            FailOn::Error | FailOn::Warning | FailOn::Never => 0,
        }
    }

    /// Serialize the stable JSON report schema. `--detail` only shortens the
    /// text report; JSON consumers always receive the complete frame list.
    pub(crate) fn to_json(&self, _detail: Detail) -> serde_json::Value {
        let findings: Vec<serde_json::Value> = self
            .findings
            .iter()
            .map(|issue| {
                serde_json::json!({
                    "code": issue.rule_id,
                    "severity": issue.severity.as_str(),
                    "persistence": match issue.persistence {
                        crate::review::types::Persistence::Transient => "transient",
                        crate::review::types::Persistence::Held => "held",
                    },
                    "scene": issue.scene,
                    "time_s": issue.time_s,
                    "actor": issue.actor,
                    "source_line": issue.source_line,
                    "message": issue.message,
                    "evidence": issue.evidence,
                    "anchor": {
                        "time_s": issue.time_s,
                        "frame_path": issue.evidence.get("frame_path"),
                        "crop_path": issue.evidence.get("crop_path"),
                        "annotated_path": issue.evidence.get("annotated_path"),
                    },
                    "fix": issue.fix_hint.as_ref().map(|f| serde_json::json!({
                        "kind": f.kind,
                        "target": f.target,
                        "source_line": f.source_line,
                        "snippet": f.snippet,
                        "marker": f.marker,
                        "applicability": f.applicability.unwrap_or("none"),
                    })),
                    "waived_by": issue.waived_by,
                    "suggestion": issue.suggestion,
                    "baseline_state": issue.baseline_state,
                })
            })
            .collect();
        let frames: Vec<serde_json::Value> =
            self.frames.iter().map(|f| frame_json(f, Some(&self.render))).collect();
        serde_json::json!({
            "schema_version": 3,
            "kind": self.kind,
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
            "coverage": {
                "structural": self.coverage.structural,
                "render": self.coverage.render,
                "complete": self.coverage.complete,
                "skipped_rules": self.coverage.skipped_rules,
                "checks_run": self.coverage.checks_run,
                "checks_skipped": self.coverage.checks_skipped,
                "evidence": self.coverage.evidence,
            },
            "findings": findings,
            "summary": {
                "errors": self.summary.errors,
                "warnings": self.summary.warnings,
                "infos": self.summary.infos,
                "candidates": self.summary.candidates,
                "skipped": self.skipped,
                "waived": self.summary.waived,
            },
            "delta": self.delta.as_ref().map(|d| serde_json::json!({
                "baseline": d.baseline,
                "new": d.new,
                "resolved": d.resolved,
                "unchanged": d.unchanged,
                "new_errors": d.new_errors,
                "new_warnings": d.new_warnings,
            })),
            "budget": self.budget.as_ref().map(|b| serde_json::json!({
                "path": b.path,
                "exceeded": b.exceeded.iter().map(|e| serde_json::json!({
                    "file": e.file,
                    "code": e.code,
                    "count": e.count,
                    "allowed": e.allowed,
                })).collect::<Vec<_>>(),
                "updated": b.updated,
            })),
        })
    }

    fn verdict(&self) -> &'static str {
        let (errors, warnings) = match &self.delta {
            Some(delta) => (delta.new_errors, delta.new_warnings),
            None => (self.summary.errors, self.summary.warnings),
        };
        if errors > 0 {
            "fail"
        } else if warnings > 0 {
            "warn"
        } else {
            "pass"
        }
    }

    /// Serialize the brief JSON report schema: bounded "where to look" anchors.
    pub(crate) fn to_brief_json(
        &self,
        anchors_limit: usize,
        include_candidates: bool,
    ) -> serde_json::Value {
        let has_image = |issue: &RuleIssue| issue.evidence.get("crop_path").is_some();
        let mut sorted: Vec<&RuleIssue> = self
            .findings
            .iter()
            .filter(|issue| {
                include_candidates || issue.severity != Severity::Info || issue.waived_by.is_some()
            })
            .collect();
        sorted.sort_by(|a, b| {
            has_image(b)
                .cmp(&has_image(a))
                .then_with(|| b.severity.cmp(&a.severity))
                .then_with(|| a.time_s.partial_cmp(&b.time_s).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.rule_id.cmp(b.rule_id))
        });
        let total = sorted.len();
        let anchors: Vec<serde_json::Value> = sorted
            .into_iter()
            .take(anchors_limit)
            .map(|issue| {
                serde_json::json!({
                    "code": issue.rule_id,
                    "severity": issue.severity.as_str(),
                    "scene": issue.scene,
                    "time_s": issue.time_s,
                    "actor": issue.actor,
                    "source_line": issue.source_line,
                    "message": issue.message,
                    "frame_path": issue.evidence.get("frame_path"),
                    "crop_path": issue.evidence.get("crop_path"),
                    "annotated_path": issue.evidence.get("annotated_path"),
                    "waived_by": issue.waived_by,
                    "fix": issue.fix_hint.as_ref().map(|f| serde_json::json!({
                        "kind": f.kind,
                        "target": f.target,
                        "source_line": f.source_line,
                        "snippet": f.snippet,
                        "marker": f.marker,
                        "applicability": f.applicability.unwrap_or("none"),
                    })),
                })
            })
            .collect();
        serde_json::json!({
            "schema_version": 3,
            "kind": "audit.brief",
            "verdict": self.verdict(),
            "summary": {
                "errors": self.summary.errors,
                "warnings": self.summary.warnings,
                "infos": self.summary.infos,
                "candidates": self.summary.candidates,
                "skipped": self.skipped,
                "waived": self.summary.waived,
            },
            "anchors": anchors,
            "omitted": total.saturating_sub(anchors_limit),
            "delta": self.delta.as_ref().map(|d| serde_json::json!({
                "baseline": d.baseline,
                "new": d.new,
                "resolved": d.resolved,
                "unchanged": d.unchanged,
                "new_errors": d.new_errors,
                "new_warnings": d.new_warnings,
            })),
            "budget": self.budget.as_ref().map(|b| serde_json::json!({
                "path": b.path,
                "exceeded": b.exceeded.iter().map(|e| serde_json::json!({
                    "file": e.file,
                    "code": e.code,
                    "count": e.count,
                    "allowed": e.allowed,
                })).collect::<Vec<_>>(),
                "updated": b.updated,
            })),
        })
    }

    /// Print the text report.
    pub(crate) fn print_text(&self, detail: Detail, include_candidates: bool) {
        println!("audit: {}", self.input);
        println!(
            "target: {}, {:.2}s, {}x{}",
            self.target.kind.as_str(),
            self.target.duration_s,
            self.target.width,
            self.target.height
        );
        println!("render: {} ({})", self.render.status.as_str(), self.render.reason);
        println!(
            "summary: {} error(s), {} warning(s), {} info(s), {} skipped, {} waived",
            self.summary.errors,
            self.summary.warnings,
            self.summary.infos,
            self.skipped,
            self.summary.waived
        );
        if let Some(delta) = &self.delta {
            println!(
                "delta: new={}, resolved={}, unchanged={}",
                delta.new, delta.resolved, delta.unchanged
            );
        }
        if let Some(budget) = &self.budget {
            if !budget.exceeded.is_empty() {
                println!("budget: exceeded for {} codes", budget.exceeded.len());
            } else if budget.updated {
                println!("budget: updated");
            }
        }
        if detail == Detail::Full {
            for frame in &self.frames {
                // Simplified display for brevity
                println!("  frame {:.3}s [{}] cast={}", frame.time_s, frame.scene, frame.cast);
            }
        }
        if self.findings.is_empty() {
            println!("findings: none");
            return;
        }
        for finding in &self.findings {
            if !include_candidates && finding.severity == Severity::Info {
                continue;
            }
            let time = finding
                .time_s
                .map(|time| format!("{time:.2}s"))
                .unwrap_or_else(|| "-".to_string());
            let actor = finding.actor.as_deref().unwrap_or("-");
            let line = finding
                .source_line
                .map(|line| format!("L{line}"))
                .unwrap_or_else(|| "L-".to_string());
            println!(
                "  [{}] {} @ {} {} {}: {}",
                finding.severity.as_str(),
                finding.rule_id,
                time,
                actor,
                line,
                finding.message
            );
            if detail == Detail::Full {
                println!("      suggestion: {}", finding.suggestion);
            }
        }
    }
}

fn write_evidence(
    dir: &Path,
    findings: &mut [RuleIssue],
    render: &RenderFacts,
    include_candidates: bool,
) {
    if render.status != RenderStatus::Rendered {
        return;
    }
    for (finding_index, finding) in findings.iter_mut().enumerate() {
        // Info findings are already-explained observations; cropping them
        // would bury the actionable candidates in noise, unless --candidates
        // is specified.
        if finding.severity < Severity::Warning && !include_candidates {
            continue;
        }
        let Some(time_s) = finding.time_s else {
            continue;
        };
        // The structural tier may sample at times the render tier never
        // rendered. Crop the nearest rendered frame and record which one,
        // so the evidence stays honest about the frame it shows.
        let nearest = render.samples.iter().enumerate().min_by(|(_, a), (_, b)| {
            (a.time_s - time_s).abs().total_cmp(&(b.time_s - time_s).abs())
        });
        let Some((idx, sample)) = nearest else {
            continue;
        };
        let frame_time_s = sample.time_s;
        let frame_path = dir.join(format!("review_{idx:04}_{frame_time_s:.2}s.png"));
        if !frame_path.exists() {
            continue;
        }

        let img = image::open(&frame_path);
        let Ok(img) = img else {
            continue;
        };
        let mut img = img.to_rgba8();

        let bounds = crop_bounds(&finding.evidence);

        if let Some([x0, y0, x1, y1]) = bounds {
            // Annotated overlay
            let red = image::Rgba([255, 59, 48, 255]);
            let w = img.width() as i32;
            let h = img.height() as i32;

            // Draw 2px border
            for thickness in 0..2 {
                for x in x0.saturating_sub(thickness).max(0)..x1.saturating_add(thickness).min(w) {
                    for y in [
                        y0.saturating_sub(thickness),
                        y1.saturating_add(thickness).saturating_sub(1),
                    ] {
                        if y >= 0 && y < h {
                            img.put_pixel(x as u32, y as u32, red);
                        }
                    }
                }
                for y in y0.saturating_sub(thickness).max(0)..y1.saturating_add(thickness).min(h) {
                    for x in [
                        x0.saturating_sub(thickness),
                        x1.saturating_add(thickness).saturating_sub(1),
                    ] {
                        if x >= 0 && x < w {
                            img.put_pixel(x as u32, y as u32, red);
                        }
                    }
                }
            }

            let safe_code = finding.rule_id.replace(['.', '-'], "_");
            let annotated_name =
                format!("{safe_code}_{finding_index:04}_{frame_time_s:.2}s.annotated.png");
            let annotated_path = dir.join(&annotated_name);
            if img.save(&annotated_path).is_ok()
                && let serde_json::Value::Object(ref mut map) = finding.evidence
            {
                map.insert("annotated_path".to_string(), serde_json::json!(annotated_path));
            }

            // Reset image for crop
            let img = image::open(&frame_path).unwrap().to_rgba8();

            let cx0 = x0.saturating_sub(16).max(0) as u32;
            let cy0 = y0.saturating_sub(16).max(0) as u32;
            let cx1 = x1.saturating_add(16).min(img.width() as i32).max(0) as u32;
            let cy1 = y1.saturating_add(16).min(img.height() as i32).max(0) as u32;

            let width = cx1.saturating_sub(cx0);
            let height = cy1.saturating_sub(cy0);

            if width > 0 && height > 0 {
                let crop = image::imageops::crop_imm(&img, cx0, cy0, width, height).to_image();
                let crop_name = format!("{safe_code}_{finding_index:04}_{frame_time_s:.2}s.png");
                let crop_path = dir.join(&crop_name);
                if crop.save(&crop_path).is_ok()
                    && let serde_json::Value::Object(ref mut map) = finding.evidence
                {
                    map.insert("crop_path".to_string(), serde_json::json!(crop_path));
                    map.insert("frame_path".to_string(), serde_json::json!(frame_path));
                    map.insert("frame_time_s".to_string(), serde_json::json!(frame_time_s));
                }
            }
        }
    }
}

fn frame_json(frame: &FrameFact, render: Option<&RenderFacts>) -> serde_json::Value {
    let sample =
        render.and_then(|r| r.samples.iter().find(|s| (s.time_s - frame.time_s).abs() < 0.001));

    let actor_ink: serde_json::Map<String, serde_json::Value> = sample
        .map(|s| {
            s.actor_ink
                .iter()
                .map(|((scene, label), &ink)| (format!("{scene}:{label}"), serde_json::json!(ink)))
                .collect()
        })
        .unwrap_or_default();

    serde_json::json!({
        "time_s": frame.time_s,
        "scene": frame.scene,
        "cast": frame.cast,
        "ink_fraction": sample.map(|s| s.ink_fraction),
        "changed_from_prev": sample.and_then(|s| s.changed_from_prev),
        "actor_ink": actor_ink,
        "actors": frame.actors.iter().map(|a| serde_json::json!({
            "label": a.label,
            "bounds": a.bounds.map(|b| vec![b.x0, b.y0, b.x1, b.y1]),
            "opacity": a.opacity,
            "on_frame_fraction": a.on_frame_fraction,
            "draw_commands": a.draw_commands,
        })).collect::<Vec<_>>(),
    })
}

/// Run one review.
pub(crate) fn run_review(
    input: &Path,
    set: &[(String, String)],
    extensions: &CliExtensions,
    options: &ReviewOptions,
    kind: &'static str,
    brief: bool,
    evidence_dir: Option<&Path>,
) -> Result<AuditReport, String> {
    let (target, diagnostics) = try_load_and_build(input, extensions, set)?;
    if diagnostics.iter().any(|diagnostic| diagnostic.is_error()) {
        print_build_diagnostics(&diagnostics);
        let errors = diagnostics.iter().filter(|diagnostic| diagnostic.is_error()).count();
        return Err(format!("build produced {errors} error diagnostic(s)"));
    }

    let (width, height) = configured_resolution(&target).unwrap_or((1280, 720));
    let dimensions = SceneDimensions { width, height };
    let base_times = options.times.clone().unwrap_or_else(|| {
        samples::compute_sample_times(&target, &options.samples, options.max_frames)
    });
    let mut facts = collect_facts(&target, dimensions, base_times, &diagnostics)?;
    resolve_actor_source_lines(input, &mut facts);

    let _ = brief;
    let source = std::fs::read_to_string(input).unwrap_or_default();
    let mut markers = crate::review::markers::parse_markers(&source);
    let analyzer = animatix_analyzer::Analyzer::new_with_path(&source, Some(input.to_path_buf()));
    let labels: Vec<String> = facts.actors.iter().map(|a| a.label.clone()).collect();
    crate::review::markers::bind_markers(&mut markers, &labels, &analyzer);

    let render = run_render_tier(
        &target,
        dimensions,
        &facts.frames.iter().map(|frame| frame.time_s).collect::<Vec<_>>(),
        options.policy,
        options.render,
        evidence_dir.or(options.frames_dir.as_deref()),
        &facts,
    )?;

    apply_render_facts(&mut facts, &render);
    let RuleRun {
        issues: mut findings,
        skipped,
        checked,
        skipped_rules,
    } = if kind == "measure" {
        RuleRun {
            issues: Vec::new(),
            skipped: 0,
            checked: 0,
            skipped_rules: Vec::new(),
        }
    } else {
        rules::evaluate(&facts, &markers, options.policy)
    };

    let mut delta = None;
    if let Some(path) = &options.delta {
        let baseline = crate::review::delta::load_baseline(path)?;
        let d = crate::review::delta::apply(&mut findings, &baseline, &path.display().to_string());
        delta = Some(d);
    }

    let mut budget_report = None;
    if let Some(path) = &options.budget {
        if options.update_budget {
            crate::review::budget::update(
                path,
                input.to_str().unwrap_or(""),
                &findings,
                options.policy,
                options.render,
            )?;
            budget_report = Some(crate::review::budget::BudgetReport {
                path: path.to_string_lossy().into(),
                exceeded: Vec::new(),
                updated: true,
            });
        } else {
            let budget = crate::review::budget::load(path)?;
            let mut b = crate::review::budget::check(
                &budget,
                input.to_str().unwrap_or(""),
                &findings,
                options.policy,
                options.render,
            )?;
            b.path = path.display().to_string();
            budget_report = Some(b);
        }
    }

    if let Some(dir) = evidence_dir {
        write_evidence(dir, &mut findings, &render, options.candidates);
    }

    let render_status_str = match render.status {
        RenderStatus::Rendered => "complete",
        RenderStatus::Skipped => "skipped",
        RenderStatus::Failed => "failed",
    };
    let evidence_str = if evidence_dir.is_none() {
        "skipped"
    } else if render.status == RenderStatus::Rendered {
        "complete"
    } else {
        "unavailable"
    };
    let coverage = Coverage {
        structural: "complete",
        render: render_status_str,
        evidence: evidence_str,
        complete: render_status_str == "complete",
        skipped_rules,
        checks_run: checked,
        checks_skipped: skipped,
    };

    // Calculate summary
    let mut summary = Summary {
        errors: 0,
        warnings: 0,
        infos: 0,
        candidates: 0,
        waived: 0,
        skipped,
    };
    for finding in &findings {
        if finding.waived_by.is_some() {
            summary.waived += 1;
        } else {
            match finding.severity {
                Severity::Error => summary.errors += 1,
                Severity::Warning => summary.warnings += 1,
                Severity::Info => {
                    summary.infos += 1;
                    summary.candidates += 1;
                },
            }
        }
    }

    Ok(AuditReport {
        input: input.display().to_string(),
        kind,
        target: facts.target.clone(),
        render,
        sample_times_s: facts.frames.iter().map(|frame| frame.time_s).collect(),
        frames: facts.frames.clone(),
        findings,
        summary,
        skipped,
        coverage,
        delta,
        budget: budget_report,
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

#[derive(Clone, Debug)]
pub(crate) struct Summary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub candidates: usize,
    pub skipped: usize,
    pub waived: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Coverage {
    pub structural: &'static str,
    pub render: &'static str,
    pub evidence: &'static str,
    pub complete: bool,
    pub skipped_rules: Vec<String>,
    pub checks_run: usize,
    pub checks_skipped: usize,
}

/// Union of a finding's evidence bounds, in frame pixels.
///
/// Single-actor rules emit `bounds`; overlap rules emit `bounds_a` / `bounds_b`.
/// JSON serializes these as floating-point numbers, so parse through `as_f64` —
/// `as_i64` returns `None` for `230.0` and would silently drop every crop.
fn crop_bounds(evidence: &serde_json::Value) -> Option<[i32; 4]> {
    fn rect(value: &serde_json::Value) -> Option<[i32; 4]> {
        let arr = value.as_array()?;
        let num = |v: &serde_json::Value| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64));
        Some([
            num(arr.first()?)? as i32,
            num(arr.get(1)?)? as i32,
            num(arr.get(2)?)? as i32,
            num(arr.get(3)?)? as i32,
        ])
    }
    if let Some(bounds) = evidence.get("bounds") {
        return rect(bounds);
    }
    match (evidence.get("bounds_a").and_then(rect), evidence.get("bounds_b").and_then(rect)) {
        (Some(a), Some(b)) => Some([
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::facts::TargetFacts;
    use crate::review::types::{RenderFacts, RenderStatus, TargetKind};

    fn report_with_issue(severity: Severity) -> AuditReport {
        AuditReport {
            input: "demo.amx".into(),
            kind: "audit",
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
                draw_order: Vec::new(), // Added based on new FrameFact requirement
            }],
            findings: vec![RuleIssue {
                rule_id: "test.rule",
                severity,
                persistence: crate::review::types::Persistence::Held,
                scene: None,
                time_s: Some(0.0),
                actor: Some("box".into()),
                source_line: Some(1),
                message: "test".into(),
                evidence: serde_json::json!({}),
                fix_hint: None,
                waived_by: None,
                suggestion: "fix it".into(),
                baseline_state: None,
            }],
            summary: Summary {
                errors: if severity == Severity::Error { 1 } else { 0 },
                warnings: if severity == Severity::Warning { 1 } else { 0 },
                infos: if severity == Severity::Info { 1 } else { 0 },
                candidates: if severity == Severity::Info { 1 } else { 0 },
                skipped: 0,
                waived: 0,
            },
            skipped: 3,
            coverage: Coverage {
                structural: "complete",
                render: "skipped",
                evidence: "skipped",
                complete: false,
                skipped_rules: Vec::new(),
                checks_run: 10,
                checks_skipped: 3,
            },
            delta: None,
            budget: None,
        }
    }

    #[test]
    fn json_schema_has_stable_top_level_keys() {
        let report = report_with_issue(Severity::Warning);
        let json = report.to_json(Detail::Summary);
        for key in [
            "schema_version",
            "kind",
            "input",
            "target",
            "render",
            "sample_times_s",
            "frames",
            "findings",
            "summary",
            "coverage",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert_eq!(json["schema_version"], 3);
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
    fn crop_bounds_accepts_float_and_integer_rects() {
        let float = serde_json::json!({ "bounds": [230.0, 100.5, 1050.0, 620.25] });
        assert_eq!(crop_bounds(&float), Some([230, 100, 1050, 620]));
        let int = serde_json::json!({ "bounds": [10, 20, 30, 40] });
        assert_eq!(crop_bounds(&int), Some([10, 20, 30, 40]));
    }

    #[test]
    fn crop_bounds_unions_overlap_pairs() {
        let pair = serde_json::json!({
            "bounds_a": [230.0, 100.0, 1050.0, 620.0],
            "bounds_b": [700.0, 50.0, 900.0, 700.0]
        });
        assert_eq!(crop_bounds(&pair), Some([230, 50, 1050, 700]));
        let single = serde_json::json!({ "bounds_a": [1.0, 2.0, 3.0, 4.0] });
        assert_eq!(crop_bounds(&single), Some([1, 2, 3, 4]));
    }

    #[test]
    fn crop_bounds_returns_none_without_geometry() {
        let none = serde_json::json!({ "hidden_time_s": 0.5, "visible_time_s": 1.5 });
        assert_eq!(crop_bounds(&none), None);
        let malformed = serde_json::json!({ "bounds": ["x", "y"] });
        assert_eq!(crop_bounds(&malformed), None);
    }
}
