use super::*;
use animatix_syntax::parser::parse_source;
use crate::timeline::TrackAccessor;

#[test]
fn test_split_text_declaration_properties() {
    let source = r#"
#0s
title: Text, text: "Hello World", at: (100, 100), split_by: "word", split_stagger: 0.05, split_offset_y: 24.0, split_mask: true
"#;
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&stmts.expect("parsed AST"), &std::collections::HashMap::new());
    let track = report.output.tracks.get("title").expect("title track");
    assert_eq!(track.text.split_by.as_ref().map(|t| t.evaluate(0)), Some("word".to_string()));
    assert_eq!(track.text.split_stagger.as_ref().map(|t| t.evaluate(0)), Some(0.05));
    assert_eq!(track.text.split_offset_y.as_ref().map(|t| t.evaluate(0)), Some(24.0));
    assert_eq!(track.text.split_mask.as_ref().map(|t| t.evaluate(0)), Some(true));
}

#[test]
fn test_split_text_action_modifiers_and_evaluation() {
    let source = r#"
#0s
title: Text, text: "Hello Beautiful World", at: (200, 200)

#1s
draw-in title [1s, by: word, offset_y: 30, mask: baseline, stagger: 0.1s]
"#;
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));
    let track = timeline.tracks.get("title").expect("title track");

    assert!(track.text.char_progress.is_some());
    assert_eq!(track.text.char_progress.get(1000, 1.0), 0.0);
    assert_eq!(track.text.char_progress.get(2000, 1.0), 1.0);

    // At t=1500 (midpoint of reveal, progress=0.5):
    let (paths_mid, clip_mid) = track.evaluate_text_paths_and_clip(1500);
    assert!(!paths_mid.is_empty(), "Glyphs should be present");
    assert!(clip_mid.is_some(), "Baseline mask should generate clip path at midpoint");

    // Word 0 should have progressed further than word 2
    let word0_y = paths_mid.iter().find(|p| p.word_idx == 0).map(|p| p.glyph_center[1]).unwrap_or(0.0);
    let word2_y = paths_mid.iter().find(|p| p.word_idx == 2).map(|p| p.glyph_center[1]).unwrap_or(0.0);
    assert!(word0_y <= word2_y, "Word 0 (y={word0_y}) should rise earlier/higher than word 2 (y={word2_y})");

    // At t=2000 (reveal complete):
    let (paths_end, clip_end) = track.evaluate_text_paths_and_clip(2000);
    assert!(!paths_end.is_empty());
    assert!(clip_end.is_none(), "Baseline mask should be None once reveal is complete");
    for p in &paths_end {
        assert_eq!(p.opacity, 1.0, "All glyphs should be at full opacity at end");
    }
}

#[test]
fn test_reveal_in_on_text_target() {
    let source = r#"
#0s
sub: Text, text: "Split line reveal", at: (100, 150)

#0.5s
reveal-in sub [500ms, by: line, offset_y: 15, mask: baseline]
"#;
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));
    let track = timeline.tracks.get("sub").expect("sub track");

    assert!(track.text.char_progress.is_some());
    assert_eq!(track.text.char_progress.get(500, 1.0), 0.0);
    assert_eq!(track.text.char_progress.get(1000, 1.0), 1.0);

    let (paths_mid, clip_mid) = track.evaluate_text_paths_and_clip(750);
    assert!(!paths_mid.is_empty());
    assert!(clip_mid.is_some());
}

#[test]
fn test_counter_declaration_and_prebaked_data() {
    let source = r#"
#0s
c: Counter, value: 1234, prefix: "$", suffix: " /mo", decimals: 2, comma: true, at: (300, 200)
"#;
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&stmts.expect("parsed AST"), &std::collections::HashMap::new());
    let track = report.output.tracks.get("c").expect("counter track");

    let data = track.text.counter_data.as_ref().expect("prebaked counter data");
    assert_eq!(data.digits.len(), 10, "Should prebake digits 0..=9");
    assert!(data.col_width > 0.0, "Column width should be positive");
    assert!(data.slot_height > 0.0, "Slot height should be positive");
    assert!(data.prefix.1 > 0.0, "Prefix width should be non-empty");
    assert!(data.suffix.1 > 0.0, "Suffix width should be non-empty");
    assert!(data.comma.1 > 0.0, "Comma width should be non-empty");
    assert!(data.dot.1 > 0.0, "Dot width should be non-empty");

    let size = track.geometry.size.get(0, [0.0, 0.0]);
    assert!(size[0] > 0.0 && size[1] > 0.0, "Initial half size should be non-zero");
}

#[test]
fn test_counter_evaluation_and_keyframing() {
    let source = r#"
#0s
cnt: Counter, value: 0, prefix: "$", suffix: "k", at: (400, 300)

#1s
cnt.value = 100 [1s]
"#;
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));
    let track = timeline.tracks.get("cnt").expect("cnt track");

    use crate::primitives::Primitive;
    let primitive = crate::primitives::counter::COUNTER;
    let asset_cache = crate::timeline::assets::AssetCache::default();

    let make_ctx = |time_ms: u64| crate::primitives::EvaluateCtx {
        track,
        time_ms,
        local_transform: kurbo::Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions { width: 1920, height: 1080 },
        background_color: [0.0, 0.0, 0.0, 1.0],
        overrides: None,
        vector_paths: &[],
        asset_cache: &asset_cache,
        target_resolver: None,
    };

    let ctx0 = make_ctx(0);
    let cmds0 = primitive.evaluate(&ctx0, None).expect("eval success").expect("render commands");
    assert_eq!(cmds0.len(), 1);
    match &cmds0[0] {
        crate::primitives::RenderCommand::Text { paths, clip_path, .. } => {
            assert!(!paths.is_empty(), "Paths should be generated for counter at t=0");
            assert!(clip_path.is_some(), "Clip path should be present for slot clipping");
        },
        other => panic!("Expected RenderCommand::Text, got {other:?}"),
    }

    let ctx_mid = make_ctx(1500);
    let cmds_mid = primitive.evaluate(&ctx_mid, None).expect("eval success").expect("render commands");
    match &cmds_mid[0] {
        crate::primitives::RenderCommand::Text { paths, .. } => {
            assert!(!paths.is_empty(), "Paths should be generated at midpoint");
        },
        other => panic!("Expected RenderCommand::Text, got {other:?}"),
    }

    let ctx_end = make_ctx(2000);
    let cmds_end = primitive.evaluate(&ctx_end, None).expect("eval success").expect("render commands");
    match &cmds_end[0] {
        crate::primitives::RenderCommand::Text { paths, .. } => {
            assert!(!paths.is_empty(), "Paths should be generated at end");
        },
        other => panic!("Expected RenderCommand::Text, got {other:?}"),
    }
}
