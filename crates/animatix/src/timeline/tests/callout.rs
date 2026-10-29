//! Tests for the Callout primitive

use super::*;
use crate::ast::Property;
use crate::timeline::animation_track::{CalloutPlace, GeometryTracks};

// ── AST fixtures ──────────────────────────────────────────────────────────

fn prop(name: &str, value: Expr) -> Property {
    Property {
        name: name.to_string(),
        value,
        value_span: None,
        trailing_comment: None,
    }
}

fn vec2(x: f32, y: f32) -> Expr {
    Expr::Tuple(vec![Expr::Num(x as f64), Expr::Num(y as f64)])
}

fn actor(label: &str, ty: &str, props: Vec<Property>) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: ty.to_string(),
        props,
        modifiers: vec![],
        children: vec![],
        span: None,
    }
}

fn keyframe(seconds: f64, body: Vec<Stmt>) -> Stmt {
    Stmt::Keyframe {
        time: crate::ast::Time::Seconds(seconds),
        body,
        span: None,
    }
}

fn assign(label: &str, property: &str, value: Expr) -> Stmt {
    Stmt::Assignment {
        target: vec![crate::ast::TargetSegment::Static(label.to_string())],
        property: property.to_string(),
        value,
        modifiers: vec![],
        easing: None,
        value_span: None,
        span: None,
    }
}

fn make_config() -> Stmt {
    Stmt::Config {
        settings: vec![prop("colorscheme", Expr::Str("editorial-dark".to_string()))],
        span: None,
    }
}

fn build_report(ast: Vec<Stmt>) -> crate::timeline::BuildReport<Timeline> {
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new())
}

/// A note actor with the authored `Callout` properties.
fn note(props: Vec<Property>) -> Stmt {
    actor("note", "Callout", props)
}

/// One `Callout` at t=0, which is what most property tests need.
fn callout(props: Vec<Property>) -> Vec<Stmt> {
    vec![make_config(), keyframe(0.0, vec![note(props)])]
}

/// A plain Rect target (half-size) for target-mode callouts.
fn rect(label: &str) -> Stmt {
    actor(
        label,
        "Rect",
        vec![
            prop("position", vec2(200.0, 100.0)),
            prop("size", vec2(80.0, 40.0)),
        ],
    )
}

/// A note targeting `target`, with the standard place/standoff/label.
fn target_note(target: Expr) -> Stmt {
    note(vec![
        prop("target", target),
        prop("place", Expr::Ident("right".to_string())),
        prop("standoff", Expr::Num(40.0)),
        prop("label", Expr::Str("key".to_string())),
    ])
}

/// Build `actors` plus a targeting note at t=0, optionally retargeting at 1s.
fn targeted(actors: Vec<Stmt>, target: Expr, retarget: Option<Expr>) -> Timeline {
    let mut body = actors;
    body.push(target_note(target));
    let mut ast = vec![make_config(), keyframe(0.0, body)];
    if let Some(value) = retarget {
        ast.push(keyframe(1.0, vec![assign("note", "target", value)]));
    }
    build_report(ast).output
}

/// Build a minimal timeline with a single Callout actor.
fn build_callout_timeline(
    from: [f32; 2],
    to: [f32; 2],
    label: &str,
    label_at: [f32; 2],
    head_size: f32,
) -> Timeline {
    build_report(vec![
        make_config(),
        keyframe(
            0.0,
            vec![actor(
                "callout",
                "Callout",
                vec![
                    prop("from", vec2(from[0], from[1])),
                    prop("to", vec2(to[0], to[1])),
                    prop("label", Expr::Str(label.to_string())),
                    prop("label_at", vec2(label_at[0], label_at[1])),
                    prop("head_size", Expr::Num(head_size as f64)),
                    prop("color", Expr::Ident("accent.primary".to_string())),
                ],
            )],
        ),
    ])
    .output
}

fn make_target_track(
    _label: &str,
    pos: [f32; 2],
    half: [f32; 2],
) -> (&'static str, GeometryTracks) {
    let geometry = GeometryTracks {
        position: Some({
            let mut t = PropertyTrack::new(pos);
            t.add_keyframe(0, pos, Easing::Linear);
            t
        }),
        size: Some({
            let mut t = PropertyTrack::new(half);
            t.add_keyframe(0, half, Easing::Linear);
            t
        }),
        ..Default::default()
    };
    ("Rect", geometry)
}

/// Make a Callout track that targets another actor (place=right, standoff=0).
fn make_callout_track(_label: &str, target: &str) -> (&'static str, GeometryTracks) {
    let geometry = GeometryTracks {
        callout_target: Some({
            let mut t = PropertyTrack::new(target.to_string());
            t.add_keyframe(0, target.to_string(), Easing::Linear);
            t
        }),
        callout_place: Some({
            let mut t = PropertyTrack::new(CalloutPlace::Right);
            t.add_keyframe(0, CalloutPlace::Right, Easing::Linear);
            t
        }),
        callout_standoff: Some({
            let mut t = PropertyTrack::new(0.0f32);
            t.add_keyframe(0, 0.0, Easing::Linear);
            t
        }),
        ..Default::default()
    };
    ("Callout", geometry)
}

fn spawn_seen(
    timeline: &mut crate::timeline::Timeline,
    label: &str,
    actor_type: &str,
    geometry: GeometryTracks,
) -> crate::timeline::EntityId {
    let id = timeline.world.spawn(label, actor_type);
    *timeline.world.geometry_mut(id) = geometry;
    timeline.world.meta_mut(id).first_seen_ms = 0;
    id
}

/// The callout tip the frame derives for `note`.
fn derived_to(timeline: &Timeline) -> [f32; 2] {
    let note = timeline.get_entity_id("note").unwrap();
    crate::timeline::callout_geometry::derive_callout_geometry(
        timeline.world().geometry(note),
        timeline.world().shape_or_default(note),
        0,
        Some(timeline),
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    )
    .to
}

// ── Rendering and property tests ──────────────────────────────────────────

#[test]
fn test_callout_basic_rendering() {
    // A Callout renders both arrow and text: line_from/line_to, label,
    // label_at and head_size all reach their tracks.
    let timeline = build_callout_timeline([0.0, 0.0], [100.0, 0.0], "Test", [10.0, 5.0], 10.0);

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);

    let track = timeline.get_entity_id("callout").expect("callout track should exist");
    let geometry = timeline.world.geometry(track);
    assert_eq!(timeline.world.actor_type(track), "Callout", "track kind should be Callout");

    let from = timeline.world().shape_or_default(track).line_from.get(0, [0.0; 2]);
    assert_eq!(from, [0.0, 0.0], "from should be (0, 0)");

    let to = timeline.world().shape_or_default(track).line_to.get(0, [0.0; 2]);
    assert_eq!(to, [100.0, 0.0], "to should be (100, 0)");

    let label_val = timeline.world().text_or_default(track).text_content.get(0, String::new());
    assert_eq!(label_val, "Test", "label should be 'Test'");

    let label_at = geometry.label_at.get(0, [0.0; 2]);
    assert_eq!(label_at, [10.0, 5.0], "label_at should be (10, 5)");

    let head = timeline.world().shape_or_default(track).head_size.get(0, 0.0);
    assert_eq!(head, 10.0, "head_size should be 10.0");

    let _scene = timeline.evaluate(2.5, dims);
}

#[test]
fn test_callout_label_positioning() {
    let timeline = build_callout_timeline([0.0, 0.0], [100.0, 0.0], "Note", [10.0, 5.0], 10.0);
    let track = timeline.get_entity_id("callout").expect("callout track should exist");
    let geometry = timeline.world.geometry(track);
    let label_at = geometry.label_at.get(0, [0.0; 2]);
    assert_eq!(label_at, [10.0, 5.0], "label_at should be (10,5)");

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_callout_animation() {
    // An assignment at 2s adds keyframes to the `to` track.
    let ast = vec![
        make_config(),
        keyframe(
            0.0,
            vec![actor(
                "anim_callout",
                "Callout",
                vec![
                    prop("from", vec2(0.0, 0.0)),
                    prop("to", vec2(100.0, 100.0)),
                    prop("label", Expr::Str("Animated".to_string())),
                    prop("head_size", Expr::Num(10.0)),
                ],
            )],
        ),
        keyframe(
            2.0,
            vec![{
                let mut stmt = assign("anim_callout", "to", vec2(300.0, 200.0));
                if let Stmt::Assignment { easing, .. } = &mut stmt {
                    *easing = Some(crate::easing::Easing::EaseInOut);
                }
                stmt
            }],
        ),
    ];

    let report = build_report(ast);
    assert!(
        report.diagnostics.is_empty(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let track = timeline.get_entity_id("anim_callout").expect("anim_callout track should exist");

    let to_track = timeline
        .world()
        .shape_or_default(track)
        .line_to
        .as_ref()
        .expect("line_to track should exist");
    assert!(
        to_track.keyframes.len() >= 2,
        "line_to should have at least 2 keyframes for animation, got {}",
        to_track.keyframes.len()
    );

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let to_0s = timeline.world().shape_or_default(track).line_to.get(0, [0.0; 2]);
    assert_eq!(to_0s, [100.0, 100.0], "to at 0s should be (100, 100)");

    let _scene_0s = timeline.evaluate(0.0, dims);
    let _scene_2s = timeline.evaluate(2.0, dims);
    let _scene_1s = timeline.evaluate(1.0, dims);
}

#[test]
fn test_callout_arrowhead_size() {
    let timeline_small = build_callout_timeline([0.0, 0.0], [100.0, 0.0], "Small", [0.0, 0.0], 5.0);
    let timeline_large =
        build_callout_timeline([0.0, 0.0], [100.0, 0.0], "Large", [0.0, 0.0], 20.0);

    let track_small = timeline_small.get_entity_id("callout").expect("callout track should exist");
    let track_large = timeline_large.get_entity_id("callout").expect("callout track should exist");

    let head_small = timeline_small.world().shape_or_default(track_small).head_size.get(0, 0.0);
    let head_large = timeline_large.world().shape_or_default(track_large).head_size.get(0, 0.0);

    assert_eq!(head_small, 5.0, "small head_size should be 5.0");
    assert_eq!(head_large, 20.0, "large head_size should be 20.0");
    assert!(
        head_large > head_small,
        "large head_size should be greater than small head_size"
    );
}

#[test]
fn test_callout_no_label_does_not_set_text_content() {
    let ast = vec![
        make_config(),
        keyframe(
            0.0,
            vec![actor(
                "no_label",
                "Callout",
                vec![prop("from", vec2(0.0, 0.0)), prop("to", vec2(100.0, 0.0))],
            )],
        ),
    ];
    let timeline = build_report(ast).output;

    let track = timeline.get_entity_id("no_label").expect("no_label track should exist");
    assert_eq!(timeline.world.actor_type(track), "Callout");

    let label_val = timeline.world().text_or_default(track).text_content.get(0, String::new());
    assert_eq!(label_val, "", "label should be empty when not provided");

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_callout_target_assignment_accepts_bare_actor_label() {
    let timeline = targeted(
        vec![rect("box1"), rect("box2")],
        Expr::Ident("box1".to_string()),
        Some(Expr::Ident("box2".to_string())),
    );
    let note = timeline.get_entity_id("note").expect("note track should exist");
    let geometry = timeline.world.geometry(note);
    assert_eq!(geometry.callout_target.get(0, String::new()), "box1");
    assert_eq!(geometry.callout_target.get(1000, String::new()), "box2");
}

#[test]
fn test_callout_target_accepts_array_actor_refs() {
    let timeline = targeted(
        vec![rect("bar__0"), rect("bar__2")],
        Expr::Index(Box::new(Expr::Ident("bar".to_string())), Box::new(Expr::Num(0.0))),
        Some(Expr::Index(Box::new(Expr::Ident("bar".to_string())), Box::new(Expr::Num(2.0)))),
    );
    let note = timeline.get_entity_id("note").expect("note track should exist");
    let geometry = timeline.world.geometry(note);
    assert_eq!(geometry.callout_target.get(0, String::new()), "bar__0");
    assert_eq!(geometry.callout_target.get(1000, String::new()), "bar__2");
}

#[test]
fn test_callout_target_accepts_namespaced_array_actor_refs() {
    // Component-expanded targets use `instance.label[index]`; the callout
    // target parser must resolve that to `instance.label__N`, not drop the
    // index or reject the path.
    let timeline = targeted(
        vec![rect("bars.bar__2")],
        Expr::Index(
            Box::new(Expr::Path(vec!["bars".to_string(), "bar".to_string()])),
            Box::new(Expr::Num(2.0)),
        ),
        None,
    );
    let note = timeline.get_entity_id("note").expect("note track should exist");
    let geometry = timeline.world.geometry(note);
    assert_eq!(geometry.callout_target.get(0, String::new()), "bars.bar__2");
}

#[test]
fn test_callout_target_mode_seeds_tracks() {
    // Verify that target/place/standoff/to_offset are seeded into tracks
    // when a targeted Callout is declared.
    let ast = vec![
        make_config(),
        keyframe(
            0.0,
            vec![
                actor(
                    "box",
                    "Rect",
                    vec![
                        prop("position", vec2(200.0, 100.0)),
                        prop("size", vec2(80.0, 40.0)),
                    ],
                ),
                note(vec![
                    prop("target", Expr::Ident("box".to_string())),
                    prop("place", Expr::Ident("right".to_string())),
                    prop("standoff", Expr::Num(40.0)),
                    prop("to_offset", vec2(0.0, 0.0)),
                    prop("label", Expr::Str("Look here".to_string())),
                ]),
            ],
        ),
    ];

    let report = build_report(ast);
    assert!(
        report.diagnostics.is_empty(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let note = timeline.get_entity_id("note").expect("note track should exist");
    let geometry = timeline.world.geometry(note);

    assert_eq!(
        geometry.callout_target.get(0, String::new()),
        "box",
        "callout_target should be 'box'"
    );
    assert_eq!(
        geometry.callout_place.get(0, CalloutPlace::Right),
        CalloutPlace::Right,
        "callout_place should be Right"
    );
    assert!(
        (geometry.callout_standoff.get(0, 0.0) - 40.0).abs() < f32::EPSILON,
        "callout_standoff should be 40.0"
    );
    assert_eq!(
        geometry.callout_to_offset.get(0, [99.0; 2]),
        [0.0, 0.0],
        "callout_to_offset should be (0, 0)"
    );

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_callout_target_mode_derives_positions() {
    // When target mode is active, evaluate() derives `to` at the right edge
    // of the target actor and `from` = to + [standoff, 0]. Exact arrow
    // positions are internal to the render path, so this checks no panic.
    let ast = vec![
        make_config(),
        keyframe(
            0.0,
            vec![
                actor(
                    "box",
                    "Rect",
                    vec![
                        prop("position", vec2(200.0, 100.0)),
                        prop("size", vec2(80.0, 40.0)),
                    ],
                ),
                note(vec![
                    prop("target", Expr::Ident("box".to_string())),
                    prop("place", Expr::Ident("right".to_string())),
                    prop("standoff", Expr::Num(40.0)),
                    prop("label", Expr::Str("Note".to_string())),
                ]),
            ],
        ),
    ];
    let timeline = build_report(ast).output;
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_callout_place_variants_accepted() {
    // Verify that all valid place identifiers parse to the correct enum variant.
    for (ident, expected) in &[
        ("right", CalloutPlace::Right),
        ("left", CalloutPlace::Left),
        ("top", CalloutPlace::Top),
        ("above", CalloutPlace::Top),
        ("bottom", CalloutPlace::Bottom),
        ("below", CalloutPlace::Bottom),
        ("auto", CalloutPlace::Auto),
    ] {
        let report = build_report(callout(vec![prop("place", Expr::Ident(ident.to_string()))]));
        assert!(
            report.diagnostics.is_empty(),
            "place={ident} produced unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let track = report.output.get_entity_id("note").unwrap();
        let geometry = report.output.world.geometry(track);
        assert_eq!(
            geometry.callout_place.get(0, CalloutPlace::Right),
            *expected,
            "place={ident} should map to {expected:?}"
        );
    }
}

#[test]
fn test_callout_place_invalid_produces_diagnostic() {
    let report = build_report(callout(vec![prop("place", Expr::Ident("sideways".to_string()))]));
    assert!(
        !report.diagnostics.is_empty(),
        "invalid place 'sideways' should produce a diagnostic"
    );
}

#[test]
fn test_callout_place_default_is_right() {
    let report = build_report(callout(vec![]));
    let track = report.output.get_entity_id("note").unwrap();
    let geometry = report.output.world.geometry(track);
    assert_eq!(
        geometry.callout_place.get(0, CalloutPlace::Auto),
        CalloutPlace::Right,
        "default callout_place should be Right"
    );
}

// ── Transform-aware target bounds regression tests ────────────────────────

#[test]
fn test_callout_target_translated_world_bounds() {
    // A top-level target at (200, 100) with half=(40, 20).
    // place=right → attach = (240, 100).
    let mut timeline = Timeline::new();
    let (target_type, target_geometry) = make_target_track("box", [200.0, 100.0], [40.0, 20.0]);
    spawn_seen(&mut timeline, "box", target_type, target_geometry);
    let (callout_type, callout_geometry) = make_callout_track("note", "box");
    spawn_seen(&mut timeline, "note", callout_type, callout_geometry);

    let geom = derived_to(&timeline);
    assert!((geom[0] - 240.0).abs() < 0.5, "expected to.x=240, got {}", geom[0]);
    assert!((geom[1] - 100.0).abs() < 0.5, "expected to.y=100, got {}", geom[1]);
}

#[test]
fn test_callout_target_scaled_world_bounds() {
    // Target at (200, 100), half=(40, 20), scale=2 → AABB half=(80, 40).
    // place=right → attach = (280, 100).
    let mut timeline = Timeline::new();
    let (target_type, mut target_geometry) = make_target_track("box", [200.0, 100.0], [40.0, 20.0]);
    target_geometry.scale = Some({
        let mut t = PropertyTrack::new(2.0f32);
        t.add_keyframe(0, 2.0, Easing::Linear);
        t
    });
    spawn_seen(&mut timeline, "box", target_type, target_geometry);
    let (callout_type, callout_geometry) = make_callout_track("note", "box");
    spawn_seen(&mut timeline, "note", callout_type, callout_geometry);

    let geom = derived_to(&timeline);
    assert!(
        (geom[0] - 280.0).abs() < 0.5,
        "expected to.x=280 (scale-aware), got {}",
        geom[0]
    );
    assert!((geom[1] - 100.0).abs() < 0.5, "expected to.y=100, got {}", geom[1]);
}

#[test]
fn test_callout_target_rotated_aabb() {
    // Target at (200, 100), half=(40, 20), rotation=π/2: the AABB swaps axes,
    // so place=right attaches at 200 + half_y = 220.
    let angle_rad = std::f32::consts::FRAC_PI_2;
    let mut timeline = Timeline::new();
    let (target_type, mut target_geometry) = make_target_track("box", [200.0, 100.0], [40.0, 20.0]);
    target_geometry.rotation = Some({
        let mut t = PropertyTrack::new(angle_rad);
        t.add_keyframe(0, angle_rad, Easing::Linear);
        t
    });
    spawn_seen(&mut timeline, "box", target_type, target_geometry);
    let (callout_type, callout_geometry) = make_callout_track("note", "box");
    spawn_seen(&mut timeline, "note", callout_type, callout_geometry);

    let hw = 40.0_f32;
    let hh = 20.0_f32;
    let expected_half_x = hw * angle_rad.cos().abs() + hh * angle_rad.sin().abs();
    let expected_to_x = 200.0 + expected_half_x;
    let geom = derived_to(&timeline);
    assert!(
        (geom[0] - expected_to_x).abs() < 0.5,
        "expected to.x≈{expected_to_x:.1} (rotation-aware AABB), got {}",
        geom[0]
    );
}

#[test]
fn test_callout_target_nested_child_world_bounds() {
    // parent at (100, 0); child nested at (100, 0) relative to parent.
    // Child world centre = (200, 0); place=right → attach = (240, 0).
    let mut timeline = Timeline::new();
    let (parent_type, parent_geometry) = make_target_track("parent", [100.0, 0.0], [50.0, 50.0]);
    let parent_id = spawn_seen(&mut timeline, "parent", parent_type, parent_geometry);
    let (child_type, child_geometry) = make_target_track("child", [100.0, 0.0], [40.0, 20.0]);
    let child_id = spawn_seen(&mut timeline, "child", child_type, child_geometry);
    timeline.world.link(parent_id, child_id);

    let (callout_type, callout_geometry) = make_callout_track("note", "child");
    spawn_seen(&mut timeline, "note", callout_type, callout_geometry);

    let geom = derived_to(&timeline);
    assert!(
        (geom[0] - 240.0).abs() < 0.5,
        "expected to.x=240 (nested world bounds), got {}",
        geom[0]
    );
    assert!((geom[1] - 0.0).abs() < 0.5, "expected to.y=0, got {}", geom[1]);
}

#[test]
fn test_callout_missing_target_produces_diagnostic() {
    let report = build_report(callout(vec![prop("target", Expr::Str("nonexistent".to_string()))]));
    use crate::diagnostics::DiagnosticCode;
    let found = report
        .diagnostics
        .iter()
        .any(|d| matches!(d.code, DiagnosticCode::CalloutTargetNotFound));
    assert!(
        found,
        "expected CalloutTargetNotFound diagnostic, got: {:?}",
        report.diagnostics
    );
}
