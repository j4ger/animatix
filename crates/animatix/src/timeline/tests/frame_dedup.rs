//! The content epoch a rendered frame is keyed on: it must move whenever the
//! document moves, and must not move when it is merely read.

use super::*;

fn build_from_source(source: &str) -> Timeline {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error),
        "build errors: {:?}",
        report.diagnostics
    );
    report.output
}

const SCENE: &str = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
box: Rect, size: (200, 100), at: (320, 180), color: accent.primary
"#;

#[test]
fn reading_the_epoch_does_not_move_it() {
    let timeline = build_from_source(SCENE);
    assert_eq!(timeline.content_epoch(), timeline.content_epoch());
}

#[test]
fn mutable_access_to_a_track_moves_the_epoch() {
    let mut timeline = build_from_source(SCENE);
    let before = timeline.content_epoch();
    // Every public mutable accessor funnels through `invalidate_frame_cache`,
    // which is the single point the epoch is bumped from — so an edit made in
    // place cannot leave a frame dedup serving the pre-edit pixels.
    assert!(
        timeline.get_track_mut("box").is_some(),
        "the fixture did not build a track named `box`"
    );
    assert_eq!(timeline.content_epoch(), before + 1, "a mutation funnel did not move the epoch");
}

#[test]
fn a_fresh_document_restarts_the_epoch_at_zero() {
    // Not a defect, a constraint: the epoch counts a document's *edits*, not its
    // content, so two different scenes both start at 0. That is why
    // `FrameSignature` carries a driver-side document generation beside it — a
    // driver that swaps documents without bumping that would dedup a frame it
    // no longer has.
    let first = build_from_source(SCENE);
    let second = build_from_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
dot: Rect, size: (40, 40), at: (320, 180), color: accent.warning
"#,
    );
    assert_eq!(first.content_epoch(), 0);
    assert_eq!(second.content_epoch(), 0);
}
