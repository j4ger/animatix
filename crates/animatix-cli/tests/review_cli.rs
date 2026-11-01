//! End-to-end CLI checks for `animatix review`: JSON shape, no-GPU
//! `--render never`, and exit-code semantics.

use std::path::PathBuf;
use std::process::Command;

fn target_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_animatix"))
}

fn temp_project(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "animatix-review-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create temp project");
    let input = dir.join("scene.amx");
    std::fs::write(&input, source).expect("write scene");
    input
}

fn parse_stdout_json(output: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8");
    serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!("stdout is not one JSON document: {error}\n--- stdout ---\n{stdout}")
    })
}

#[test]
fn review_json_shape_and_default_exit_zero() {
    // `ghost` is declared before the first keyframe and never revealed, so the
    // build layer produces a never-revealed warning. Structural review must
    // surface it and still exit 0 under the default `--fail-on error`.
    let input = temp_project(
        "json",
        "#0s\nghost: Text, text: \"hidden\", at: (10, 10)\n#1s\nfade-in ghost [100ms]\n",
    );
    let output = Command::new(target_bin())
        .args([
            "review",
            input.to_str().unwrap(),
            "--format",
            "json",
            "--render",
            "never",
        ])
        .output()
        .expect("run review");
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
        "issues",
        "summary",
    ] {
        assert!(json.get(key).is_some(), "missing `{key}` in {json}");
    }
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["render"]["status"], "skipped");
    assert_eq!(json["target"]["kind"], "single_scene");
    assert!(json["summary"].get("errors").is_some());
    assert!(json["summary"].get("warnings").is_some());
    assert!(json["summary"].get("infos").is_some());
    assert!(json["summary"].get("skipped").is_some());
    let frames = json["frames"].as_array().expect("frames must be an array");
    assert!(!frames.is_empty(), "review must report measured frames");
    for frame in frames {
        for key in ["time_s", "scene", "ink_fraction", "cast"] {
            assert!(frame.get(key).is_some(), "frame missing `{key}`: {frame}");
        }
    }

    std::fs::remove_dir_all(input.parent().expect("temp parent")).ok();
}

#[test]
fn review_fail_on_warning_exits_one_but_never_exits_zero() {
    let input = temp_project(
        "exit",
        "#0s\nghost: Text, text: \"hidden\", at: (10, 10)\n#1s\nfade-in ghost [100ms]\n",
    );
    let path = input.to_str().unwrap();

    let strict = Command::new(target_bin())
        .args([
            "review",
            path,
            "--format",
            "json",
            "--render",
            "never",
            "--fail-on",
            "warning",
        ])
        .output()
        .expect("run strict review");
    assert_eq!(strict.status.code(), Some(1), "warnings must fail under --fail-on warning");

    let relaxed = Command::new(target_bin())
        .args([
            "review",
            path,
            "--format",
            "json",
            "--render",
            "never",
            "--fail-on",
            "never",
        ])
        .output()
        .expect("run relaxed review");
    assert_eq!(relaxed.status.code(), Some(0), "warnings must never fail under --fail-on never");

    std::fs::remove_dir_all(input.parent().expect("temp parent")).ok();
}

#[test]
fn review_missing_input_is_cannot_run_exit_two() {
    let output = Command::new(target_bin())
        .args(["review", "/definitely/not/a/scene.amx", "--render", "never"])
        .output()
        .expect("run review");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn review_emit_verify_refuses_to_overwrite() {
    let input = temp_project(
        "emit",
        "#0s\nbox: Rect, at: (10, 10), size: (20, 20)\n#1s\nfade-in box [100ms]\n",
    );
    let dir = input.parent().expect("temp parent");
    let verify_path = dir.join("suggested.verify.txt");
    std::fs::write(&verify_path, "ink 0.0 0.0001\n").expect("seed verify file");
    let output = Command::new(target_bin())
        .args([
            "review",
            input.to_str().unwrap(),
            "--render",
            "never",
            "--emit-verify",
            verify_path.to_str().unwrap(),
        ])
        .output()
        .expect("run review");
    assert_eq!(output.status.code(), Some(2), "--emit-verify must not silently overwrite");
    assert_eq!(std::fs::read_to_string(&verify_path).unwrap(), "ink 0.0 0.0001\n");
    std::fs::remove_dir_all(dir).ok();
}
