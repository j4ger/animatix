//! Scene-camera (`camera.at` / `camera.zoom` / `camera.rotation`) build and
//! sampling behaviour.

use super::*;
use crate::timeline::SceneDimensions;
use kurbo::{Affine, Point};

fn build(source: &str) -> Timeline {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new()).output
}

fn scene() -> SceneDimensions {
    SceneDimensions {
        width: 640,
        height: 360,
    }
}

const PLAIN: &str = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), at: (200, 100), color: accent.primary
"#;

#[test]
fn a_scene_that_never_addresses_the_camera_stays_untransformed() {
    let timeline = build(PLAIN);
    assert!(
        !timeline.camera_used.get(),
        "an untouched camera must not engage the frame path"
    );
    let affine = timeline.camera.affine(0, scene(), None);
    assert_eq!(affine, Affine::IDENTITY);
}

#[test]
fn camera_zoom_magnifies_about_the_scene_center() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), at: (200, 100), color: accent.primary
camera.zoom = 2.0
"#,
    );
    assert!(timeline.camera_used.get());
    let affine = timeline.camera.affine(0, scene(), None);
    // The center is fixed; a point 100 px right of it lands 200 px right.
    assert_eq!(affine * Point::new(320.0, 180.0), Point::new(320.0, 180.0));
    assert_eq!(affine * Point::new(420.0, 180.0), Point::new(520.0, 180.0));
}

#[test]
fn camera_pan_shifts_in_screen_pixels_after_the_zoom() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), at: (200, 100), color: accent.primary
camera.zoom = 2.0
camera.at = (10.0, -20.0)
"#,
    );
    let affine = timeline.camera.affine(0, scene(), None);
    assert_eq!(affine * Point::new(420.0, 180.0), Point::new(530.0, 160.0));
}

#[test]
fn camera_axes_interpolate_over_their_authored_duration() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), at: (200, 100), color: accent.primary
camera.zoom = 1.0
#1s
camera.zoom = 3.0 [1000ms, ease: linear]
"#,
    );
    let start = timeline.camera.affine(1000, scene(), None) * Point::new(420.0, 180.0);
    let middle = timeline.camera.affine(1500, scene(), None) * Point::new(420.0, 180.0);
    let end = timeline.camera.affine(2000, scene(), None) * Point::new(420.0, 180.0);
    assert_eq!(start.x, 420.0, "zoom 1 keeps the point where it is");
    assert_eq!(end.x, 620.0, "zoom 3 → 300 px from center");
    assert!(
        (middle.x - 520.0).abs() < 1e-6,
        "linear halfway should sit at 520, got {middle:?}"
    );
}

#[test]
fn an_always_block_camera_write_engages_the_frame_path() {
    // `always` never touches the camera tracks — it writes the frame override
    // map — so `refresh_camera_used` has to read the modifier statements.
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
always { camera.at = (4.0, 4.0) }
#0s
a: Rect, size: (100, 100), at: (200, 100), color: accent.primary
"#,
    );
    assert!(timeline.camera_used.get(), "an `always`-driven camera must count as used");

    let mut per_axis = std::collections::HashMap::new();
    per_axis.insert("at".to_string(), Value::Vec2([7.0, -7.0]));
    let affine = timeline.camera.affine(0, scene(), Some(&per_axis));
    assert_eq!(affine * Point::new(320.0, 180.0), Point::new(327.0, 173.0));
}

fn build_report(source: &str) -> Vec<animatix_syntax::diagnostics::Diagnostic> {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new()).diagnostics
}

#[test]
fn an_unknown_camera_axis_is_reported() {
    let diagnostics = build_report(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), at: (200, 100)
camera.tilt = 0.4
"#,
    );
    let message = diagnostics
        .iter()
        .find(|d| d.message.contains("has no property"))
        .expect("unknown camera axis is reported");
    assert!(
        message.message.contains("rotation / spin"),
        "the report should name the axes that do exist: {}",
        message.message
    );
}

#[test]
fn a_wrongly_shaped_camera_value_is_reported() {
    let diagnostics = build_report(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), at: (200, 100)
camera.zoom = (1.0, 2.0)
"#,
    );
    assert!(
        diagnostics.iter().any(|d| d.message.contains("camera.zoom` expects a number")),
        "a tuple zoom must be reported, got {:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn the_camera_label_is_reserved_for_the_camera() {
    let diagnostics = build_report(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
camera: Rect, size: (100, 100), at: (200, 100)
"#,
    );
    assert!(
        diagnostics.iter().any(|d| d.message.contains("is reserved")),
        "declaring an actor named `camera` must warn, got {:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}
