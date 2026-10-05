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

/// A camera axis first addressed at a later stamp holds its identity until that
/// stamp. The delayed-write path seeds the track with the value in effect
/// before it, and for a track that did not exist that used to be `T::default()`
/// — `0.0` for `zoom`, which collapsed the whole scene to its center point for
/// every frame before the stamp.
#[test]
fn a_delayed_camera_write_holds_its_identity_until_its_stamp() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), at: (200, 100), color: accent.primary
#1s
camera.zoom = 2.0
"#,
    );
    // Before the stamp: no transform at all, not a zero-scale collapse.
    assert_eq!(timeline.camera.affine(0, scene(), None), Affine::IDENTITY);
    assert_eq!(timeline.camera.affine(999, scene(), None), Affine::IDENTITY);
    // After it: the authored zoom, about the same center.
    let affine = timeline.camera.affine(1500, scene(), None);
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

/// World-space bounds centre of `label` after evaluating the frame at `time_s`.
/// `precise_bounds` is camera-included world space — the same space `verify`
/// reads — so it is where an opt-out has to show up.
fn bounds_centre(timeline: &Timeline, time_s: f64, label: &str) -> Point {
    let dims = SceneDimensions {
        width: 640,
        height: 360,
    };
    let mut backend: Option<&mut dyn crate::timeline::effects::FilterBackend> = None;
    let program = timeline.evaluate_program_with_debug(
        time_s,
        dims,
        DebugRenderOptions::default(),
        &mut backend,
    );
    let rect = *program
        .precise_bounds
        .get(label)
        .unwrap_or_else(|| panic!("{label} has no bounds at {time_s}s"));
    Point::new((rect.x0 + rect.x1) / 2.0, (rect.y0 + rect.y1) / 2.0)
}

/// `camera_follow: false` is the HUD case: the scene pushes in, the overlay does
/// not move. Measured against an identical actor that stays camera'd, in the same
/// scene, so the control cannot drift.
#[test]
fn a_camera_follow_false_root_stays_put_under_a_push_in() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
scrolled: Rect, size: (40, 40), at: (420, 180), color: accent.primary
hud: Rect, size: (40, 40), at: (420, 180), color: accent.secondary, camera_follow: false
#1s
camera.zoom = 2.0
"#,
    );
    assert!(timeline.camera_used.get());
    // Before the push both are where they were authored.
    assert_eq!(bounds_centre(&timeline, 0.0, "scrolled"), Point::new(420.0, 180.0));
    assert_eq!(bounds_centre(&timeline, 0.0, "hud"), Point::new(420.0, 180.0));

    // At 2x about the scene center (320, 180) the camera'd actor's x travels
    // 320 + (420 - 320) * 2 = 520. The HUD keeps its authored 420 — y is the
    // center line, so it moves for neither.
    assert_eq!(bounds_centre(&timeline, 2.0, "scrolled"), Point::new(520.0, 180.0));
    assert_eq!(bounds_centre(&timeline, 2.0, "hud"), Point::new(420.0, 180.0));
}

/// The flag is read once at build: a frame-time toggle would need a plan slot and
/// a track, which the opt-out does not ask for. Say so rather than accept an
/// assignment that quietly does nothing.
#[test]
fn camera_follow_cannot_be_assigned_at_frame_time() {
    let diagnostics = build_report(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
hud: Rect, size: (40, 40), at: (420, 180), camera_follow: false
#1s
hud.camera_follow = true [300ms]
"#,
    );
    assert!(
        diagnostics.iter().any(|d| {
            d.message.contains("camera_follow")
                && (d.message.contains("not part of the current runtime assignment surface")
                    || d.message.contains("not assignable"))
        }),
        "a frame-time `camera_follow` write must be reported, got {:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// A declaration that is not a bool is the type layer's business, and `check`
/// reports it with a span — measured on the CLI:
/// `type-mismatch: Type mismatch for 'Rect.camera_follow': expected Bool, found Str`.
/// What the engine has to guarantee is that a rejected value cannot reach the
/// frame: `value_parser`'s Bool arm returns `None` for anything but a bool, so
/// the flag keeps its default and the actor stays camera'd.
#[test]
fn a_non_bool_camera_follow_declaration_keeps_the_default() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
hud: Rect, size: (40, 40), at: (420, 180), camera_follow: "no"
"#,
    );
    let track = timeline.tracks.get("hud").expect("hud track");
    assert!(track.camera_follow, "a value the bool parser rejects must not flip the flag");
}

/// The opt-out applies where the camera applies — once, to a root subtree. On a
/// nested actor it would read as a HUD that keeps moving, so the declaration is
/// reported instead of left to be discovered in a render.
#[test]
fn a_nested_camera_follow_false_warns() {
    let diagnostics = build_report(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
deck: Col, at: (320, 180) {
  hud: Rect, size: (40, 40), camera_follow: false
}
"#,
    );
    assert!(
        diagnostics.iter().any(|d| d.message.contains("not a root actor")),
        "a nested opt-out must be reported, got {:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// A `Filter` scope that opts out keeps its authored `bounds:` as screen
/// coordinates. The rule lives inside `effect_scope_region` (it consults the
/// track it is given), so it is pinned here instead of through a render.
#[test]
fn a_camera_follow_false_scope_keeps_its_authored_region() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
bg: Filter, bounds: (40, 30, 120, 80), camera_follow: false {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
#1s
camera.zoom = 2.0
"#,
    );
    let scope = timeline.tracks.get("bg").expect("filter scope");
    let dims = SceneDimensions {
        width: 640,
        height: 360,
    };
    let camera = timeline.camera.affine(2000, dims, None);
    assert_ne!(camera, Affine::IDENTITY, "the scene is pushed in");

    let region = timeline
        .effect_scope_region(scope, dims, 2000, camera)
        .expect("the scope has a region");
    // Exactly what the identity camera gives the same authored rectangle:
    // (40, 30)-(160, 110), padded by the blur's 10 px support on each side.
    assert_eq!(region.origin, [30.0, 20.0]);
    assert_eq!(region.size.width, 140);
    assert_eq!(region.size.height, 100);
}

/// The flag has to reach **every** actor family. Text builds through
/// `process_text_actor_decl`, which returns before the generic property walk, so
/// a declaration read placed in the generic path is honoured on a `Rect` and
/// silently ignored on a `Text` — the first version of this feature did exactly
/// that, and the pinned overlay in `examples/animation/37_hud_overlay.amx` still
/// moved (and was then culled off-screen by the camera it should not have seen).
#[test]
fn camera_follow_reaches_the_actor_families_that_build_their_own_declaration() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (1280, 720) }
#0s
plate: Rect, size: (300, 200), at: (400, 400), color: accent.primary
hud: Text, text: "LIVE", font_size: 24, color: accent.secondary, at: (1088, 84), camera_follow: false
#1s
camera.zoom = 1.45
"#,
    );
    assert!(!timeline.tracks.get("hud").expect("hud track").camera_follow);
    assert!(timeline.tracks.get("plate").expect("plate track").camera_follow);

    let dims = SceneDimensions {
        width: 1280,
        height: 720,
    };
    let mut backend: Option<&mut dyn crate::timeline::effects::FilterBackend> = None;
    let program = timeline.evaluate_program_with_debug(
        2.6,
        dims,
        DebugRenderOptions::default(),
        &mut backend,
    );
    // 1.45x about (640, 360): the plate's own width grows from 300 to 435 and
    // its centre slides from 400 to 292.
    let plate = program.precise_bounds.get("plate").expect("plate bounds");
    assert!(
        (plate.x1 - plate.x0 - 435.0).abs() < 1.0,
        "the camera'd plate must be magnified 1.45x: {plate:?}"
    );
    assert!(plate.x0 < 100.0, "and pushed left of its authored edge: {plate:?}");
    let hud = program.precise_bounds.get("hud").expect("hud bounds");
    let centre = Point::new((hud.x0 + hud.x1) / 2.0, (hud.y0 + hud.y1) / 2.0);
    assert!(
        (centre.x - 1088.0).abs() < 1.0 && (centre.y - 84.0).abs() < 1.0,
        "a pinned Text overlay must stay at its authored position, got {hud:?}"
    );
}
