use crate::review::types::RuleIssue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct BaselineFinding {
    pub code: String,
    pub scene: Option<String>,
    pub actor: Option<String>,
    pub source_line: Option<usize>,
    pub time_s: Option<f64>,
    pub message: String,
}

pub(crate) struct DeltaReport {
    pub baseline: String,
    pub new: usize,
    pub resolved: usize,
    pub unchanged: usize,
    pub new_errors: usize,
    pub new_warnings: usize,
}

fn finding_key(
    rule_id: &str,
    scene: &Option<String>,
    actor: &Option<String>,
    source_line: &Option<usize>,
    time_s: &Option<f64>,
    message: &str,
) -> String {
    let time_key = time_s
        .map(|t| (t * 100.0).round() as i64)
        .map(|t| t.to_string())
        .unwrap_or_else(|| "none".to_string());
    let scene_key = scene.as_deref().unwrap_or("none");
    let actor_key = actor.as_deref().unwrap_or("none");
    let line_key = source_line.map(|l| l.to_string()).unwrap_or_else(|| "none".to_string());

    if time_s.is_some() {
        format!("{}|{}|{}|{}|{}", rule_id, scene_key, actor_key, line_key, time_key)
    } else {
        format!("{}|{}|{}|{}|{}", rule_id, scene_key, actor_key, line_key, message)
    }
}

pub(crate) fn load_baseline(path: &Path) -> Result<Vec<BaselineFinding>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read baseline: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("failed to parse baseline: {}", e))?;
    let kind = json["kind"].as_str().unwrap_or_default();
    if !matches!(kind, "audit" | "audit.brief") {
        return Err(format!("baseline must be an audit or audit.brief report, got kind '{kind}'"));
    }

    let mut findings = Vec::new();
    if let Some(f_list) = json["findings"].as_array() {
        for f in f_list {
            findings.push(BaselineFinding {
                code: f["code"].as_str().unwrap_or_default().to_string(),
                scene: f["scene"].as_str().map(|s| s.to_string()),
                actor: f["actor"].as_str().map(|s| s.to_string()),
                source_line: f["source_line"].as_u64().map(|l| l as usize),
                time_s: f["time_s"].as_f64(),
                message: f["message"].as_str().unwrap_or_default().to_string(),
            });
        }
    } else if let Some(a_list) = json["anchors"].as_array() {
        for a in a_list {
            findings.push(BaselineFinding {
                code: a["code"].as_str().unwrap_or_default().to_string(),
                scene: a["scene"].as_str().map(|s| s.to_string()),
                actor: a["actor"].as_str().map(|s| s.to_string()),
                source_line: a["source_line"].as_u64().map(|l| l as usize),
                time_s: a["time_s"].as_f64(),
                message: a["message"].as_str().unwrap_or_default().to_string(),
            });
        }
    } else {
        return Err("baseline must have 'findings' or 'anchors'".into());
    }

    Ok(findings)
}

pub(crate) fn apply(
    findings: &mut [RuleIssue],
    baseline: &[BaselineFinding],
    baseline_path: &str,
) -> DeltaReport {
    let mut baseline_map: BTreeMap<String, bool> = baseline
        .iter()
        .map(|f| {
            (
                finding_key(&f.code, &f.scene, &f.actor, &f.source_line, &f.time_s, &f.message),
                false,
            )
        })
        .collect();

    let mut report = DeltaReport {
        baseline: baseline_path.to_string(),
        new: 0,
        resolved: 0,
        unchanged: 0,
        new_errors: 0,
        new_warnings: 0,
    };

    for f in findings.iter_mut() {
        let key = finding_key(f.rule_id, &f.scene, &f.actor, &f.source_line, &f.time_s, &f.message);
        if let Some(seen) = baseline_map.get_mut(&key) {
            *seen = true;
            report.unchanged += 1;
            f.baseline_state = Some("unchanged".to_string());
        } else {
            report.new += 1;
            f.baseline_state = Some("new".to_string());
            if f.severity == crate::review::types::Severity::Error {
                report.new_errors += 1;
            } else if f.severity == crate::review::types::Severity::Warning {
                report.new_warnings += 1;
            }
        }
    }

    report.resolved = baseline_map.values().filter(|seen| !**seen).count();

    report
}
