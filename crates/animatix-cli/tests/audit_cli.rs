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
    let input = temp_project(
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
    assert_eq!(json["schema_version"], 2);
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

    std::fs::remove_dir_all(input.parent().expect("temp parent")).ok();
}

#[test]
fn audit_fail_on_warning_exits_one_but_never_exits_zero() {
    let input = temp_project(
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

    std::fs::remove_dir_all(input.parent().expect("temp parent")).ok();
}

#[test]
fn audit_missing_input_is_cannot_run_exit_two() {
    let output = Command::new(target_bin())
        .args(["audit", "/definitely/not/a/scene.amx", "--render", "never"])
        .output()
        .expect("run audit");
    assert_eq!(output.status.code(), Some(2));
}
