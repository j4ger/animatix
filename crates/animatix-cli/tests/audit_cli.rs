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
fn check_rejects_removed_and_invalid_canvas_declarations() {
    for (index, key) in [
        "resolution: (1280, 720)",
        "canvas_size: (0.5, 720)",
        "canvas_size: (100%, 720)",
        "canvas_size: (W, 720)",
    ]
    .into_iter()
    .enumerate()
    {
        let (_project, input) = temp_project(
            &format!("canvas-{index}"),
            &format!("config {{ {key} }}\n#0s\nbox: Rect, size: (40, 40)\n"),
        );
        let output = Command::new(target_bin())
            .args(["check", input.to_str().unwrap(), "--format", "json"])
            .output()
            .expect("run check");
        let report = parse_stdout_json(&output);
        assert_eq!(output.status.code(), Some(1), "{key}: {report}");
        assert_eq!(report["passed"], false);
        assert!(report["errors"].as_array().unwrap().iter().any(|error| {
            error["message"].as_str().is_some_and(|message| message.contains("canvas_size"))
        }));
    }
}

#[test]
fn audit_json_shape_and_default_exit_zero() {
    // `ghost` is declared before the first keyframe and never revealed, so the
    // build layer produces a never-revealed warning. Structural audit must
    // surface it and still exit 0 under the default '--fail-on error'.
    let (_project, input) = temp_project(
        "json",
        "#0s\nghost: Text, text: \"hidden\", at: (10, 10)\n#1s\nfade_in(target: ghost, duration: 100ms)\n",
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
    assert_eq!(json["schema_version"], 4);
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
fn site_audit_renders_rest_probes_between_report_samples() {
    if animatix_render::offscreen::OffscreenRenderer::new().is_err() {
        animatix_render::testing::skip_if_no_gpu();
        return;
    }

    let (project, input) = temp_project(
        "site-rest-probes",
        "config { canvas_size: (160, 90), duration: 4 }\n#0s\nbox: Rect, at: (80, 45), size: (60, 40)\n",
    );
    let frames = project.path().join("frames");
    let output = Command::new(target_bin())
        .arg("audit")
        .arg(input)
        .args([
            "--policy",
            "site",
            "--render",
            "always",
            "--times",
            "0,4",
            "--format",
            "json",
            "--fail-on",
            "never",
            "--frames-dir",
        ])
        .arg(&frames)
        .output()
        .expect("run site audit");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report = parse_stdout_json(&output);
    assert_eq!(report["render"]["status"], "rendered");
    assert_eq!(report["sample_times_s"], serde_json::json!([0.0, 4.0]));
    assert!(frames.join("review_0001_2.00s.png").is_file());
    assert!(frames.join("review_0002_3.95s.png").is_file());
}

#[test]
fn audit_fail_on_warning_exits_one_but_never_exits_zero() {
    let (_project, input) = temp_project(
        "exit",
        "#0s\nghost: Text, text: \"hidden\", at: (10, 10)\n#1s\nfade_in(target: ghost, duration: 100ms)\n",
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

#[test]
fn display_size_is_a_measurement_and_a_separate_audit_candidate() {
    let (_project, input) = temp_project(
        "display-text",
        r#"
config { canvas_size: (1280, 720), duration: 1 }
#0s
small: Text, text: "x = 2", at: (400, 350), font_size: 16, role: "note"
large: Text, text: "Readable", at: (800, 350), font_size: 54
"#,
    );
    let run = |verb| {
        let output = Command::new(target_bin())
            .arg(verb)
            .arg(&input)
            .args([
                "--times",
                "0.5",
                "--display-width",
                "360",
                "--format",
                "json",
                "--render",
                "never",
            ])
            .output()
            .expect("review command should run");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        parse_stdout_json(&output)
    };

    let measure = run("measure");
    assert!(measure["findings"].as_array().unwrap().is_empty());
    let samples = measure["frames"][0]["actors"].as_array().unwrap();
    let small = samples.iter().find(|actor| actor["label"] == "small").unwrap();
    let measured = &small["text_at_display"];
    assert_eq!(measured["width_px"], 360);
    let measured_height = measured["bounds_height_px"].as_f64().unwrap();
    assert_eq!(measured["role"], "note");
    assert!(measured_height > 0.0 && measured_height < 6.0);

    let audit = run("audit");
    let candidates: Vec<_> = audit["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|finding| finding["code"] == "text.small-at-display")
        .collect();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["actor"], "small");
    assert_eq!(candidates[0]["time_s"], 0.5);
    assert_eq!(candidates[0]["severity"], "info");
    assert_eq!(candidates[0]["evidence"]["min_text_height_px"], 6.0);
    assert_eq!(candidates[0]["evidence"]["bounds_height_px"], measured["bounds_height_px"]);
}

fn run_paint_audit(name: &str, source: &str, times: &str) -> serde_json::Value {
    let (_project, input) = temp_project(name, source);
    let output = Command::new(target_bin())
        .arg("audit")
        .arg(input)
        .args([
            "--render",
            "always",
            "--format",
            "json",
            "--fail-on",
            "never",
            "--times",
            times,
        ])
        .output()
        .expect("run rendered audit");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report = parse_stdout_json(&output);
    assert_eq!(report["render"]["status"], "rendered");
    report
}

fn paint_finding(report: &serde_json::Value, code: &str, actor: &str) -> bool {
    report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["code"] == code && f["actor"] == actor)
}

#[test]
fn audit_reveal_on_decorated_panel_uses_the_actors_own_paint() {
    if animatix_render::offscreen::OffscreenRenderer::new().is_err() {
        animatix_render::testing::skip_if_no_gpu();
        return;
    }
    let front = r#"
config { canvas_size: (480,270), duration: 2 }
#0s
scene.background_color = hex_color("1a1e29")
card: Rect, at: (240,135), size: (328,184), color: hex_color("35536a")
dot: Ellipse, at: (240,135), size: (88,88), color: hex_color("f2ba30"), opacity: 0
#1s
dot.opacity = 1
#2s
"#;
    let report = run_paint_audit("reveal-on-panel", front, "0,0.999,1,2");
    assert!(!paint_finding(&report, "actor.reveal-missing", "dot"), "{report}");
    let card = front.lines().find(|l| l.starts_with("card:")).unwrap();
    let behind = front.replace(&format!("{card}\n"), "").replace("#1s", &format!("{card}\n#1s"));
    let report = run_paint_audit("reveal-behind-panel", &behind, "0,0.999,1,2");
    assert!(paint_finding(&report, "actor.reveal-missing", "dot"), "{report}");
}

#[test]
fn audit_text_detects_real_occlusion_even_when_the_cover_matches_the_background() {
    if animatix_render::offscreen::OffscreenRenderer::new().is_err() {
        animatix_render::testing::skip_if_no_gpu();
        return;
    }
    let source = r#"
config { canvas_size: (480,270), duration: 2 }
#0s
scene.background_color = hex_color("151c2b")
station: Text, at: (240,135), text: "CENTRAL", font_size: 35, color: hex_color("e9d4a8")
binder: Rect, at: (240,135), size: (200,70), color: hex_color("d45858"), opacity: 0
#1s
binder.opacity = 1
#2s
"#;
    for color in ["d45858", "151c2b"] {
        let report = run_paint_audit(color, &source.replace("d45858", color), "0,0.999,1,2");
        assert!(paint_finding(&report, "text.occluded-full", "station"), "{report}");
        assert!(!paint_finding(&report, "actor.reveal-missing", "binder"), "{report}");
    }
    let no_cover = source
        .lines()
        .filter(|line| !line.starts_with("binder"))
        .collect::<Vec<_>>()
        .join("\n");
    let report = run_paint_audit("no-cover", &no_cover, "0,0.999,1,2");
    assert!(!paint_finding(&report, "text.occluded-full", "station"), "{report}");
}

#[test]
fn audit_occlusion_measures_the_actual_transition_not_the_raw_scene_stack() {
    if animatix_render::offscreen::OffscreenRenderer::new().is_err() {
        animatix_render::testing::skip_if_no_gpu();
        return;
    }
    let start = r#"
config { canvas_size: (480,270) }
#Overview
config { duration: 2 }
scene.background_color = hex_color("151c2b")
#0s
old_station: Text, at: (240,135), text: "CENTRAL", font_size: 35, color: hex_color("e9d4a8"), shared_id: "station"
#1s
play Detail [fade, 1s]
#Detail
config { duration: 2 }
scene.background_color = hex_color("151c2b")
#0s
"#;
    let shared = format!(
        r#"{start}new_station: Text, at: (240,135), text: "CENTRAL", font_size: 35, color: hex_color("e9d4a8"), shared_id: "station"
"#
    );
    let report = run_paint_audit("one-shared-text", &shared, "0,0.999,1,1.5,1.999,2,3");
    assert!(!paint_finding(&report, "text.occluded-full", "old_station"), "{report}");
    let rect = format!(
        r#"{start}binder: Rect, at: (240,135), size: (200,70), color: hex_color("d45858")
"#
    );
    let report = run_paint_audit("transparent-incoming", &rect, "0,0.999,1");
    assert!(!paint_finding(&report, "text.occluded-full", "old_station"), "{report}");
    let real = rect.replace(
        "#1s\nplay",
        "blocker: Rect, at: (240,135), size: (200,70), color: hex_color(\"151c2b\")\n#1s\nplay",
    );
    let report = run_paint_audit("real-outgoing-cover", &real, "1,1.3");
    assert!(paint_finding(&report, "text.occluded-full", "old_station"), "{report}");
}
