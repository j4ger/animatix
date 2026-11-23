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
            "4.8",
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
    for label in ["chart", "expression", "head.heading_ink", "conclusion"] {
        let actor = actors.iter().find(|a| a["label"] == label).unwrap();
        assert_eq!(actor["on_frame_fraction"], 1.0, "{label}: {actor}");
        assert!(actor["draw_commands"].as_u64().unwrap_or(0) > 0, "{label}: {actor}");
    }

    std::fs::write(&math, source.replace("@content", "@contnet")).unwrap();
    let check = run(root.path(), &["check", "math-explainer/main.amx", "--format", "json"]);
    assert!(!check.status.success());
    let report: Value = serde_json::from_slice(&check.stdout).unwrap();
    assert!(
        report["errors"].as_array().unwrap().iter().any(|d| {
            d["code"] == "invalid-component-slot"
                && d["message"].as_str().unwrap_or_default().contains("@contnet")
        }),
        "{report}"
    );
}

#[test]
fn explainer_template_orders_entrances_and_can_open_static_in_portrait() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("layout.amx"),
        include_str!("../../../web/content/templates/layout.amx"),
    )
    .unwrap();
    let source = r#"
import "./layout.amx"
config { resolution: (720, 1280), duration: 3, style: "editorial" }
let motion = "subtle"
#0s
panel: ExplainerLayout, eyebrow: "EXPLAINER",
  heading: "A longer title that wraps across two lines in this portrait template",
  takeaway: "The conclusion appears after the figure.", with_notes: false
panel.enter(motion: motion, defer_takeaway: true)
#1s
panel.reveal_takeaway(motion: motion)
"#;
    std::fs::write(root.path().join("scene.amx"), source).unwrap();
    let frames = |set: &str| {
        let output = run(
            root.path(),
            &[
                "measure",
                "scene.amx",
                "--times",
                "0,0.09,0.4,1.3,2.9",
                "--render",
                "never",
                "--set",
                set,
            ],
        );
        assert!(output.status.success(), "{output:?}");
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        report["frames"].as_array().unwrap().clone()
    };
    let opacity = |frame: &Value, label: &str| {
        frame["actors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|actor| actor["label"] == label)
            .unwrap()["opacity"]
            .as_f64()
            .unwrap()
    };
    let animated = frames("motion=\"subtle\"");
    let title = "panel.head.heading_ink";
    let plate = "panel.figure_plate";
    let takeaway = "panel.conclusion";
    assert_eq!(opacity(&animated[0], title), 0.0);
    assert!(opacity(&animated[1], title) > 0.0);
    assert_eq!(opacity(&animated[1], plate), 0.0);
    assert_eq!(opacity(&animated[2], plate), 1.0);
    assert_eq!(opacity(&animated[2], takeaway), 0.0);
    assert_eq!(opacity(&animated[3], takeaway), 1.0);
    assert_eq!(opacity(&animated[4], title), 1.0);
    let still = frames("motion=\"none\"");
    for label in [title, plate, takeaway] {
        assert_eq!(opacity(&still[0], label), 1.0);
    }
}
