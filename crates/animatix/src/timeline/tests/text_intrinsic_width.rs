//! Intrinsic text measurements must not become authored wrapping constraints.

use super::support::build_timeline;
use super::*;

const FORMULA: &str = "$x - frac(x^3, 6) + frac(x^5, 120)$";

fn sampled_text_size(timeline: &Timeline, label: &str) -> [f32; 2] {
    let mut backend = None;
    let program = timeline.evaluate_program_with_debug(
        0.0,
        SceneDimensions {
            width: 1280,
            height: 720,
        },
        DebugRenderOptions::default(),
        &mut backend,
    );
    let item = program.items.iter().find(|item| item.label == label).expect("text item");
    let paths = item.commands.iter().find_map(|command| match command {
        crate::primitives::RenderCommand::Text { paths, .. } => Some(paths.as_ref()),
        _ => None,
    });
    crate::renderer::text::measure_text_paths(paths.expect("glyph paths")).map(|half| half * 2.0)
}

#[test]
fn intrinsic_col_keeps_typst_formula_identical_to_unconstrained_text() {
    for size in [
        "",
        "size: auto,",
        "size: (auto, auto),",
        "size: (auto, 200),",
        "size: fit,",
    ] {
        let source = format!(
            r#"
#0s
column: Col, {size} at: (320, 360), gap: 12 {{
  short: Typst, text: "$x - frac(x^3, 6)$", font_size: 28
  formula: Typst, text: "{FORMULA}", font_size: 28
}}
control: Typst, text: "{FORMULA}", font_size: 28, at: (900, 360)
"#
        );
        let timeline = build_timeline(&source);
        let formula = sampled_text_size(&timeline, "formula");
        let control = sampled_text_size(&timeline, "control");
        assert!(
            (formula[0] - control[0]).abs() < 0.01 && (formula[1] - control[1]).abs() < 0.01,
            "{size:?}: intrinsic text must not rewrap: formula={formula:?}, control={control:?}"
        );
        let id = timeline.world.get_id("formula").expect("formula entity");
        assert_eq!(
            timeline.world.text_or_default(id).text_max_width.get(0, 0.0),
            0.0,
            "a content-derived width is not a wrapping constraint"
        );
    }
}

#[test]
fn fixed_and_parent_assigned_col_widths_still_wrap_typst() {
    for (opening, closing, width) in [
        ("column: Col, size: (80, auto) {", "}", 80.0),
        ("column: Col, size: auto, max_item_width: 80 {", "}", 80.0),
        ("outer: Col, size: (160, auto) { column: Col, size: (50%, auto) {", "} }", 80.0),
        (
            "outer: Grid, size: (160, auto), cols: 2 { column: Col, size: (fill, auto) {",
            "} }",
            80.0,
        ),
    ] {
        let source = format!(
            r#"
#0s
{opening}
  formula: Typst, text: "{FORMULA}", font_size: 28
{closing}
control: Typst, text: "{FORMULA}", font_size: 28, at: (900, 360)
"#
        );
        let timeline = build_timeline(&source);
        let formula = sampled_text_size(&timeline, "formula");
        let control = sampled_text_size(&timeline, "control");
        assert!(
            formula[1] > control[1] + 10.0,
            "a bounded column must wrap: {opening}: formula={formula:?}, control={control:?}"
        );
        let id = timeline.world.get_id("formula").expect("formula entity");
        let wrap_width = timeline.world.text_or_default(id).text_max_width.get(0, 0.0);
        assert!((wrap_width - width).abs() < 0.01, "{opening}: wrap width={wrap_width}");
    }
}

#[test]
fn intrinsic_col_keeps_authored_text_wrap_width() {
    let timeline = build_timeline(&format!(
        r#"
#0s
column: Col, size: auto {{
  formula: Typst, text: "{FORMULA}", font_size: 28, text_max_width: 80
}}
control: Typst, text: "{FORMULA}", font_size: 28, text_max_width: 80, at: (900, 360)
"#
    ));
    let formula = sampled_text_size(&timeline, "formula");
    let control = sampled_text_size(&timeline, "control");
    assert_eq!(formula, control, "an authored text width remains authoritative");
    let id = timeline.world.get_id("formula").expect("formula entity");
    assert_eq!(timeline.world.text_or_default(id).text_max_width.get(0, 0.0), 80.0);
}

#[test]
fn nested_intrinsic_cols_keep_typst_unconstrained_during_reflow() {
    for parent in ["Col", "Stack", "Group"] {
        let timeline = build_timeline(&format!(
            r#"
config {{ dynamic_layout: true }}
#0s
outer: {parent}, size: auto {{
  column: Col, size: auto, padding: 12 {{
    formula: Typst, text: "{FORMULA}", font_size: 28
  }}
}}
control: Typst, text: "{FORMULA}", font_size: 28, at: (900, 360)
"#
        ));
        let formula = sampled_text_size(&timeline, "formula");
        let control = sampled_text_size(&timeline, "control");
        assert_eq!(formula, control, "{parent}: reflow must retain intrinsic text");
    }
}
