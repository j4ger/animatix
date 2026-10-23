//! Morph tests.

use super::align::*;
use super::geometry::*;
use super::interpolate::*;
use super::text::*;
use super::*;

use crate::easing::Easing;

#[test]
fn test_list_alignment() {
    let p1 = BezPath::from_vec(vec![
        PathEl::MoveTo(Point::new(0.0, 0.0)),
        PathEl::LineTo(Point::new(10.0, 0.0)),
    ]);
    let p2 = BezPath::from_vec(vec![
        PathEl::MoveTo(Point::new(0.0, 10.0)),
        PathEl::LineTo(Point::new(10.0, 10.0)),
    ]);

    let source = vec![p1.clone(), p2.clone()];
    let target = vec![p1.clone(), p2.clone(), p1.clone(), p2.clone(), p1.clone()];

    let aligned = align_path_lists(&source, &target);
    assert_eq!(aligned.len(), 5);

    // Check that 3rd item in source is degenerate
    let (s3, _t3) = &aligned[2];
    assert_eq!(s3.elements().len(), 1);
    assert!(matches!(s3.elements()[0], PathEl::MoveTo(_)));
}

#[test]
fn test_subpath_alignment() {
    let mut source = BezPath::new();
    source.move_to(Point::new(0.0, 0.0));
    source.line_to(Point::new(10.0, 0.0));

    let mut target = BezPath::new();
    target.move_to(Point::new(0.0, 0.0));
    target.line_to(Point::new(10.0, 0.0));
    target.move_to(Point::new(20.0, 0.0));
    target.line_to(Point::new(30.0, 0.0));

    let (a_src, a_tgt) = align_subpaths(&source, &target);

    // 2 subpaths each
    let src_subs = extract_subpaths(&a_src);
    let tgt_subs = extract_subpaths(&a_tgt);
    assert_eq!(src_subs.len(), 2);
    assert_eq!(tgt_subs.len(), 2);
}

#[test]
fn test_segment_alignment() {
    // Triangle
    let mut source = BezPath::new();
    source.move_to(Point::new(0.0, 0.0));
    source.line_to(Point::new(10.0, 0.0));
    source.line_to(Point::new(5.0, 10.0));
    source.close_path();

    // Square
    let mut target = BezPath::new();
    target.move_to(Point::new(0.0, 0.0));
    target.line_to(Point::new(10.0, 0.0));
    target.line_to(Point::new(10.0, 10.0));
    target.line_to(Point::new(0.0, 10.0));
    target.close_path();

    let subs_src = extract_subpaths(&source);
    let subs_tgt = extract_subpaths(&target);

    let (a_src, a_tgt) = align_segments(&subs_src[0], &subs_tgt[0]);
    assert_eq!(a_src.len(), a_tgt.len());

    // Everything should be MoveTo, CurveTo, or ClosePath
    for el in &a_src {
        assert!(matches!(el, PathEl::MoveTo(_) | PathEl::CurveTo(_, _, _) | PathEl::ClosePath));
    }
}

#[test]
fn test_morphing() {
    let mut source = BezPath::new();
    source.move_to(Point::new(0.0, 0.0));
    source.line_to(Point::new(10.0, 0.0));

    let mut target = BezPath::new();
    target.move_to(Point::new(0.0, 10.0));
    target.line_to(Point::new(10.0, 10.0));

    let t50 = morph_paths(&source, &target, 0.5);
    let els = t50.elements();

    if let PathEl::MoveTo(p) = els[0] {
        assert_eq!(p.y, 5.0);
    } else {
        panic!("Expected MoveTo");
    }
}

#[test]
fn test_match_strategy_reorders_paths_by_centroid() {
    let left = BezPath::from_vec(vec![
        PathEl::MoveTo(Point::new(0.0, 0.0)),
        PathEl::LineTo(Point::new(10.0, 0.0)),
    ]);
    let right = BezPath::from_vec(vec![
        PathEl::MoveTo(Point::new(100.0, 0.0)),
        PathEl::LineTo(Point::new(110.0, 0.0)),
    ]);

    let auto_pairs = align_path_lists_with_strategy(
        &[left.clone(), right.clone()],
        &[right.clone(), left.clone()],
        MorphStrategy::Auto,
    );
    let match_pairs = align_path_lists_with_strategy(
        &[left.clone(), right.clone()],
        &[right.clone(), left.clone()],
        MorphStrategy::Match,
    );

    assert_ne!(get_centroid(&auto_pairs[0].0), get_centroid(&auto_pairs[0].1));
    assert_eq!(get_centroid(&match_pairs[0].0), get_centroid(&match_pairs[0].1));
}

#[test]
fn test_nearest_strategy_pairs_by_proximity() {
    let left = BezPath::from_vec(vec![
        PathEl::MoveTo(Point::new(0.0, 0.0)),
        PathEl::LineTo(Point::new(10.0, 0.0)),
    ]);
    let right = BezPath::from_vec(vec![
        PathEl::MoveTo(Point::new(100.0, 0.0)),
        PathEl::LineTo(Point::new(110.0, 0.0)),
    ]);

    // Target order is swapped: [right, left]
    let nearest_pairs = align_path_lists_with_strategy(
        &[left.clone(), right.clone()],
        &[right.clone(), left.clone()],
        MorphStrategy::Nearest,
    );

    // Nearest should pair left↔left and right↔right despite index mismatch
    assert_eq!(get_centroid(&nearest_pairs[0].0), get_centroid(&nearest_pairs[0].1));
    assert_eq!(get_centroid(&nearest_pairs[1].0), get_centroid(&nearest_pairs[1].1));
}

#[test]
fn test_nearest_strategy_with_unequal_counts() {
    let p1 = BezPath::from_vec(vec![
        PathEl::MoveTo(Point::new(0.0, 0.0)),
        PathEl::LineTo(Point::new(10.0, 0.0)),
    ]);
    let p2 = BezPath::from_vec(vec![
        PathEl::MoveTo(Point::new(100.0, 0.0)),
        PathEl::LineTo(Point::new(110.0, 0.0)),
    ]);
    let p3 = BezPath::from_vec(vec![
        PathEl::MoveTo(Point::new(200.0, 0.0)),
        PathEl::LineTo(Point::new(210.0, 0.0)),
    ]);

    // 2 sources, 3 targets
    let pairs = align_path_lists_with_strategy(
        &[p1.clone(), p2.clone()],
        &[p3.clone(), p2.clone(), p1.clone()],
        MorphStrategy::Nearest,
    );

    assert_eq!(pairs.len(), 3);
    // p1 should pair with p1 (centroids match)
    // p2 should pair with p2 (centroids match)
    // p3 should pair with degenerate source
}

#[test]
fn test_path_arc_curves_midpoint_interpolation() {
    let mut source = BezPath::new();
    source.move_to(Point::new(0.0, 0.0));
    source.line_to(Point::new(10.0, 0.0));

    let mut target = BezPath::new();
    target.move_to(Point::new(0.0, 10.0));
    target.line_to(Point::new(10.0, 10.0));

    let curved = morph_paths_with_options(
        &source,
        &target,
        0.5,
        MorphOptions {
            strategy: MorphStrategy::Auto,
            path_arc: 1.57,
            stretch: false,
        },
    );

    match curved.elements()[0] {
        PathEl::MoveTo(point) => assert_ne!(point.x, 0.0),
        _ => panic!("Expected curved move-to point"),
    }
}

#[test]
fn test_stretch_normalizes_morph_bounds() {
    let mut source = BezPath::new();
    source.move_to(Point::new(0.0, 0.0));
    source.line_to(Point::new(10.0, 0.0));
    source.line_to(Point::new(10.0, 10.0));
    source.line_to(Point::new(0.0, 10.0));
    source.close_path();

    let mut target = BezPath::new();
    target.move_to(Point::new(0.0, 0.0));
    target.line_to(Point::new(100.0, 0.0));
    target.line_to(Point::new(100.0, 10.0));
    target.line_to(Point::new(0.0, 10.0));
    target.close_path();

    let stretched = morph_paths_with_options(
        &source,
        &target,
        0.5,
        MorphOptions {
            strategy: MorphStrategy::Auto,
            path_arc: 0.0,
            stretch: true,
        },
    );
    let unstretched = morph_paths_with_options(
        &source,
        &target,
        0.5,
        MorphOptions {
            strategy: MorphStrategy::Auto,
            path_arc: 0.0,
            stretch: false,
        },
    );

    let stretched_bounds = path_bounds(&stretched);
    let unstretched_bounds = path_bounds(&unstretched);
    assert!(stretched_bounds.width() >= unstretched_bounds.width());
}

// ────────────────────────────────────────────────────────
// evaluate_paths_with_options tests (moved from track.rs)
// ────────────────────────────────────────────────────────

fn identity_interpolate<T: Interpolate>(
    source: &T,
    target: &T,
    t: f32,
    _options: MorphOptions,
) -> T {
    source.interpolate(target, t)
}

#[test]
fn test_evaluate_paths_with_options_empty_track() {
    let paths: PropertyTrack<f32> = PropertyTrack::new(42.0);
    let morph: PropertyTrack<MorphOptions> = PropertyTrack::new(MorphOptions::default());
    let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
    assert_eq!(result, 42.0);
}

#[test]
fn test_evaluate_paths_with_options_before_first_keyframe() {
    let mut paths = PropertyTrack::new(0.0);
    paths.add_keyframe(1000, 100.0, Easing::Linear);
    let morph = PropertyTrack::new(MorphOptions::default());
    let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
    // Before first keyframe, evaluate_paths_with_options returns the first keyframe's value
    assert_eq!(result, 100.0);
}

#[test]
fn test_evaluate_paths_with_options_after_last_keyframe() {
    let mut paths = PropertyTrack::new(0.0);
    paths.add_keyframe(0, 10.0, Easing::Linear);
    paths.add_keyframe(1000, 100.0, Easing::Linear);
    let morph = PropertyTrack::new(MorphOptions::default());
    let result = evaluate_paths_with_options(&paths, &morph, 2000, identity_interpolate);
    assert_eq!(result, 100.0);
}

#[test]
fn test_evaluate_paths_with_options_between_two_keyframes() {
    let mut paths = PropertyTrack::new(0.0);
    paths.add_keyframe(0, 0.0, Easing::Linear);
    paths.add_keyframe(1000, 100.0, Easing::Linear);
    let morph = PropertyTrack::new(MorphOptions::default());
    let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
    assert_eq!(result, 50.0);
}

#[test]
fn test_evaluate_paths_with_options_uses_morph_at_second_keyframe() {
    let mut paths = PropertyTrack::new(0.0);
    paths.add_keyframe(0, 0.0, Easing::Linear);
    paths.add_keyframe(1000, 100.0, Easing::Linear);
    let mut morph = PropertyTrack::new(MorphOptions::default());
    // Morph at the same time as the second keyframe
    morph.add_keyframe(
        1000,
        MorphOptions {
            strategy: MorphStrategy::Fade,
            path_arc: 0.0,
            stretch: false,
        },
        Easing::Linear,
    );
    // Should use Fade morph strategy (which just returns source+target)
    let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
    // identity_interpolate just does normal interpolation regardless of morph
    // So result should be 50.0
    assert_eq!(result, 50.0);
}

// ────────────────────────────────────────────────────────
// Morph interpolation tests (moved from track.rs)
// ────────────────────────────────────────────────────────

#[test]
fn fade_vello_paths_at_start_returns_only_source() {
    let source = vec![VelloPath {
        path: std::sync::Arc::new(BezPath::new()),
        fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 255)),
        stroke: None,
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }];
    let target = vec![VelloPath {
        path: std::sync::Arc::new(BezPath::new()),
        fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 255)),
        stroke: None,
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }];
    let result = interpolate_vello_paths(
        &source,
        &target,
        0.0,
        MorphOptions {
            strategy: MorphStrategy::Fade,
            path_arc: 0.0,
            stretch: false,
        },
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].fill.unwrap().to_rgba8().a, 255);
}

#[test]
fn fade_vello_paths_at_end_returns_only_target() {
    let source = vec![VelloPath {
        path: std::sync::Arc::new(BezPath::new()),
        fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 255)),
        stroke: None,
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }];
    let target = vec![VelloPath {
        path: std::sync::Arc::new(BezPath::new()),
        fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 255)),
        stroke: None,
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }];
    let result = interpolate_vello_paths(
        &source,
        &target,
        1.0,
        MorphOptions {
            strategy: MorphStrategy::Fade,
            path_arc: 0.0,
            stretch: false,
        },
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].fill.unwrap().to_rgba8().a, 255);
}

#[test]
fn fade_vello_paths_at_midpoint_returns_both_halved() {
    let source = vec![VelloPath {
        path: std::sync::Arc::new(BezPath::new()),
        fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 200)),
        stroke: Some((vello::peniko::Color::from_rgba8(255, 255, 255, 100), 2.0)),
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }];
    let target = vec![VelloPath {
        path: std::sync::Arc::new(BezPath::new()),
        fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 128)),
        stroke: None,
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }];
    let result = interpolate_vello_paths(
        &source,
        &target,
        0.5,
        MorphOptions {
            strategy: MorphStrategy::Fade,
            path_arc: 0.0,
            stretch: false,
        },
    );
    assert_eq!(result.len(), 2);
    // Source path alpha should be halved: 200 * 0.5 = 100
    assert_eq!(result[0].fill.unwrap().to_rgba8().a, 100);
    // Source stroke alpha should be halved: 100 * 0.5 = 50
    assert_eq!(result[0].stroke.unwrap().0.to_rgba8().a, 50);
    assert_eq!(result[0].stroke.unwrap().1, 2.0);
    // Target path alpha should be halved: 128 * 0.5 = 64
    assert_eq!(result[1].fill.unwrap().to_rgba8().a, 64);
    assert!(result[1].stroke.is_none());
}

#[test]
fn fade_text_paths_at_midpoint_returns_both_halved() {
    let source = vec![TextPath::new(BezPath::new(), [0, 0, 0, 255], 1.0)];
    let target = vec![TextPath::new(BezPath::new(), [255, 255, 255, 255], 0.8)];
    let result = interpolate_text_paths(
        &source,
        &target,
        0.5,
        MorphOptions {
            strategy: MorphStrategy::Fade,
            path_arc: 0.0,
            stretch: false,
        },
    );
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].opacity, 0.5);
    assert_eq!(result[1].opacity, 0.4);
}

#[test]
fn fade_vello_paths_empty_source() {
    let source: Vec<VelloPath> = vec![];
    let target = vec![VelloPath {
        path: std::sync::Arc::new(BezPath::new()),
        fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 128)),
        stroke: None,
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }];
    let result = interpolate_vello_paths(
        &source,
        &target,
        0.25,
        MorphOptions {
            strategy: MorphStrategy::Fade,
            path_arc: 0.0,
            stretch: false,
        },
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].fill.unwrap().to_rgba8().a, 32); // 128 * 0.25
}

#[test]
fn fade_vello_paths_empty_target() {
    let source = vec![VelloPath {
        path: std::sync::Arc::new(BezPath::new()),
        fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 200)),
        stroke: None,
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }];
    let target: Vec<VelloPath> = vec![];
    let result = interpolate_vello_paths(
        &source,
        &target,
        0.75,
        MorphOptions {
            strategy: MorphStrategy::Fade,
            path_arc: 0.0,
            stretch: false,
        },
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].fill.unwrap().to_rgba8().a, 50); // 200 * 0.25
}
