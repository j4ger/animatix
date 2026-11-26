mod common;

use std::process::Command;

#[test]
fn still_image_keeps_the_same_boundary_as_storyboard() {
    if animatix_render::offscreen::OffscreenRenderer::new().is_err() {
        assert!(std::env::var_os("ANIMATIX_REQUIRE_GPU").is_none(), "GPU required");
        return;
    }

    let actor = "#0s\nsignal: Rect, at: (32,32), size: (40,40), color: hex_color(\"324354\")";
    for (scenes, event, boundary) in [
        ("config { duration: 9 }", "#8.04s", 8.04_f64),
        ("# Lead\nconfig { duration: 1 }\n# Body\nconfig { duration: 2 }", "#880ms", 1.88),
        (
            "# Lead\nconfig { duration: 0.0005 }\n# Body\nconfig { duration: 2 }",
            "#5ms",
            0.0055,
        ),
    ] {
        let source = format!(
            "config {{ canvas_size: (64,64) }}\n{scenes}\n{actor}\n{event}\nsignal.color = hex_color(\"5cdeab\")"
        );
        let (directory, input) = common::temp_project("exact-still", &source);
        let output = directory.path().join("out.png");
        let run = |args: &[&str]| {
            let result = Command::new(env!("CARGO_BIN_EXE_animatix"))
                .args(args)
                .arg(&input)
                .arg("-o")
                .arg(&output)
                .output()
                .unwrap();
            assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
            image::open(&output).unwrap().to_rgba8()
        };
        let old = [50, 67, 84, 255];
        let new = [92, 222, 171, 255];
        let times = [boundary.next_down(), boundary, boundary.next_up()].map(|t| t.to_string());
        for (time, expected) in times.iter().zip([old, new, new]) {
            assert_eq!(run(&["image", "--time", time]).get_pixel(32, 32).0, expected);
        }
        let sheet = run(&[
            "storyboard",
            "--times",
            &times.join(","),
            "--cols",
            "3",
            "--tile-width",
            "64",
            "--tile-height",
            "64",
            "--max-width",
            "196",
        ]);
        let cell_width = sheet.width() / 3;
        for (i, expected) in [old, new, new].into_iter().enumerate() {
            assert_eq!(
                sheet.get_pixel(cell_width * i as u32 + cell_width / 2, sheet.height() - 48).0,
                expected
            );
        }
    }
}

#[test]
fn an_explicit_review_preserves_sub_millisecond_samples_and_local_state() {
    let source = r#"
config { canvas_size: (64,64) }
# Lead
config { duration: 1 }
#0s
lead: Rect
# Body
config { duration: 2 }
#0s
signal: Rect
#880ms
signal.opacity = 0
"#;
    let (_directory, input) = common::temp_project("exact-review", source);
    let boundary = 1.88_f64;
    let times = [boundary.next_down(), boundary, boundary.next_up()]
        .map(|t| t.to_string())
        .join(",");
    let result = Command::new(env!("CARGO_BIN_EXE_animatix"))
        .args(["measure", "--render", "never", "--times", &times])
        .arg(input)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let frames = report["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 3);
    for (frame, opacity) in frames.iter().zip([1.0, 0.0, 0.0]) {
        let signal = frame["actors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|actor| actor["label"] == "signal")
            .unwrap();
        assert_eq!(signal["opacity"], opacity);
    }
}

#[cfg(feature = "video")]
#[test]
fn gif_uses_the_export_grid_without_accumulating_delay_error() {
    use image::AnimationDecoder;

    if animatix_render::offscreen::OffscreenRenderer::new().is_err() {
        assert!(std::env::var_os("ANIMATIX_REQUIRE_GPU").is_none(), "GPU required");
        return;
    }
    let (directory, input) = common::temp_project(
        "exact-gif",
        concat!(
            "config { canvas_size: (64,64), duration: 2 }\n#0s\n",
            "signal: Rect, at: (32,32), size: (40,40), color: hex_color(\"324354\")\n",
            "#1s\nsignal.color = hex_color(\"5cdeab\")",
        ),
    );
    let output = directory.path().join("out.gif");
    for fps in [24, 25, 30] {
        let result = Command::new(env!("CARGO_BIN_EXE_animatix"))
            .args([
                "gif",
                "--duration",
                "2",
                "--fps",
                &fps.to_string(),
                "-j",
                "1",
            ])
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let file = std::io::BufReader::new(std::fs::File::open(&output).unwrap());
        let frames = image::codecs::gif::GifDecoder::new(file)
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        assert_eq!(frames.len(), 2 * fps);
        let mut elapsed = 0;
        for (i, frame) in frames.iter().enumerate() {
            let (ms, denominator) = frame.delay().numer_denom_ms();
            assert_eq!(denominator, 1);
            assert_eq!(frame.buffer().get_pixel(32, 32).0[1] > 150, i >= fps);
            elapsed += ms;
            assert!((f64::from(elapsed) - 1000.0 * (i + 1) as f64 / fps as f64).abs() <= 5.0);
        }
        assert_eq!(elapsed, 2000);
    }
}
