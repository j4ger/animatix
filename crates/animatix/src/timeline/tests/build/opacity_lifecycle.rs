//! Declaration-opacity lifecycle: first declaration, re-declaration and an
//! explicit `opacity` for both the generic actor path and the plot path.
//!
//! The rule is one rule for both paths: a first declaration with no explicit
//! opacity starts at `default_opacity` (0 before any keyframe); a later
//! declaration with no explicit opacity keeps the last authored value; an
//! explicit opacity always wins.

use super::support::build_clean;

fn opacity_at(timeline: &crate::timeline::Timeline, label: &str, time_ms: u64) -> f32 {
    let id = timeline.world.tracks.get_id(label).expect("track should exist");
    timeline
        .world
        .style(id)
        .opacity
        .as_ref()
        .expect("opacity track")
        .evaluate(time_ms)
}

#[test]
fn generic_first_declaration_without_opacity_is_hidden_by_default() {
    let timeline =
        build_clean(r#"box: Rect, size: (100, 100), color: accent.primary, at: (200, 150)"#);
    assert_eq!(opacity_at(&timeline, "box", 0), 0.0);
}

#[test]
fn generic_redeclaration_without_opacity_keeps_the_last_value() {
    let timeline = build_clean(
        r#"
        box: Rect, size: (100, 100), opacity: 0.4, at: (200, 150)
        #1s
        box: Rect, size: (100, 100), at: (200, 150)
        "#,
    );
    assert_eq!(
        opacity_at(&timeline, "box", 0),
        0.4,
        "the re-declaration must not reset an authored opacity"
    );
    assert_eq!(opacity_at(&timeline, "box", 1000), 0.4);
}

#[test]
fn generic_redeclaration_with_explicit_opacity_uses_the_authored_value() {
    let timeline = build_clean(
        r#"
        box: Rect, size: (100, 100), opacity: 0.4, at: (200, 150)
        #1s
        box: Rect, size: (100, 100), opacity: 0.8, at: (200, 150)
        "#,
    );
    assert_eq!(opacity_at(&timeline, "box", 0), 0.4);
    assert_eq!(opacity_at(&timeline, "box", 1000), 0.8);
}

#[test]
fn plot_first_declaration_without_opacity_is_hidden_by_default() {
    let timeline = build_clean(
        r#"g: Graph, x_domain: (-4, 4), y_domain: (-2, 18), size: (380, 280), at: (280, 200)"#,
    );
    assert_eq!(opacity_at(&timeline, "g", 0), 0.0);
}

#[test]
fn plot_redeclaration_without_opacity_keeps_the_last_value() {
    let timeline = build_clean(
        r#"
        g: Graph, x_domain: (-4, 4), y_domain: (-2, 18), size: (380, 280), opacity: 0.35, at: (280, 200)
        #1s
        g: Graph, x_domain: (-4, 4), y_domain: (-2, 18), size: (380, 280), at: (280, 200)
        "#,
    );
    assert_eq!(
        opacity_at(&timeline, "g", 0),
        0.35,
        "the plot re-declaration must not snap opacity back to 1.0"
    );
    assert_eq!(opacity_at(&timeline, "g", 1000), 0.35);
}

#[test]
fn plot_redeclaration_with_explicit_opacity_uses_the_authored_value() {
    let timeline = build_clean(
        r#"
        g: Graph, x_domain: (-4, 4), y_domain: (-2, 18), size: (380, 280), opacity: 0.35, at: (280, 200)
        #1s
        g: Graph, x_domain: (-4, 4), y_domain: (-2, 18), size: (380, 280), opacity: 0.6, at: (280, 200)
        "#,
    );
    assert_eq!(opacity_at(&timeline, "g", 0), 0.35);
    assert_eq!(opacity_at(&timeline, "g", 1000), 0.6);
}
