//! Build tests for `format()` template specifiers.

use super::support::{build, text_at};
use crate::diagnostics::DiagnosticCode;

#[test]
fn fixed_point_specifier_renders_the_number() {
    let report = build(
        r#"
config { resolution: (320, 180) }
#0s
t: Text, text: format("{:.1f}k msg/s", 184.5), at: (160, 90)
"#,
    );
    assert_eq!(text_at(&report.output, "t", 0), "184.5k msg/s");
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.code != DiagnosticCode::UnsupportedFormatSpecifier),
        "the supported `f` form must not warn: {:?}",
        report.diagnostics
    );
}

#[test]
fn precision_zero_still_renders_a_whole_number() {
    let report = build(
        r#"
config { resolution: (320, 180) }
#0s
t: Text, text: format("{:.0}", 2.6), at: (160, 90)
"#,
    );
    assert_eq!(text_at(&report.output, "t", 0), "3");
}

#[test]
fn unsupported_specifier_warns_with_the_actor_and_specifier() {
    let report = build(
        r#"
config { resolution: (320, 180) }
#0s
t: Text, text: format("{:d}", 7), at: (160, 90)
"#,
    );
    let warning = report
        .diagnostics
        .iter()
        .find(|d| d.code == DiagnosticCode::UnsupportedFormatSpecifier)
        .expect("an unsupported specifier must warn");
    assert!(
        warning.message.contains("`t`"),
        "warning should name the actor: {}",
        warning.message
    );
    assert!(
        warning.message.contains("{:d}"),
        "warning should name the specifier: {}",
        warning.message
    );
    assert_eq!(
        text_at(&report.output, "t", 0),
        "7",
        "an unsupported placeholder falls back to default formatting instead of brace garbage"
    );
}

#[test]
fn unsupported_specifier_in_an_always_block_warns_with_the_actor() {
    let report = build(
        r#"
config { resolution: (320, 180) }
t: Text, text: "0", at: (160, 90)
always {
  t.text = format("{:,}", 1000)
}
"#,
    );
    let warning = report
        .diagnostics
        .iter()
        .find(|d| d.code == DiagnosticCode::UnsupportedFormatSpecifier)
        .expect("a frame-time format() template must be checked at build time too");
    assert!(
        warning.message.contains("`t`") && warning.message.contains("{:,}"),
        "warning should name the actor and specifier: {}",
        warning.message
    );
}
