use crate::review::types::{Policy, RenderMode, RuleIssue, Severity};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct Budget {
    pub schema_version: u32,
    pub policy: Policy,
    pub render: RenderMode,
    pub files: BTreeMap<String, BTreeMap<String, usize>>,
}

pub(crate) struct BudgetReport {
    pub path: String,
    pub exceeded: Vec<BudgetExceeded>,
    pub updated: bool,
}

pub(crate) struct BudgetExceeded {
    pub file: String,
    pub code: String,
    pub count: usize,
    pub allowed: usize,
}

pub(crate) fn load(path: &Path) -> Result<Budget, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read budget: {}", e))?;
    let budget: Budget =
        serde_json::from_str(&content).map_err(|e| format!("failed to parse budget: {}", e))?;
    if budget.schema_version != 3 {
        return Err(format!("budget schema_version must be 3, got {}", budget.schema_version));
    }
    Ok(budget)
}

pub(crate) fn check(
    budget: &Budget,
    file: &str,
    findings: &[RuleIssue],
    policy: Policy,
    render: RenderMode,
) -> Result<BudgetReport, String> {
    if budget.policy != policy {
        return Err(format!(
            "budget policy mismatch: expected {:?}, got {:?}",
            budget.policy, policy
        ));
    }
    if budget.render != render {
        return Err(format!(
            "budget render mode mismatch: expected {:?}, got {:?}",
            budget.render, render
        ));
    }

    let mut report = BudgetReport {
        path: "".to_string(), // Should be passed in or handled differently
        exceeded: Vec::new(),
        updated: false,
    };

    let file_budget = budget.files.get(file).cloned().unwrap_or_default();

    let mut current_counts: BTreeMap<String, usize> = BTreeMap::new();
    for f in findings {
        if f.severity == Severity::Error || f.severity == Severity::Warning {
            *current_counts.entry(f.rule_id.to_string()).or_default() += 1;
        }
    }

    for (code, count) in current_counts {
        let allowed = file_budget.get(&code).copied().unwrap_or(0);
        if count > allowed {
            report.exceeded.push(BudgetExceeded {
                file: file.to_string(),
                code,
                count,
                allowed,
            });
        }
    }

    Ok(report)
}

pub(crate) fn update(
    path: &Path,
    file: &str,
    findings: &[RuleIssue],
    policy: Policy,
    render: RenderMode,
) -> Result<(), String> {
    let mut budget = if path.exists() {
        load(path)?
    } else {
        Budget {
            schema_version: 3,
            policy,
            render,
            files: BTreeMap::new(),
        }
    };

    if budget.policy != policy || budget.render != render {
        return Err("budget policy or render mode mismatch".into());
    }

    let mut current_counts: BTreeMap<String, usize> = BTreeMap::new();
    for f in findings {
        if f.severity == Severity::Error || f.severity == Severity::Warning {
            *current_counts.entry(f.rule_id.to_string()).or_default() += 1;
        }
    }

    // Keep the committed budget readable: files with no error/warning debt are
    // simply absent rather than present with an empty map.
    if current_counts.is_empty() {
        budget.files.remove(file);
    } else {
        budget.files.insert(file.to_string(), current_counts);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(&budget).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}
