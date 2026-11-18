use std::process::Command;

#[cfg(feature = "video")]
mod common;

#[cfg(feature = "video")]
#[test]
fn chapter_selection_reports_unknown_and_empty_scenes_before_replacing_the_output() {
    let (project, input) = common::temp_project(
        "video-chapter",
        "# First\nconfig { duration: 1 }\n# Second\nconfig { duration: 1 }\n",
    );
    let file = project.path().join("output.mp4");
    std::fs::write(&file, b"previous export").unwrap();

    for (scene, message) in [
        ("Missing", "Unknown scene 'Missing'. Available scenes are First, Second."),
        ("Second", "Scene 'Second' has no frames"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_animatix"))
            .args(["--no-color", "video"])
            .arg(&input)
            .args(["--scene", scene, "--duration", "0.5", "-o"])
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(message), "{stderr}");
        assert_eq!(std::fs::read(&file).unwrap(), b"previous export");
    }
}

#[cfg(not(feature = "video"))]
#[test]
fn video_commands_point_to_the_optional_feature_in_default_builds() {
    let feature_hint = "optional 'video' feature";
    for command in ["video", "gif", "audio", "chapter-plan"] {
        let result = Command::new(env!("CARGO_BIN_EXE_animatix"))
            .args(["--no-color", command, "example.amx"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(feature_hint), "{stderr}");
        assert!(
            stderr.contains("nix develop --command cargo build -p animatix-cli --features video"),
            "{stderr}"
        );
    }

    for args in [&["vido"][..], &["plugin", "video"][..]] {
        let result = Command::new(env!("CARGO_BIN_EXE_animatix")).args(args).output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("unrecognized subcommand"), "{stderr}");
        assert!(!stderr.contains(feature_hint), "{stderr}");
    }
}
