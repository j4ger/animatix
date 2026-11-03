//! Build tests for structural `if` / `match` blocks.
//!
//! A top-level conditional must lower only the branch its build-time condition
//! selects. Before the walk handled `Stmt::Conditional`, these declarations
//! parsed, checked clean, and produced no actors at all.

use super::support::build;

#[test]
fn conditional_actor_declaration_instantiates_the_taken_branch() {
    let report = build(
        r#"
let n = 2
config { resolution: (640, 360) }
always { }
if n == 2 {
  a: Rect, size: (100, 100), color: accent.primary, at: (320, 180), opacity: 1.0
}
"#,
    );
    assert!(
        report.diagnostics.iter().all(|d| !d.is_error()),
        "unexpected build errors: {:?}",
        report.diagnostics
    );
    assert!(report.output.has_actor("a"), "the taken `if` branch must declare `a`");
}

#[test]
fn conditional_actor_declaration_skips_the_untaken_branch() {
    let report = build(
        r#"
let n = 1
config { resolution: (640, 360) }
if n == 2 {
  a: Rect, size: (100, 100), color: accent.primary, at: (320, 180), opacity: 1.0
} else {
  b: Rect, size: (100, 100), color: accent.primary, at: (320, 180), opacity: 1.0
}
"#,
    );
    assert!(!report.output.has_actor("a"), "the untaken `if` branch must stay out");
    assert!(report.output.has_actor("b"), "the taken `else` branch must declare `b`");
}

#[test]
fn conditional_for_loop_instantiates_array_actors() {
    let report = build(
        r#"
let banks = 2
config { resolution: (640, 360) }
always { }
if banks == 2 {
  for i in {0, 1} {
    lane[i]: Rect, size: (80, 80), color: accent.primary,
      at: (200 + i * 120, 180), opacity: 1.0
  }
}
"#,
    );
    assert!(
        report.output.has_actor("lane__0") && report.output.has_actor("lane__1"),
        "both loop iterations must become array actors: {:?}",
        report.output.actor_labels().collect::<Vec<_>>()
    );
}
