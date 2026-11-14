//! End-to-end CLI checks for `animatix audit`: JSON shape, no-GPU
//! `--render never`, and exit-code semantics.

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
fn audit_json_shape_and_default_exit_zero() {
    // `ghost` is declared before the first keyframe and never revealed, so the
    // build layer produces a never-revealed warning. Structural audit must
    // surface it and still exit 0 under the default '--fail-on error'.
    let (_project, input) = temp_project(
        "json",
        "#0s\nghost: Text, text: \"hidden\", at: (10, 10)\n#1s\nfade-in ghost [100ms]\n",
    );
    let output = Command::new(target_bin())
        .args([
            "audit",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run audit");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_stdout_json(&output);
    for key in [
        "schema_version",
        "input",
        "target",
        "render",
        "sample_times_s",
        "frames",
        "findings",
        "summary",
    ] {
        assert!(json.get(key).is_some(), "missing `{key}` in {json}");
    }
    assert_eq!(json["schema_version"], 3);
    assert_eq!(json["kind"], "audit");
    assert_eq!(json["render"]["status"], "skipped");
    assert_eq!(json["target"]["kind"], "single_scene");
    assert!(json["summary"].get("errors").is_some());
    assert!(json["summary"].get("warnings").is_some());
    assert!(json["summary"].get("infos").is_some());
    assert!(json["summary"].get("skipped").is_some());
    let frames = json["frames"].as_array().expect("frames must be an array");
    assert!(!frames.is_empty(), "audit must report measured frames");
    for frame in frames {
        for key in ["time_s", "scene", "ink_fraction", "cast"] {
            assert!(frame.get(key).is_some(), "frame missing `{key}`: {frame}");
        }
    }
}

#[test]
fn audit_fail_on_warning_exits_one_but_never_exits_zero() {
    let (_project, input) = temp_project(
        "exit",
        "#0s\nghost: Text, text: \"hidden\", at: (10, 10)\n#1s\nfade-in ghost [100ms]\n",
    );
    let path = input.to_str().unwrap();

    let strict = Command::new(target_bin())
        .args([
            "audit",
            path,
            "--format",
            "json",
            "--render",
            "never",
            "--fail-on",
            "warning",
        ])
        .output()
        .expect("run strict audit");
    assert_eq!(strict.status.code(), Some(1), "warnings must fail under --fail-on warning");

    let relaxed = Command::new(target_bin())
        .args([
            "audit",
            path,
            "--format",
            "json",
            "--render",
            "never",
            "--fail-on",
            "never",
        ])
        .output()
        .expect("run relaxed audit");
    assert_eq!(relaxed.status.code(), Some(0), "warnings must never fail under --fail-on never");
}

#[test]
fn audit_missing_input_is_cannot_run_exit_two() {
    let output = Command::new(target_bin())
        .args(["audit", "/definitely/not/a/scene.amx", "--render", "never"])
        .output()
        .expect("run audit");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn audit_brief_output_format() {
    let (_project, input) = temp_project(
        "brief",
        "#0s\nactor1: Rect, at: (0,0), size: (10,10)\nactor2: Rect, at: (0,0), size: (10,10)\n",
    );
    let output = Command::new(target_bin())
        .args([
            "audit",
            input.to_str().unwrap(),
            "--brief",
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run audit");
    let json = parse_stdout_json(&output);
    assert_eq!(json["kind"], "audit.brief");
    assert!(json.get("frames").is_none());
    let findings = json["anchors"].as_array().expect("anchors array");
    for f in findings {
        assert!(f.get("code").is_some());
        assert!(f.get("severity").is_some());
        assert!(f.get("fix").is_some());
        assert!(f.get("waived_by").is_some());
    }
}

#[test]
fn audit_fail_on_warning_for_overlap() {
    let (_project, input) = temp_project(
        "overlap",
        "#0s\nactor1: Rect, at: (0,0), size: (10,10)\nactor2: Rect, at: (0,0), size: (10,10)\n",
    );
    let output = Command::new(target_bin())
        .args([
            "audit",
            input.to_str().unwrap(),
            "--render",
            "never",
            "--fail-on",
            "warning",
        ])
        .output()
        .expect("run audit");
    assert_eq!(
        output.status.code(),
        Some(0),
        "overlaps are now Info and should not fail with --fail-on warning"
    );
}

#[test]
fn audit_marker_waiver_end_to_end() {
    let (_project, input) = temp_project(
        "waiver",
        "#0s\n// layout: allow-overlap - test\nactor1: Rect, at: (0,0), size: (10,10)\nactor2: Rect, at: (0,0), size: (10,10)\n",
    );
    let output = Command::new(target_bin())
        .args([
            "audit",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run audit");
    let json = parse_stdout_json(&output);
    let findings = json["findings"].as_array().unwrap();
    let waiver = findings
        .iter()
        .find(|f| f["code"] == "overlap.undeclared" && f["waived_by"] != serde_json::Value::Null)
        .expect("should find waived finding");
    assert_eq!(waiver["severity"], "info");
    assert!(json["summary"]["waived"].as_u64().unwrap() >= 1);
}

#[test]
fn audit_render_never_no_missing_actors() {
    let (_project, input) =
        temp_project("missing", "#0s\nactor1: Text, text: \"missing-ink\", at: (0,0)\n");
    let output = Command::new(target_bin())
        .args([
            "audit",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run audit");
    let json = parse_stdout_json(&output);
    let codes: Vec<_> = json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap())
        .collect();
    assert!(!codes.contains(&"actor.ink-missing"));
    assert!(!codes.contains(&"actor.reveal-missing"));
}

#[test]
fn audit_times_sampling() {
    let (_project, input) = temp_project(
        "times",
        "#0s\nactor1: Rect, at: (0,0), size: (10,10)\n#1s\nactor1: Rect, at: (20,20), size: (10,10)\n",
    );
    let output = Command::new(target_bin())
        .args([
            "audit",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
            "--times",
            "0,1",
        ])
        .output()
        .expect("run audit");
    let json = parse_stdout_json(&output);
    let frames: Vec<_> = json["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["time_s"].as_f64().unwrap())
        .collect();
    assert!(frames.contains(&0.0));
    assert!(frames.contains(&1.0));
}

#[test]
fn measure_json_findings_empty() {
    let (_project, input) = temp_project("empty", "#0s\nactor1: Rect, at: (0,0), size: (10,10)\n");
    let output = Command::new(target_bin())
        .args(["measure", input.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("run measure");
    let json = parse_stdout_json(&output);
    assert_eq!(json["kind"], "measure");
    assert!(json["findings"].as_array().unwrap().is_empty());
}
