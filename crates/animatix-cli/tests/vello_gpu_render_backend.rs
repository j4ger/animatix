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
fn image_command_preserves_typst_clip_and_code_highlight_pixels() {
    if OffscreenRenderer::new().is_err() {
        animatix_render::testing::skip_if_no_gpu();
        return;
    }
    let (project, input) = common::temp_project(
        "text-paint-acceptance",
        r##"
config { resolution: (520, 240), colorscheme: "editorial-dark" }
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
