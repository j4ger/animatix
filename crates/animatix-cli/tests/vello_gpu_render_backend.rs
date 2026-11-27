use std::process::Command;

use animatix_render::offscreen::OffscreenRenderer;

mod common;

#[test]
fn image_command_renders_with_vello_gpu_by_default() {
    if OffscreenRenderer::new().is_err() {
        animatix_render::testing::skip_if_no_gpu();
        return;
    }

    let (project, input) = common::temp_project(
        "vello-gpu-image",
        "#0s\nr: Rect, size: (64, 64), at: (32, 32), color: (1, 0, 0, 1)\n",
    );
    let output = project.path().join("candidate.png");
    let result = Command::new(env!("CARGO_BIN_EXE_animatix"))
        .args(["--no-color", "image"])
        .arg(input)
        .args(["--time", "0", "-o"])
        .arg(&output)
        .output()
        .expect("run candidate image export");
    assert!(
        result.status.success(),
        "candidate image command failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let image = image::open(&output).expect("candidate PNG").to_rgba8();
    let center = image.get_pixel(32, 32).0;
    assert!(center[0] > 200 && center[1] < 80 && center[2] < 80, "center: {center:?}");
    assert_eq!(center[3], 255);
}

#[test]
fn image_command_grayscale_converts_a_composited_transition_frame() {
    if OffscreenRenderer::new().is_err() {
        animatix_render::testing::skip_if_no_gpu();
        return;
    }

    let source = r##"
config { canvas_size: (32, 32) }

# From
config { duration: 1 }
#0s
red: Rect, size: (32, 32), at: (16, 16), color: (1, 0, 0, 1)
#1s
play To [fade, 500ms]

# To
config { duration: 1 }
#0s
blue: Rect, size: (32, 32), at: (16, 16), color: (0, 0, 1, 1)
"##;
    let (project, input) = common::temp_project("grayscale-transition", source);
    let color_output = project.path().join("color.png");
    let gray_output = project.path().join("gray.png");

    for (output, grayscale) in [(&color_output, false), (&gray_output, true)] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_animatix"));
        command
            .args(["--no-color", "image"])
            .arg(&input)
            .args(["--time", "0.75", "-o"])
            .arg(output);
        if grayscale {
            command.arg("--grayscale");
        }
        let result = command.output().expect("run transition image export");
        assert!(
            result.status.success(),
            "transition export failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    let color = image::open(&color_output).unwrap().to_rgba8();
    let gray = image::open(&gray_output).unwrap().to_rgba8();
    let center = color.get_pixel(16, 16).0;
    assert!(
        center[0] > 0 && center[2] > 0,
        "chosen timestamp should render a blended transition pixel: {center:?}"
    );
    assert_ne!(center[0], center[1], "color output should remain in color: {center:?}");
    let expected_gray = ((u32::from(center[0]) * 19_595
        + u32::from(center[1]) * 38_470
        + u32::from(center[2]) * 7_471
        + 32_768)
        >> 16) as u8;
    let gray_center = gray.get_pixel(16, 16).0;
    assert_eq!(
        gray_center,
        [expected_gray, expected_gray, expected_gray, center[3]],
        "transition pixel should use the rounded RGB luminance and retain alpha"
    );
    for pixel in gray.pixels() {
        assert_eq!(pixel[0], pixel[1], "pixel should be grayscale: {pixel:?}");
        assert_eq!(pixel[1], pixel[2], "pixel should be grayscale: {pixel:?}");
    }
}

#[test]
fn image_command_preserves_typst_clip_and_code_highlight_pixels() {
    if OffscreenRenderer::new().is_err() {
        animatix_render::testing::skip_if_no_gpu();
        return;
    }
    let (project, input) = common::temp_project(
        "text-paint-acceptance",
        r##"
config { canvas_size: (520, 240), colorscheme: "editorial-dark" }
#0s
rect: Typst, text: "#box(width: 240pt, height: 32pt, fill: red)", text_max_width: 100, overflow: "clip", at: (140, 65)
code: Code, text: "let answer = 42;", language: "rust", font_size: 40, at: (270, 152)
"##,
    );
    let output = project.path().join("text.png");
    let result = Command::new(env!("CARGO_BIN_EXE_animatix"))
        .args(["--no-color", "image"])
        .arg(input)
        .args(["--time", "0", "-o"])
        .arg(&output)
        .output()
        .expect("run text image export");
    assert!(
        result.status.success(),
        "export failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let image = image::open(output).unwrap().to_rgb8();
    let inside = image.get_pixel(140, 65).0;
    assert!(inside[0] > 200 && inside[1] < 100, "Typst red fill was lost: {inside:?}");
    assert_eq!(image.get_pixel(196, 65), image.get_pixel(4, 4), "Typst clip leaked");
    let colors: std::collections::HashSet<_> = image
        .enumerate_pixels()
        .filter(|(_, y, _)| (120..185).contains(y))
        .map(|(_, _, rgb)| rgb.0)
        .filter(|rgb| rgb.iter().max().unwrap() - rgb.iter().min().unwrap() > 30)
        .collect();
    assert!(colors.len() > 4, "highlighted tokens should paint in theme colors");
}
