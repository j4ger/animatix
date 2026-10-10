use super::*;
use animatix_syntax::parser::parse_source;
use crate::timeline::TrackAccessor;

fn build_test_timeline(source: &str) -> Timeline {
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&stmts.expect("parsed AST"), &std::collections::HashMap::new());
    report.output
}

#[test]
fn idle_float_synthesizes_seamless_motion_offset() {
    let source = r#"
#0s
box: Rect, size: (100, 100), at: (200, 200), idle: "float"
"#;
    let timeline = build_test_timeline(source);
    let track = timeline.tracks.get("box").expect("box track exists");
    
    // At t=0, motion_offset y is 0.0
    let y0 = track.geometry.motion_offset.get(0, [0.0, 0.0])[1];
    assert!((y0 - 0.0).abs() < 1e-4, "y0 was {y0}");

    // At t=750, motion_offset y is -6.0
    let y750 = track.geometry.motion_offset.get(750, [0.0, 0.0])[1];
    assert!((y750 - (-6.0)).abs() < 1e-4, "y750 was {y750}");

    // At t=1500, motion_offset y returns to 0.0
    let y1500 = track.geometry.motion_offset.get(1500, [0.0, 0.0])[1];
    assert!((y1500 - 0.0).abs() < 1e-4, "y1500 was {y1500}");

    // At t=2250, motion_offset y is +6.0
    let y2250 = track.geometry.motion_offset.get(2250, [0.0, 0.0])[1];
    assert!((y2250 - 6.0).abs() < 1e-4, "y2250 was {y2250}");

    // At t=3000, motion_offset y returns to 0.0
    let y3000 = track.geometry.motion_offset.get(3000, [0.0, 0.0])[1];
    assert!((y3000 - 0.0).abs() < 1e-4, "y3000 was {y3000}");
}

#[test]
fn idle_breath_synthesizes_seamless_scale() {
    let source = r#"
#0s
box: Rect, size: (100, 100), at: (200, 200), idle: "breath"
"#;
    let timeline = build_test_timeline(source);
    let track = timeline.tracks.get("box").expect("box track exists");

    // At t=0, scale is 1.0
    let s0 = track.geometry.scale.get(0, 1.0);
    assert!((s0 - 1.0).abs() < 1e-4, "s0 was {s0}");

    // At t=1500, scale is 1.03
    let s1500 = track.geometry.scale.get(1500, 1.0);
    assert!((s1500 - 1.03).abs() < 1e-4, "s1500 was {s1500}");

    // At t=3000, scale returns to 1.0
    let s3000 = track.geometry.scale.get(3000, 1.0);
    assert!((s3000 - 1.0).abs() < 1e-4, "s3000 was {s3000}");
}

#[test]
fn rect_with_shadow_generates_shadow_render_command_and_bounds() {
    let source = r#"
#0s
box: Rect, size: (100, 100), at: (200, 200), shadow: (0, 8, 16, 2), shadow_color: (0, 0, 0, 0.25)
"#;
    let timeline = build_test_timeline(source);
    let track = timeline.tracks.get("box").expect("box track exists");
    use crate::primitives::Primitive;
    let primitive = crate::primitives::RECT;
    let asset_cache = crate::timeline::assets::AssetCache::default();
    let ctx = crate::primitives::EvaluateCtx {
        track,
        time_ms: 0,
        local_transform: kurbo::Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions { width: 1920, height: 1080 },
        background_color: [0.0, 0.0, 0.0, 1.0],
        overrides: None,
        vector_paths: &[],
        asset_cache: &asset_cache,
        target_resolver: None,
    };
    let cmds = primitive.evaluate(&ctx, None).expect("eval success").expect("render commands");
    assert_eq!(cmds.len(), 2, "Expected shadow + rect fill commands");
    match cmds[0].clone() {
        crate::primitives::RenderCommand::Shadow { size, corner_radius, params, color } => {
            assert_eq!(size, [50.0, 50.0]);
            assert_eq!(corner_radius, 0.0);
            assert_eq!(params, [0.0, 8.0, 16.0, 2.0]);
            assert_eq!(color, [0.0, 0.0, 0.0, 0.25]);
            let bounds = cmds[0].local_bounds(None).expect("shadow has local bounds");
            assert!(bounds.x0 < -52.0);
            assert!(bounds.y1 > 52.0 + 8.0);
        },
        other => panic!("Expected RenderCommand::Shadow as first command, got {other:?}"),
    }
}

#[test]
fn tilt_affine_applied_from_rotate_x_and_rotate_y() {
    let source = r#"
#0s
box: Rect, size: (100, 100), at: (200, 200), rotate_x: 0.1, rotate_y: 0.2
"#;
    let timeline = build_test_timeline(source);
    let track = timeline.tracks.get("box").expect("box track exists");
    let dims = SceneDimensions { width: 1920, height: 1080 };
    let node = timeline.evaluate_node_transform(track, 0, 1.0, kurbo::Affine::IDENTITY, dims, None, None);
    let coeffs = node.local_transform.as_coeffs();
    let rx: f64 = 0.1;
    let ry: f64 = 0.2;
    assert!((coeffs[0] - ry.cos()).abs() < 1e-4);
    assert!((coeffs[1] - rx.sin() * ry.sin()).abs() < 1e-4);
    assert!((coeffs[2] - 0.0).abs() < 1e-4);
    assert!((coeffs[3] - rx.cos()).abs() < 1e-4);
}

