use super::support::build;

#[test]
fn test_colorscheme_primitive_declaration() {
    let timeline = build(
        r#"
let test-scheme = Colorscheme {
    scene.background: (0.1, 0.2, 0.3),
    text.primary: (0.9, 0.95, 1.0),
}
config { colorscheme: "test-scheme" }
"#,
    )
    .output;

    assert_eq!(timeline.colorscheme.name, "test-scheme");
    assert_eq!(timeline.colorscheme.color("scene.background"), Some([0.1, 0.2, 0.3, 1.0]));
    assert_eq!(timeline.colorscheme.color("text.primary"), Some([0.9, 0.95, 1.0, 1.0]));
}

#[test]
fn test_colorscheme_let_declaration() {
    let timeline = build(
        r#"
let test-scheme-let = Colorscheme {
    scene.background: (0.15, 0.25, 0.35),
    text.primary: (0.85, 0.9, 0.95),
}
config { colorscheme: "test-scheme-let" }
"#,
    )
    .output;

    assert_eq!(timeline.colorscheme.name, "test-scheme-let");
    assert_eq!(timeline.colorscheme.color("scene.background"), Some([0.15, 0.25, 0.35, 1.0]));
    assert_eq!(timeline.colorscheme.color("text.primary"), Some([0.85, 0.9, 0.95, 1.0]));
}

#[test]
fn test_colorscheme_inheritance() {
    let timeline = build(
        r#"
let child = Colorscheme {
    extends: "default-dark",
    scene.background: (0.5, 0.5, 0.5),
}
config { colorscheme: "child" }
"#,
    )
    .output;

    assert_eq!(timeline.colorscheme.name, "child");
    assert_eq!(timeline.colorscheme.color("scene.background"), Some([0.5, 0.5, 0.5, 1.0]));
    assert_eq!(timeline.colorscheme.color("text.primary"), Some([1.0, 1.0, 1.0, 1.0]));
}

#[test]
fn test_colorscheme_auto_cycle() {
    let mut timeline = build(
        r#"
let auto-test = Colorscheme {
    auto: ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0)),
}
config { colorscheme: "auto-test" }
a: Ellipse, color: auto
b: Ellipse, color: auto
"#,
    )
    .output;

    let color_a = timeline.auto_color_for_label("a");
    let color_b = timeline.auto_color_for_label("b");

    assert_eq!(color_a, Some([1.0, 0.0, 0.0, 1.0]));
    assert_eq!(color_b, Some([0.0, 1.0, 0.0, 1.0]));
}
