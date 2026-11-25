use std::process::Command;

#[test]
fn an_empty_time_header_is_the_base_of_a_relative_keyframe() {
    let directory = tempfile::tempdir().unwrap();
    let scene = directory.path().join("relative.amx");
    std::fs::write(
        &scene,
        r#"
        config { duration: 4 }
        #0s
          title: Text, text: "Start"
        #1.9s
        #+250ms
          title.opacity = 0
    "#,
    )
    .unwrap();
    // The assignment is under the relative keyframe. The measured opacity
    // changes only after both headers, even through module expansion.
    let result = Command::new(env!("CARGO_BIN_EXE_animatix"))
        .args(["measure", "--render", "never", "--times", "1.99,2.25"])
        .arg(scene)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let opacity = |index: usize| {
        report["frames"][index]["actors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|actor| actor["label"] == "title")
            .unwrap()["opacity"]
            .as_f64()
            .unwrap()
    };
    assert_eq!([opacity(0), opacity(1)], [1.0, 0.0]);
}
