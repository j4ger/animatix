//! Declaration-tail regressions: the prepared `size` spec and the pre-spawn
//! `from`/`to` actor-anchor side channel.

use super::super::tests::without_content_lints;
use super::support::{build, build_clean};
use crate::timeline::taffy_layout::{ChildSizeSpec, SizeSpec};
use crate::timeline::{SceneAnchor, Timeline};

fn size_spec(timeline: &Timeline, label: &str) -> ChildSizeSpec {
    let id = timeline.world.tracks.get_id(label).expect("track should exist");
    timeline
        .world
        .geometry(id)
        .size_spec
        .expect("size declaration should set a layout spec")
}

/// The layout spec is parsed once in the prepare pass and applied from there:
/// a literal pair stays fixed, a percent keeps its share, and numeric keyframes
/// keep the matching half-size.
#[test]
fn prepared_size_decl_keeps_fixed_percent_and_numeric_semantics() {
    let timeline = build_clean(
        r#"
        fixed: Rect, size: (120, 80)
        percent: Rect, size: (50%, 40)
        "#,
    );

    assert_eq!(
        size_spec(&timeline, "fixed"),
        ChildSizeSpec::from_parts(SizeSpec::Fixed(120.0), SizeSpec::Fixed(80.0))
    );
    let fixed_id = timeline.world.tracks.get_id("fixed").expect("fixed id");
    assert_eq!(
        timeline
            .world
            .geometry(fixed_id)
            .size
            .as_ref()
            .expect("size keyframe")
            .evaluate(0),
        [60.0, 40.0],
        "the numeric half-size still feeds the size keyframes"
    );

    assert_eq!(
        size_spec(&timeline, "percent"),
        ChildSizeSpec::from_parts(SizeSpec::Percent(0.5), SizeSpec::Fixed(40.0))
    );
}

/// Keyword diagnostics are emitted by the single apply step; a second parse in
/// the declaration tail would duplicate these warnings.
#[test]
fn root_fill_uses_canvas_and_non_container_auto_warns_once() {
    let report = build(
        r#"
        auto_box: Rect, size: auto
        fill_box: Rect, size: fill
        "#,
    );
    let auto_warnings: Vec<_> = without_content_lints(&report.diagnostics)
        .filter(|d| d.message.contains("auto/fit sizing only applies to layout containers"))
        .collect();
    assert_eq!(
        auto_warnings.len(),
        1,
        "auto on a non-container must warn exactly once, got: {:?}",
        report.diagnostics
    );
    assert_eq!(auto_warnings[0].location.subject.as_deref(), Some("auto_box"));

    let fill_warnings: Vec<_> = without_content_lints(&report.diagnostics)
        .filter(|d| d.message.contains("fill on top-level actor"))
        .collect();
    assert!(fill_warnings.is_empty());
    let fill_id = report.output.world.get_id("fill_box").expect("fill id");
    assert_eq!(
        report
            .output
            .world
            .geometry(fill_id)
            .size
            .as_ref()
            .expect("fill size")
            .evaluate(0),
        [960.0, 50.0],
    );

    assert_eq!(
        size_spec(&report.output, "auto_box"),
        ChildSizeSpec::from_parts(SizeSpec::Auto, SizeSpec::Auto)
    );
    assert_eq!(
        size_spec(&report.output, "fill_box"),
        ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Auto)
    );
}

#[test]
fn line_anchor_refs_bind_on_the_first_declaration() {
    let timeline = build_clean(
        r#"
        link: Line, from: a.top, to: a.bottom
        a: Rect, size: (40, 40), at: (100, 100), opacity: 1
        "#,
    );
    let link = timeline.world.get_id("link").expect("link");
    let actor = timeline.world.get_id("a").expect("target");
    let shape = timeline.world.shape(link).expect("shape");
    for (track, anchor) in [
        (&shape.from_anchor, SceneAnchor::Top),
        (&shape.to_anchor, SceneAnchor::Bottom),
    ] {
        let track = track.as_ref().expect("point track");
        assert!(matches!(
            track.evaluate(0),
            crate::timeline::actor_points::track::PointValue::Actor {
                id: Some(id), point: crate::timeline::ActorPoint::Bounds(point), ..
            } if id == actor && point == anchor
        ));
    }
}
