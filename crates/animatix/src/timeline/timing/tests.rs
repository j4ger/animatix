use super::parsing::*;
use super::stagger::*;
use super::*;

#[test]
fn default_easing_follows_the_statements_role() {
    // Arrivals decelerate, departures accelerate, repositioning does both.
    assert_eq!(default_easing_for(ModifierHost::Action, Some("fade-in")), Easing::ExpoOut);
    assert_eq!(default_easing_for(ModifierHost::Action, Some("settle-in")), Easing::ExpoOut);
    assert_eq!(default_easing_for(ModifierHost::Action, Some("pop-in")), Easing::ExpoOut);
    assert_eq!(default_easing_for(ModifierHost::Action, Some("fade-out")), Easing::EaseIn);
    assert_eq!(default_easing_for(ModifierHost::Action, Some("remove")), Easing::EaseIn);
    assert_eq!(default_easing_for(ModifierHost::Action, Some("move")), Easing::EaseInOut);
    assert_eq!(
        default_easing_for(ModifierHost::Assignment, Some("x.y")),
        Easing::Linear,
        "assignments keep the old default: an action that expands into them \
             already carries its own timing"
    );
    // Oscillating effects are continuous loops; easing them softens the
    // effect itself.
    assert_eq!(default_easing_for(ModifierHost::Action, Some("shake")), Easing::Linear);
    assert_eq!(default_easing_for(ModifierHost::Action, Some("bounce")), Easing::Linear);
}

#[test]
fn an_explicit_ease_always_wins_over_the_role_default() {
    let ease = |mods: &[Modifier], host, subject| {
        let mut diagnostics = Vec::new();
        let parsed = parse_timing_modifiers(mods, host, subject, &mut diagnostics);
        assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
        parsed
    };
    // A duration is what makes a statement timed; an uneased instant write
    // has no segment to ease.
    let timed = |name: Option<&str>, value: Expr| Modifier {
        name: name.map(str::to_string),
        value,
    };
    let parsed = ease(
        &[timed(None, Expr::Ident("500ms".to_string()))],
        ModifierHost::Action,
        Some("fade-in"),
    );
    assert_eq!(parsed.easing, Easing::ExpoOut, "uneased entrance takes the default");
    assert!(!parsed.ease_authored);
    assert_eq!(
        ease(&[], ModifierHost::Action, Some("fade-in")).easing,
        Easing::Linear,
        "an undated write has no segment to ease"
    );

    let linear = ease(
        &[
            timed(None, Expr::Ident("500ms".to_string())),
            timed(Some("ease"), Expr::Ident("linear".to_string())),
        ],
        ModifierHost::Action,
        Some("fade-in"),
    );
    assert_eq!(linear.easing, Easing::Linear, "`ease: linear` must still mean linear");
    assert!(linear.ease_authored, "the author's intent must be visible to presets");

    let expo = ease(
        &[
            timed(None, Expr::Ident("500ms".to_string())),
            timed(Some("ease"), Expr::Ident("expo-in-out".to_string())),
        ],
        ModifierHost::Assignment,
        Some("box.at"),
    );
    assert_eq!(expo.easing, Easing::ExpoInOut, "an authored curve always wins");
}

#[test]
fn an_uneased_entrance_lands_the_eased_curve_on_its_keyframe() {
    // The default is only real if it reaches the track, not just the parser.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

box: Rect, size: (100, 100), at: (200, 180), color: accent.primary
#0s
fade-in box [500ms]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;
    let track = timeline.world.tracks.get("box").expect("box track");
    let easing = track
        .field_ref(crate::timeline::ActorField::Opacity)
        .and_then(|f| f.keyframe_easing(500));
    assert_eq!(easing, Some(Easing::ExpoOut), "the entrance keyframe must carry the default");
}
