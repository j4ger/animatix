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
fn size_keyword_warns_once_and_keeps_keyword_semantics() {
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
    assert_eq!(
        fill_warnings.len(),
        1,
        "top-level fill must warn exactly once, got: {:?}",
        report.diagnostics
    );
    assert_eq!(fill_warnings[0].location.subject.as_deref(), Some("fill_box"));

    assert_eq!(
        size_spec(&report.output, "auto_box"),
        ChildSizeSpec::from_parts(SizeSpec::Auto, SizeSpec::Auto)
    );
    assert_eq!(
        size_spec(&report.output, "fill_box"),
        ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Auto)
    );
}

/// Pre-spawn anchor refs only write to a track that already exists. A first
/// declaration has no track before `begin_actor_decl` spawns one, so it must
/// not create the side channel just to carry the ref; a re-declaration does
/// write it.
#[test]
fn line_anchor_refs_are_pre_spawn_and_need_an_existing_track() {
    let timeline = build_clean(
        r#"
        a: Rect, size: (40, 40), at: (100, 100)
        first: Line, from: a.top, to: a.bottom
        "#,
    );
    let first_id = timeline.world.tracks.get_id("first").expect("first id");
    let shape = timeline.world.shape.get(first_id).expect("first shape");
    assert!(
        shape.from_anchor.is_none() && shape.to_anchor.is_none(),
        "a first declaration has no pre-spawn track to write anchor refs to"
    );
    let timeline = build_clean(
        r#"
        a: Rect, size: (40, 40), at: (100, 100)
        link: Line, from: (0, 0), to: (100, 0)
        #1s
        link: Line, from: a.top, to: a.bottom
        "#,
    );
    let link_id = timeline.world.tracks.get_id("link").expect("link id");
    let shape = timeline.world.shape.get(link_id).expect("link shape");
    assert_eq!(
        shape.from_anchor,
        Some(("a".to_string(), SceneAnchor::Top)),
        "a re-declaration writes the pre-spawn anchor side channel"
    );
    assert_eq!(
        shape.to_anchor,
        Some(("a".to_string(), SceneAnchor::Bottom)),
        "a re-declaration writes the pre-spawn anchor side channel"
    );
}
