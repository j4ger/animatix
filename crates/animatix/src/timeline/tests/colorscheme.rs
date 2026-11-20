use super::support::build;

#[test]
fn test_colorscheme_primitive_declaration() {
    let timeline = build(
        r#"
let test_scheme = Colorscheme {
    scene.background: hex_color("000000"),
    text.primary: (0.9, 0.95, 1.0),
}
config { colorscheme: "test_scheme" }
"#,
    )
    .output;

    assert_eq!(timeline.colorscheme.name, "test_scheme");
    assert_eq!(timeline.colorscheme.color("scene.background"), Some([0.0, 0.0, 0.0, 1.0]));
    assert_eq!(timeline.colorscheme.color("text.primary"), Some([0.9, 0.95, 1.0, 1.0]));
}

#[test]
fn test_colorscheme_let_declaration() {
    let timeline = build(
        r#"
let test_scheme_let = Colorscheme {
    scene.background: (0.15, 0.25, 0.35),
    text.primary: (0.85, 0.9, 0.95),
}
config { colorscheme: "test_scheme_let" }
"#,
    )
    .output;

    assert_eq!(timeline.colorscheme.name, "test_scheme_let");
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
let auto_test = Colorscheme {
    auto: ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0)),
}
config { colorscheme: "auto_test" }
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
