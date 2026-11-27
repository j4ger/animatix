use std::process::Command;

mod common;

#[test]
fn invalid_scene_diagnostics_do_not_create_or_replace_visual_exports() {
    let (project, source) = common::temp_project(
        "invalid-export",
        "config { duration: 1, canvas_size: (320, 180) }\n#0s\nrect: Rect, color: \"#e74c3c\"\n",
    );
    for (subcommand, file_name) in [
        ("image", "output.png"),
        #[cfg(feature = "video")]
        ("video", "output.mp4"),
        #[cfg(feature = "video")]
        ("gif", "output.gif"),
    ] {
        let output = project.path().join(file_name);
        for existing in [false, true] {
            if existing {
                std::fs::write(&output, b"previous export").unwrap();
            }
            let result = Command::new(env!("CARGO_BIN_EXE_animatix"))
                .args(["--no-color", subcommand])
                .arg(&source)
                .arg("-o")
                .arg(&output)
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(1), "{subcommand}: {result:?}");
            let diagnostics = format!(
                "{}{}",
                String::from_utf8_lossy(&result.stderr),
                String::from_utf8_lossy(&result.stdout)
            );
            assert!(diagnostics.contains("expects a Color, got Str"), "{diagnostics}");
            if existing {
                assert_eq!(std::fs::read(&output).unwrap(), b"previous export");
            } else {
                assert!(!output.exists(), "{subcommand} created an output on build failure");
            }
        }
    }
}
