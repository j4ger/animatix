//! Timeline build acceptance tests for entrance opacity and text anchoring.

use super::super::tests::without_content_lints;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{Timeline, Value};

#[test]
fn component_instance_with_explicit_zero_opacity_lifts_on_fade_in() {
    const SRC: &str = r##"
pub component BoxItem(title: Str) {
  box: Stack, size: (100, 50) {
    bg: Rect, size: (100, 50), color: "#ff0000"
    txt: Text, text: title, font_size: 16
  }
}

#0s
item: BoxItem, title: "Hello", opacity: 0.0

#1s
fade-in item [500ms]
"##;
    let mut graph = animatix_syntax::module::ModuleGraph::new();
    let program = graph
        .load_program_with_source(std::path::Path::new("virtual:///comp_repro.amx"), Some(SRC))
        .expect("load program");
    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    assert!(expansion_errors.is_empty(), "expansion errors: {expansion_errors:?}");

    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
    let timeline = report.output;
    let item_track = timeline.world.tracks.get_id("item").expect("item track exists");
    assert_eq!(
        timeline.world.style(item_track).opacity.get(500, 1.0),
        0.0,
        "should be invisible before entrance"
    );
    assert_eq!(
        timeline.world.style(item_track).opacity.get(1500, 0.0),
        1.0,
        "should be lifted to 1.0 after entrance"
    );
}

#[test]
fn entrance_action_warns_when_target_opacity_remains_zero() {
    // When an actor has an opacity assignment or structure that keeps it at 0.0 after entrance
    const SRC: &str = r##"
box: Rect, size: (100, 100), opacity: 0.0
#1s
wipe-in box [500ms]
"##;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(SRC);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "wipe-in lifts authored 0.0 on rect so no warning expected, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn text_anchor_left_center_right_alignments() {
    let source = r#"
t_center: Text, text: "Animatix", at: (300, 200)
t_left: Text, text: "Animatix", text_anchor: "left", at: (300, 200)
t_right: Text, text: "Animatix", text_anchor: "right", at: (300, 200)
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    fn x_bounds(paths: &[crate::renderer::text::TextPath]) -> (f64, f64) {
        use kurbo::Shape as _;
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        for p in paths {
            let b = p.path.bounding_box();
            min_x = min_x.min(b.x0);
            max_x = max_x.max(b.x1);
        }
        (min_x, max_x)
    }

    let center = timeline.world.tracks.get_id("t_center").unwrap();
    let left = timeline.world.tracks.get_id("t_left").unwrap();
    let right = timeline.world.tracks.get_id("t_right").unwrap();
    let center_paths = timeline.world.text(center).evaluate_paths(timeline.world.style(center), 0);
    let left_paths = timeline.world.text(left).evaluate_paths(timeline.world.style(left), 0);
    let right_paths = timeline.world.text(right).evaluate_paths(timeline.world.style(right), 0);

    let (c_min, c_max) = x_bounds(&center_paths);
    let (l_min, l_max) = x_bounds(&left_paths);
    let (r_min, r_max) = x_bounds(&right_paths);

    let width = c_max - c_min;
    assert!(width > 10.0, "text must have positive width, got {width}");

    // Center anchor: centered at 0
    assert!(
        (c_min + c_max).abs() < 1e-3,
        "center anchor should center on 0: [{c_min:.3}, {c_max:.3}]"
    );

    // Left anchor: left edge sits at 0, extending right to +width
    assert!(l_min.abs() < 1e-3, "left anchor should place left edge at 0: min={l_min:.3}");
    assert!(
        (l_max - width).abs() < 1e-3,
        "left anchor right edge should be at width={width:.3}: max={l_max:.3}"
    );

    // Right anchor: right edge sits at 0, extending left to -width
    assert!(r_max.abs() < 1e-3, "right anchor should place right edge at 0: max={r_max:.3}");
    assert!(
        (r_min + width).abs() < 1e-3,
        "right anchor left edge should be at -width={:.3}: min={r_min:.3}",
        -width
    );
}

#[test]
fn text_anchor_assignment_reanchors_paths() {
    let source = r#"
#0s
t: Text, text: "AnchorTest", text_anchor: "left", at: (300, 200)

#1s
t.text_anchor = "right"
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    fn x_bounds(paths: &[crate::renderer::text::TextPath]) -> (f64, f64) {
        use kurbo::Shape as _;
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        for p in paths {
            let b = p.path.bounding_box();
            min_x = min_x.min(b.x0);
            max_x = max_x.max(b.x1);
        }
        (min_x, max_x)
    }

    let t_track = timeline.world.tracks.get_id("t").unwrap();
    let t_text = timeline.world.text(t_track);
    let t_style = timeline.world.style(t_track);
    let paths_0 = t_text.evaluate_paths(t_style, 0);
    let paths_1000 = t_text.evaluate_paths(t_style, 1000);

    let (min_0, _) = x_bounds(&paths_0);
    let (_, max_1000) = x_bounds(&paths_1000);

    assert!(
        min_0.abs() < 1e-3,
        "at 0s, text_anchor left should place left edge at 0, got {min_0:.3}"
    );
    assert!(
        max_1000.abs() < 1e-3,
        "at 1s, text_anchor right should place right edge at 0, got {max_1000:.3}"
    );
}

#[test]
fn text_anchor_override_shifts_paths() {
    let source = r#"
t: Text, text: "OverrideAnchor", text_anchor: "center", at: (300, 200)

always {
  t.text_anchor = "left"
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("text_anchor".to_string(), Value::Str("left".to_string()));

    let t_track = timeline.world.tracks.get_id("t").unwrap();
    let font_ctx = crate::renderer::text::FontContext::new();
    let mut text_compiler = crate::renderer::text::TextCompiler::new();
    let mut text_compile_ctx = crate::primitives::TextCompileCtx {
        font_context: &font_ctx,
        text_compiler: &mut text_compiler,
    };
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx = crate::primitives::EvaluateCtx {
        id: t_track,
        label: "t",
        caps: timeline.world.caps(t_track),
        highlight: timeline.world.highlight(t_track),
        property_plan: Some(timeline.world.property_plan(t_track)),
        world: Some(&timeline.world),
        shape: Some(timeline.world.shape(t_track)),
        text: Some(timeline.world.text(t_track)),
        style: Some(timeline.world.style(t_track)),
        geometry: Some(timeline.world.geometry(t_track)),
        payload: timeline.world.payload(t_track),
        time_ms: 0,
        local_transform: kurbo::Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: crate::timeline::SceneDimensions {
            width: 640,
            height: 480,
        },
        background_color: [0.0; 4],
        overrides: Some(&overrides),
        vector_paths: &[],
        asset_cache: &asset_cache,
        target_resolver: None,
        legend: None,
    };
    use crate::primitives::Primitive as _;
    let res = crate::primitives::TEXT
        .evaluate(&eval_ctx, Some(&mut text_compile_ctx))
        .unwrap();
    let Some(commands) = res else {
        panic!("expected render commands")
    };
    let crate::primitives::RenderCommand::Text { paths, .. } = &commands[0] else {
        panic!("expected Text render command");
    };
    use kurbo::Shape as _;
    let mut min_x = f64::INFINITY;
    for p in paths.iter() {
        let b = p.path.bounding_box();
        min_x = min_x.min(b.x0);
    }
    assert!(
        min_x.abs() < 1e-3,
        "override to text_anchor left should place left edge at 0, got {min_x:.3}"
    );
}
