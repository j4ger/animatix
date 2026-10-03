//! `ease:` must survive parse → source → parse.
//!
//! The parser lifts an easing out of the modifier list into a typed field on
//! the assignment, and the serializer used to print only the modifier list —
//! so every `ease:` on a property assignment vanished the first time a file was
//! formatted. Statement-count roundtrip tests cannot see that; this can.

use animatix_syntax::ast::{Expr, Stmt};
use animatix_syntax::easing::Easing;

fn parse(src: &str) -> Vec<Stmt> {
    let (stmts, errors) = animatix_syntax::parser::parse_source(src);
    assert!(
        errors.is_empty(),
        "parse errors: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    stmts.expect("no statements")
}

/// The easing carried by the first property assignment in the tree.
fn first_easing(stmts: &[Stmt]) -> Option<Easing> {
    for stmt in stmts {
        match stmt {
            Stmt::Assignment { easing, .. } => return *easing,
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                if let Some(e) = first_easing(body) {
                    return Some(e);
                }
            },
            Stmt::Stagger { body, .. } | Stmt::Sequence { body, .. } => {
                if let Some(e) = first_easing(body) {
                    return Some(e);
                }
            },
            _ => {},
        }
    }
    None
}

fn source_with(ease: &str) -> String {
    format!(
        "config {{ resolution: (400, 400), duration: 2 }}\nd: Rect, size: (40, 40), color: accent.primary, at: (60, 200)\n#0.2s\nd.at = (340, 200) [1.2s, ease: {ease}]\n"
    )
}

#[test]
fn parameterized_eases_parse_to_their_curves() {
    let cases: &[(&str, Easing)] = &[
        ("cubic-bezier(0.16, 1, 0.3, 1)", Easing::CubicBezier([0.16, 1.0, 0.3, 1.0])),
        (
            "cubic-bezier(0.42, -0.3, 0.58, 1.2)",
            Easing::CubicBezier([0.42, -0.3, 0.58, 1.2]),
        ),
        (
            "spring(6, 9)",
            Easing::Spring {
                damping: 6.0,
                frequency: 9.0,
            },
        ),
        ("expo-out", Easing::ExpoOut),
        ("expo-in-out", Easing::ExpoInOut),
        // `bounce` stays accepted as the legacy spelling of the CSS curve.
        ("bounce", Easing::Bounce),
        ("bounce-in", Easing::Bounce),
    ];
    for (written, expected) in cases {
        let parsed = first_easing(&parse(&source_with(written)));
        assert_eq!(parsed, Some(*expected), "ease: {written} did not resolve");
    }
}

#[test]
fn eases_survive_the_source_roundtrip() {
    for written in [
        "cubic-bezier(0.16, 1, 0.3, 1)",
        "spring(6, 9)",
        "expo-out",
        "ease-in-out",
    ] {
        let original = first_easing(&parse(&source_with(written)));
        let serialized = animatix_syntax::to_source::stmts_to_source(&parse(&source_with(written)));
        let reparsed = first_easing(&parse(&serialized));
        assert_eq!(
            original, reparsed,
            "ease: {written} did not survive serialization; serializer emitted:\n{serialized}"
        );
    }
}

#[test]
fn a_wrong_arity_bezier_is_not_silently_accepted() {
    // Three control points must not become an easing — the build layer is the
    // one that reports it, so the parser has to leave the modifier in place.
    let stmts = parse(&source_with("cubic-bezier(0.16, 1, 0.3)"));
    assert_eq!(first_easing(&stmts), None, "a 3-point bezier resolved to something");
    let kept = stmts.iter().any(|s| match s {
        Stmt::Assignment { modifiers, .. } => modifiers.iter().any(|m| {
            m.name.as_deref() == Some("ease")
                && matches!(&m.value, Expr::Call(name, args) if name == "cubic-bezier" && args.len() == 3)
        }),
        Stmt::Keyframe { body, .. } => body.iter().any(|s| match s {
            Stmt::Assignment { modifiers, .. } => modifiers.iter().any(|m| {
                m.name.as_deref() == Some("ease")
            }),
            _ => false,
        }),
        _ => false,
    });
    assert!(kept, "the bad ease was dropped instead of left for a diagnostic");
}

/// Recursively find the first `play` transition's easing (scene bodies nest).
fn first_transition_easing(stmts: &[Stmt]) -> Option<animatix_syntax::easing::Easing> {
    for stmt in stmts {
        match stmt {
            Stmt::Play {
                transition: Some(t), ..
            } => return Some(t.easing),
            Stmt::Scene { body, .. } => {
                if let Some(e) = first_transition_easing(body) {
                    return Some(e);
                }
            },
            _ => {},
        }
    }
    None
}

/// A `play` transition's ease used to be discarded twice over: the parser
/// hardcoded `Easing::Linear` behind a `_ => {}`, and the transition printer
/// emitted only id + duration, so `animatix fmt` deleted the authored value.
#[test]
fn a_play_transition_ease_parses_and_survives_formatting() {
    use animatix_syntax::easing::Easing;

    let doc = |ease: &str| {
        format!(
            "# A\nconfig {{ resolution: (400, 400), duration: 1 }}\nb: Rect, size: (10, 10), color: accent.primary, at: (1, 1)\n#0s\nplay B [fade, 800ms, ease: {ease}]\n# B\nconfig {{ resolution: (400, 400), duration: 1 }}\nc: Rect, size: (10, 10), color: accent.primary, at: (1, 1)\n"
        )
    };

    for (written, want) in [
        ("ease-in-out", Easing::EaseInOut),
        ("expo-out", Easing::ExpoOut),
        ("cubic-bezier(0.16, 1, 0.3, 1)", Easing::CubicBezier([0.16, 1.0, 0.3, 1.0])),
    ] {
        let parsed = first_transition_easing(&parse(&doc(written)));
        assert_eq!(parsed, Some(want), "play [{written}] did not resolve its easing");

        let serialized = animatix_syntax::to_source::stmts_to_source(&parse(&doc(written)));
        assert_eq!(
            first_transition_easing(&parse(&serialized)),
            Some(want),
            "transition ease lost in formatting; serializer emitted:\n{serialized}"
        );
    }

    // No authored ease must not start inventing one in the output.
    let plain = "# A\nconfig { resolution: (400, 400), duration: 1 }\n#0s\nplay B [fade, 800ms]\n# B\nconfig { resolution: (400, 400), duration: 1 }\n";
    let out = animatix_syntax::to_source::stmts_to_source(&parse(plain));
    assert!(!out.contains("ease:"), "a transition with no authored ease grew one:\n{out}");
}
