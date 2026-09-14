//! Authored `solo` semantics: while any actor declares `solo: true`, only
//! soloed actors draw. Unlike `visible`, the hiding is recursive — a non-solo
//! subtree is pruned whole — while ancestors of a soloed actor stay
//! traversable so a soloed descendant of a container still renders.

use super::*;

/// Parse an `.amx` snippet, build the timeline, and panic on parse errors.
fn build_from_source(source: &str) -> Timeline {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "build diagnostics: {:?}", report.diagnostics);
    report.output
}

/// Evaluate one frame on the observable path and return the labels that drew.
fn drew(source: &str, time_s: f64) -> std::collections::BTreeSet<String> {
    let timeline = build_from_source(source);
    let dimensions = SceneDimensions {
        width: 320,
        height: 180,
    };
    let mut backend: Option<&mut dyn crate::timeline::effects::FilterBackend> = None;
    let program = timeline.evaluate_program_with_debug(
        time_s,
        dimensions,
        DebugRenderOptions::default(),
        &mut backend,
    );
    program.precise_bounds.keys().cloned().collect()
}

const THREE_BOXES: &str = r#"
#0s
a: Rect, at: (60, 90), size: (40, 40), color: (1, 0, 0, 1)
b: Rect, at: (160, 90), size: (40, 40), color: (0, 1, 0, 1)
c: Rect, at: (260, 90), size: (40, 40), color: (0, 0, 1, 1), solo: true
"#;

#[test]
fn without_solo_every_actor_draws() {
    let plain = THREE_BOXES.replace(", solo: true", "");
    assert_eq!(
        drew(&plain, 1.0),
        ["a", "b", "c"].into_iter().map(String::from).collect(),
        "an unauthored solo flag must not hide anything"
    );
}

#[test]
fn solo_hides_every_other_root_actor() {
    assert_eq!(
        drew(THREE_BOXES, 1.0),
        ["c"].into_iter().map(String::from).collect(),
        "only the soloed actor draws"
    );
}

#[test]
fn solo_hides_the_siblings_inside_its_own_container() {
    let source = r#"
#0s
g: Group, anchor: scene.center {
  x: Rect, at: (-60, 0), size: (40, 40), color: (1, 0, 0, 1)
  y: Rect, at: (60, 0), size: (40, 40), color: (0, 0, 1, 1), solo: true
}

z: Rect, at: (280, 90), size: (40, 40), color: (0, 1, 0, 1)
"#;
    assert_eq!(
        drew(source, 1.0),
        ["y"].into_iter().map(String::from).collect(),
        "the soloed child draws; its sibling and unrelated roots stay hidden"
    );
}

#[test]
fn solo_prunes_whole_subtrees_recursively() {
    // `g` contains a drawable child and is not soloed, so the whole subtree
    // must vanish — `visible: false` on the container would not do this.
    let source = r#"
#0s
g: Group, anchor: scene.center {
  inner: Rect, at: (0, 0), size: (40, 40), color: (1, 0, 0, 1)
}

keep: Rect, at: (280, 90), size: (40, 40), color: (0, 1, 0, 1), solo: true
"#;
    assert_eq!(
        drew(source, 1.0),
        ["keep"].into_iter().map(String::from).collect(),
        "a non-solo container must not keep drawing its children"
    );
}

#[test]
fn soloed_actor_under_a_container_is_still_reached() {
    // The container is not soloed, but traversal must reach the soloed child
    // (regression guard for pruning the ancestor as well).
    let source = r#"
#0s
outer: Group, anchor: scene.center {
  wrapper: Group {
    inner: Rect, at: (0, 0), size: (40, 40), color: (0, 0, 1, 1), solo: true
  }
  other: Rect, at: (-90, 0), size: (40, 40), color: (1, 0, 0, 1)
}

z: Rect, at: (280, 90), size: (40, 40), color: (0, 1, 0, 1)
"#;
    assert_eq!(
        drew(source, 1.0),
        ["inner"].into_iter().map(String::from).collect(),
        "a deeply nested soloed actor still draws, at its ancestor's transform"
    );
}

#[test]
fn hidden_actor_neither_draws_nor_suppresses_others() {
    // `visible: false` wins over `solo`: the hidden actor stays hidden and does
    // not put the rest of the scene into solo mode.
    let source = r#"
#0s
a: Rect, at: (60, 90), size: (40, 40), color: (1, 0, 0, 1)
b: Rect, at: (160, 90), size: (40, 40), color: (0, 1, 0, 1), solo: true
"#;
    let mut timeline = build_from_source(source);
    timeline.tracks.get_mut("b").expect("track b").visible = false;
    let dimensions = SceneDimensions {
        width: 320,
        height: 180,
    };
    let mut backend: Option<&mut dyn crate::timeline::effects::FilterBackend> = None;
    let program = timeline.evaluate_program_with_debug(
        1.0,
        dimensions,
        DebugRenderOptions::default(),
        &mut backend,
    );
    let drawn: std::collections::BTreeSet<String> =
        program.precise_bounds.keys().cloned().collect();
    assert_eq!(
        drawn,
        ["a"].into_iter().map(String::from).collect(),
        "a hidden actor neither draws nor suppresses the scene"
    );
}

/// `SoloState` is resolved per frame and is deliberately *not* part of the
/// frame-cache key, so correctness depends on `solo` never varying with time:
/// it must stay out of the animatable set. If this test fails, whoever made
/// `solo` animatable must also add it to the frame-cache key (and to the
/// static-subtree key), or cached frames will disagree with the solo state.
#[test]
fn solo_is_not_animatable() {
    let schema = crate::timeline::property_registry::lookup_property("solo")
        .expect("`solo` must stay registered");
    assert!(
        !schema
            .flags
            .contains(crate::timeline::property_registry::PropertyFlags::ANIMATED),
        "`solo` became animatable: add it to the frame-cache key before shipping this"
    );
}

/// The flag must remain assignable, or authored `solo: true` would stop being
/// read from source.
#[test]
fn solo_is_assignable() {
    let schema = crate::timeline::property_registry::lookup_property("solo")
        .expect("`solo` must stay registered");
    assert!(
        schema
            .flags
            .contains(crate::timeline::property_registry::PropertyFlags::ASSIGNABLE),
        "`solo` must stay assignable from source"
    );
}

/// `solo` is not animatable, so an `always` block cannot drive it per frame.
/// The write used to be dropped in silence; it now reports
/// `always-write-not-animatable` and leaves the frame ungated.
#[test]
fn always_written_solo_is_reported_not_silently_dropped() {
    let source = r#"
#0s
a: Rect, at: (60, 90), size: (40, 40), color: (1, 0, 0, 1)
b: Rect, at: (160, 90), size: (40, 40), color: (0, 1, 0, 1)

always {
  a.solo = true
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let report = Timeline::build_with_diagnostics(
        &ast.expect("parsed AST"),
        &std::collections::HashMap::new(),
    );
    assert!(
        report.diagnostics.iter().any(|d| matches!(
            d.code,
            animatix_syntax::diagnostics::DiagnosticCode::AlwaysWriteNotAnimatable
        )),
        "an ignored per-frame write must be reported: {:?}",
        report.diagnostics
    );
    assert!(
        report.diagnostics.iter().any(|d| d.message.contains("solo")),
        "the diagnostic must name the property"
    );
    // Both actors still draw: the ignored write did not gate the frame.
    let mut backend: Option<&mut dyn crate::timeline::effects::FilterBackend> = None;
    let program = report.output.evaluate_program_with_debug(
        1.0,
        SceneDimensions {
            width: 320,
            height: 180,
        },
        DebugRenderOptions::default(),
        &mut backend,
    );
    assert_eq!(program.precise_bounds.len(), 2, "an ignored write must not hide anything");
}
