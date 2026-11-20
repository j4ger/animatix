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
