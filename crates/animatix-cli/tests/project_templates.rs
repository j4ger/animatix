//! Exercise installed starter sources independently of the repository layout.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_animatix"))
        .current_dir(root)
        .args(args)
        .output()
        .expect("launch animatix")
}

#[test]
fn starters_build_and_the_math_layout_reflows_for_portrait() {
    let root = tempfile::tempdir().expect("temporary starter directory");
    for template in ["narrated", "math-explainer", "metric-brief"] {
        let init = run(root.path(), &["init", "--template", template, template]);
        assert!(init.status.success(), "{init:?}");
        assert!(root.path().join(template).join("layout.amx").is_file());
        let path = format!("{template}/main.amx");
        let check = run(root.path(), &["check", &path, "--format", "json"]);
        assert!(check.status.success(), "{check:?}");
        let report: Value = serde_json::from_slice(&check.stdout).unwrap();
        assert_eq!(report["passed"], true, "{template}: {report}");
        assert!(
            report["errors"]
                .as_array()
                .is_none_or(|errors| errors.iter().all(|d| d["severity"] == "hint")),
            "{template}: {report}"
        );
    }

    let math = root.path().join("math-explainer/main.amx");
    let source = std::fs::read_to_string(&math).unwrap();
    std::fs::write(&math, source.replace("(1280, 720)", "(720, 1280)")).unwrap();
    let measure = run(
        root.path(),
        &[
            "measure",
            "math-explainer/main.amx",
            "--times",
            "4",
            "--render",
            "never",
            "--display-width",
            "360",
        ],
    );
    assert!(measure.status.success(), "{measure:?}");
    let report: Value = serde_json::from_slice(&measure.stdout).unwrap();
    assert_eq!(report["target"]["resolution"], serde_json::json!([720, 1280]));
    let actors = report["frames"][0]["actors"].as_array().unwrap();
    for label in [
        "chart",
        "expression",
        "slide.head.heading_ink",
        "slide.conclusion",
    ] {
        let actor = actors.iter().find(|a| a["label"] == label).unwrap();
        assert_eq!(actor["on_frame_fraction"], 1.0, "{label}: {actor}");
        assert!(actor["draw_commands"].as_u64().unwrap_or(0) > 0, "{label}: {actor}");
    }

    std::fs::write(&math, source.replace("@figure", "@figuer")).unwrap();
    let check = run(root.path(), &["check", "math-explainer/main.amx", "--format", "json"]);
    assert!(!check.status.success());
    let report: Value = serde_json::from_slice(&check.stdout).unwrap();
    assert!(
        report["errors"].as_array().unwrap().iter().any(|d| {
            d["code"] == "invalid-component-slot"
                && d["message"].as_str().unwrap_or_default().contains("@figuer")
        }),
        "{report}"
    );
}
