use std::sync::Arc;

use crate::timeline::assets::AssetCache;
use crate::timeline::{BuildOptions, SceneDimensions, Timeline};

const VTT: &str = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\n字幕缓存可用\n";

fn parse(source: &str) -> Vec<crate::ast::Stmt> {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    ast.expect("parsed scene")
}

#[test]
fn preloaded_transcript_builds_captions_without_filesystem_access() {
    let source = r#"
let captions_file = "./texts/captions.vtt"
#0s
subs: Captions, at: (640, 500), transcript: captions_file
"#;
    let document_dir = std::env::temp_dir().join("animatix_transcript_preload_no_file");
    let mut cache = AssetCache::new();
    cache.set_base_dirs(Some(&document_dir), None);
    cache.insert_transcript_text("texts/captions.vtt", Arc::<str>::from(VTT));
    let options = BuildOptions::default().with_asset_cache(Arc::new(cache));
    let report = Timeline::build_with_options(&parse(source), &Default::default(), options);

    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let transcript = report.output.transcript("subs").expect("transcript attaches to Captions");
    assert_eq!(transcript.segments.len(), 1);
    assert_eq!(transcript.segments[0].text, "字幕缓存可用");

    let frame = report.output.evaluate(0.25, SceneDimensions::default());
    assert!(
        !frame.encoding().draw_tags.is_empty(),
        "an active transcript segment should produce caption drawing"
    );
}

#[test]
fn invalid_preloaded_transcript_reports_a_build_warning() {
    let source = r#"
#0s
subs: Captions, transcript: "memory://broken.vtt"
"#;
    let mut cache = AssetCache::new();
    cache.insert_transcript_text("memory://broken.vtt", "this is not a transcript");
    let report = Timeline::build_with_options(
        &parse(source),
        &Default::default(),
        BuildOptions::default().with_asset_cache(Arc::new(cache)),
    );

    assert!(report.output.transcript("subs").is_none());
    let parse_warnings = report.diagnostics.iter().filter(|diagnostic| {
        diagnostic.code == crate::diagnostics::DiagnosticCode::InvalidConfigValue
            && diagnostic.message.contains("failed to parse transcript")
    });
    assert_eq!(
        parse_warnings.count(),
        1,
        "pre-scan must not duplicate the actor-build warning: {:?}",
        report.diagnostics
    );
}

#[test]
fn captions_in_expanded_loops_and_match_branches_attach_to_resolved_actor_labels() {
    let source = r#"
let captions_file = "memory://captions.vtt"
#0s
for i in {0, 1} {
  repeated[i]: Captions, at: (400 + i * 100, 500), transcript: captions_file
}
match "chosen" {
  "chosen" => {
    selected: Captions, at: (700, 500), transcript: captions_file
  }
  _ => {}
}
"#;
    let mut cache = AssetCache::new();
    cache.insert_transcript_text("memory://captions.vtt", VTT);
    let report = Timeline::build_with_options(
        &parse(source),
        &Default::default(),
        BuildOptions::default().with_asset_cache(Arc::new(cache)),
    );

    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    for label in ["repeated__0", "repeated__1", "selected"] {
        assert_eq!(
            report
                .output
                .transcript(label)
                .map(|transcript| transcript.segments[0].text.as_str()),
            Some("字幕缓存可用"),
            "transcript should attach to the actor label resolved by the statement walk: {label}"
        );
    }
    let frame = report.output.evaluate(0.25, SceneDimensions::default());
    assert!(!frame.encoding().draw_tags.is_empty(), "active loop/match captions should draw");
}

#[test]
fn transcript_prepass_remains_available_for_speech_anchored_keyframes() {
    let source = r#"
let transcript_file = "memory://voice.json"
voice: Audio, source: "voice.wav", transcript: transcript_file
#0s
card: Rect, opacity: 0
#voice@intro
card.opacity = 1
"#;
    let mut cache = AssetCache::new();
    cache.insert_transcript_text(
        "memory://voice.json",
        r#"{"duration":1.0,"markers":{"intro":0.4},"segments":[]}"#,
    );
    let report = Timeline::build_with_options(
        &parse(source),
        &Default::default(),
        BuildOptions::default().with_asset_cache(Arc::new(cache)),
    );

    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let card = report.output.get_entity_id("card").expect("card actor");
    assert!(
        report
            .output
            .world()
            .property_keyframe_times(card, crate::timeline::ActorField::Opacity)
            .contains(&400),
        "the speech prepass should retain transcript marker lowering"
    );
}

#[test]
fn transcript_cache_normalizes_ids_and_preserves_preloaded_text_on_invalidation() {
    let root =
        std::env::temp_dir().join(format!("animatix_transcript_cache_{}", std::process::id()));
    let mut cache = AssetCache::new();
    cache.set_base_dirs(Some(&root), None);
    cache.insert_transcript_text("texts/captions.vtt", VTT);

    let text = cache
        .load_transcript_text("./texts/captions.vtt")
        .expect("normalized cache hit");
    assert_eq!(text.as_ref(), VTT);
    assert_eq!(cache.transcripts().count(), 1);

    cache.invalidate_changed_assets();
    assert_eq!(
        cache
            .load_transcript_text("texts/captions.vtt")
            .expect("preloaded content has no file metadata"),
        text
    );
}

#[test]
fn filesystem_transcript_cache_invalidates_changed_file_content() {
    let root =
        std::env::temp_dir().join(format!("animatix_transcript_file_cache_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create transcript fixture directory");
    let path = root.join("captions.vtt");
    std::fs::write(&path, "WEBVTT\n").expect("write initial transcript");

    let mut cache = AssetCache::new();
    cache.set_base_dirs(Some(&root), None);
    let first = cache.load_transcript_text("captions.vtt").expect("read native transcript");
    assert_eq!(first.as_ref(), "WEBVTT\n");

    std::fs::write(&path, VTT).expect("replace transcript with different content");
    cache.invalidate_changed_assets();
    assert_eq!(cache.transcripts().count(), 0, "changed file content must invalidate cache");
    assert_eq!(cache.load_transcript_text("captions.vtt").unwrap().as_ref(), VTT);
}
