//! End-to-end CLI checks for the zero-config `animatix verify` mode.
//!
//! These tests deliberately use `--render never`: the structural tier is the
//! contract LLM-generated scenes must satisfy on any machine, while pixel
//! rules are unit-tested against synthetic facts in `review::rules`.

use std::path::PathBuf;
use std::process::Command;

mod common;
use common::temp_project;

fn target_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_animatix"))
}

fn parse_stdout_json(output: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8");
    serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!("stdout is not one JSON document: {error}\n--- stdout ---\n{stdout}")
    })
}

#[test]
fn verify_auto_json_shape_without_sidecar() {
    let input = temp_project(
        "good",
        "#0s\nbox: Rect, at: (100, 100), size: (100, 100)\n#1s\nbox.opacity = 0.5 [500ms]\n",
    );
    let output = Command::new(target_bin())
        .args([
            "verify",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run verify");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout_json(&output);
    assert_eq!(json["kind"], "verify");
    assert_eq!(json["mode"], "auto");
    assert_eq!(json["verdict"], "pass");
    assert_eq!(json["render"]["status"], "skipped");
    assert_eq!(json["coverage"]["complete"], false);
    assert_eq!(json["target"]["kind"], "single_scene");
    assert!(json["issues"].as_array().is_some());
    assert!(json["summary"]["checks_skipped"].as_u64().unwrap() > 0);
}

#[test]
fn verify_auto_reports_never_revealed_actor_with_source_evidence() {
    let input = temp_project(
        "hidden",
        "ghost: Text, text: \"hidden\", at: (100, 100)\n#1s\nbox: Rect, at: (100, 200), size: (100, 100)\n",
    );
    let output = Command::new(target_bin())
        .args([
            "verify",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run verify");
    assert_eq!(output.status.code(), Some(1));
    let json = parse_stdout_json(&output);
    assert_eq!(json["kind"], "verify");
    assert_eq!(json["verdict"], "fail");
    let issues = json["issues"].as_array().expect("issues");
    assert!(issues.iter().any(|issue| {
        issue["rule_id"] == "build.never-revealed"
            && issue["actor"] == "ghost"
            && issue["fix"]["kind"] == "add-entrance"
    }));
}

#[test]
fn verify_auto_reports_empty_scene_as_error() {
    let input = temp_project("empty", "#0s\n#1s\n");
    let output = Command::new(target_bin())
        .args([
            "verify",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run verify");
    assert_eq!(output.status.code(), Some(1));
    let json = parse_stdout_json(&output);
    assert_eq!(json["verdict"], "fail");
    assert!(
        json["issues"]
            .as_array()
            .expect("issues")
            .iter()
            .any(|issue| issue["rule_id"] == "scene.duration")
    );
}

#[test]
fn verify_auto_max_issues_never_drops_errors() {
    let input = temp_project(
        "max",
        "ghost: Text, text: \"hidden\", at: (100, 100)\n#1s\nbox: Rect, at: (100, 200), size: (100, 100)\n",
    );
    let output = Command::new(target_bin())
        .args([
            "verify",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
            "--max-issues",
            "1",
        ])
        .output()
        .expect("run verify");
    assert_eq!(output.status.code(), Some(1));
    let json = parse_stdout_json(&output);
    let issues = json["issues"].as_array().expect("issues");
    assert!(issues.iter().all(|issue| issue["severity"] == "error"));
    assert_eq!(json["summary"]["errors"], issues.len());
}

#[test]
fn verify_auto_brief_is_compact_json() {
    let input = temp_project(
        "brief",
        "ghost: Text, text: \"hidden\", at: (100, 100)\n#1s\nbox: Rect, at: (100, 200), size: (100, 100)\n",
    );
    let output = Command::new(target_bin())
        .args([
            "verify",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--brief",
            "--render",
            "never",
        ])
        .output()
        .expect("run verify");
    assert_eq!(output.status.code(), Some(1));
    let json = parse_stdout_json(&output);
    assert_eq!(json["kind"], "verify.brief");
    assert_eq!(json["verdict"], "fail");
    assert!(json["issues"][0]["fix"]["kind"].is_string());
    assert!(json.get("target").is_none());
}

#[test]
fn verify_auto_flag_ignores_existing_sidecar() {
    let input = temp_project(
        "force-auto",
        "#0s\nbox: Rect, at: (100, 100), size: (100, 100)\n#1s\nbox.opacity = 0.5 [500ms]\n",
    );
    let checks = input.parent().unwrap().join("verify.txt");
    std::fs::write(&checks, "not a legacy check\n").expect("write checks");
    let output = Command::new(target_bin())
        .args([
            "verify",
            input.to_str().unwrap(),
            "--auto",
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run verify");
    assert_eq!(output.status.code(), Some(0));
    let json = parse_stdout_json(&output);
    assert_eq!(json["mode"], "auto");
    assert_eq!(json["verdict"], "pass");
}

#[test]
fn verify_auto_build_error_is_one_json_document() {
    let input = temp_project("broken", "box: Rect, size: (10, 10)\n#1s\nfade-in\n");
    let output = Command::new(target_bin())
        .args([
            "verify",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run verify");
    assert_eq!(output.status.code(), Some(1));
    let json = parse_stdout_json(&output);
    assert_eq!(json["kind"], "verify");
    assert_eq!(json["verdict"], "fail");
    assert_eq!(json["issues"][0]["rule_id"], "build.error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "build diagnostics must not pollute JSON stdout: {stderr}");
}

#[test]
fn verify_auto_honours_explicit_checks_precedence() {
    let input = temp_project(
        "precedence",
        "#0s\nbox: Rect, at: (100, 100), size: (100, 100)\n#1s\nbox.opacity = 0.5 [500ms]\n",
    );
    let checks = input.parent().unwrap().join("verify.txt");
    std::fs::write(&checks, "nonsense check\n").expect("write checks");
    let output = Command::new(target_bin())
        .args([
            "verify",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run verify");
    // A malformed sidecar must fail in the legacy parser before any renderer
    // is created; auto mode would ignore it and pass. This pins the precedence.
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("unknown check kind"), "stderr: {text}");
}
