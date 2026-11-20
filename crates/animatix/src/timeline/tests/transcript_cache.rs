use std::sync::Arc;

use crate::timeline::assets::AssetCache;
use crate::timeline::{BuildOptions, BuildQuality, SceneDimensions, Timeline};

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
        !frame.draws().is_empty(),
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
    assert!(!frame.draws().is_empty(), "active loop/match captions should draw");
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
fn native_relative_document_base_loads_audio_transcript_without_double_joining() {
    let source = r#"
voice: Audio, source: "../assets/narration.mp3", transcript: "../assets/narration.speech.json"
subs: Captions, transcript: "../assets/narration.speech.json"
card: Rect, opacity: 0
#voice@intro
card.opacity = 1
"#;
    let relative_root = crate::test_support::TestTempDir::new_in(
        std::path::Path::new("target"),
        "transcript-relative-base",
    );
    let document_dir = relative_root.path().join("animation");
    let asset_dir = relative_root.path().join("assets");
    std::fs::create_dir_all(&document_dir).expect("create relative document directory");
    std::fs::create_dir_all(&asset_dir).expect("create relative asset directory");
    let source_assets =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets");
    std::fs::copy(
        source_assets.join("narration.speech.json"),
        asset_dir.join("narration.speech.json"),
    )
    .expect("copy real transcript fixture");
    std::fs::copy(source_assets.join("narration.mp3"), asset_dir.join("narration.mp3"))
        .expect("copy real audio fixture");

    let mut cache = AssetCache::new();
    cache.set_base_dirs(Some(&document_dir), None);
    let transcript_identity = cache.normalize_asset_url("../assets/narration.speech.json");
    let report = Timeline::build_with_options(
        &parse(source),
        &Default::default(),
        BuildOptions::default().with_asset_cache(Arc::new(cache)),
    );

    assert!(
        report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "{:?}",
        report.diagnostics
    );
    for label in ["voice", "subs"] {
        let transcript = report.output.transcript(label).expect("native transcript attaches");
        assert!(transcript.find_marker("intro").is_some());
        let assets = report.output.asset_cache().assets_for(label).collect::<Vec<_>>();
        assert!(
            assets.iter().any(|asset| asset.as_str() == transcript_identity.as_str()),
            "usage tracks the single-normalized transcript resource identity: {assets:?}"
        );
    }
    let card = report.output.get_entity_id("card").expect("card actor");
    assert!(
        report
            .output
            .world()
            .property_keyframe_times(card, crate::timeline::ActorField::Opacity)
            .contains(&113),
        "the relative native transcript's marker must lower the speech anchor"
    );
    drop(report);
}

#[test]
fn transcript_cache_normalizes_ids_and_preserves_preloaded_text_on_invalidation() {
    let root = crate::test_support::TestTempDir::new("transcript-cache");
    let mut cache = AssetCache::new();
    cache.set_base_dirs(Some(root.path()), None);
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
    let root = crate::test_support::TestTempDir::new("transcript-file-cache");
    let path = root.path().join("captions.vtt");
    std::fs::write(&path, "WEBVTT\n").expect("write initial transcript");

    let mut cache = AssetCache::new();
    cache.set_base_dirs(Some(root.path()), None);
    let first = cache.load_transcript_text("captions.vtt").expect("read native transcript");
    assert_eq!(first.as_ref(), "WEBVTT\n");

    std::fs::write(&path, VTT).expect("replace transcript with different content");
    cache.invalidate_changed_assets();
    assert_eq!(cache.transcripts().count(), 0, "changed file content must invalidate cache");
    assert_eq!(cache.load_transcript_text("captions.vtt").unwrap().as_ref(), VTT);
}

const NARRATION: &str = r#"{"duration":8,"segments":[
    {"start":1,"end":2,"text":"猫看猫。","words":[{"word":"猫","start":1,"end":1.2},{"word":"看猫","start":1.4,"end":2}]},
    {"start":3,"end":5,"text":"猫看猫。","words":[{"word":"猫看","start":3,"end":3.5},{"word":"猫。","start":4,"end":5}]}
]}"#;

fn narration_timeline(
    source: &str,
    quality: BuildQuality,
) -> crate::timeline::BuildReport<Timeline> {
    let mut cache = AssetCache::new();
    cache.insert_transcript_text("memory://voice.json", NARRATION);
    Timeline::build_with_options(
        &parse(source),
        &Default::default(),
        BuildOptions::default().with_asset_cache(Arc::new(cache)).with_quality(quality),
    )
}

#[test]
fn narration_paragraphs_and_explicit_phrase_edges_use_scene_audio_time() {
    use animatix_syntax::narration::{AnchorEdge, NarrationAnchor, encode_anchor};

    let marker = encode_anchor(&NarrationAnchor {
        paragraph: "猫看猫。".into(),
        paragraph_occurrence: 2,
        phrase: Some("猫".into()),
        phrase_occurrence: 2,
        edge: AnchorEdge::End,
    })
    .expect("valid phrase");
    let report = narration_timeline(
        &format!(
            r#"
voice: Audio, source: "voice.wav", transcript: "memory://voice.json" [delay: 500ms]
card: Rect, opacity: 0
/// 猫看猫。
card.opacity = 0.25
/// 猫看猫。
#narration@{marker}
card.opacity = 0.5
#8s
card.opacity = 1
"#
        ),
        BuildQuality::Production,
    );

    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let card = report.output.get_entity_id("card").expect("card actor");
    let times = report
        .output
        .world()
        .property_keyframe_times(card, crate::timeline::ActorField::Opacity);
    assert!(times.contains(&1500), "{times:?}");
    assert!(times.contains(&5500), "{times:?}");
    assert!(times.contains(&8000), "{times:?}");
    assert!(!times.contains(&3500), "{times:?}");
}

#[test]
fn draft_narration_and_stale_phrase_bindings_preview() {
    use crate::diagnostics::DiagnosticCode::NarrationBindingUnresolved;
    use animatix_syntax::narration::{AnchorEdge, NarrationAnchor, encode_anchor};

    let marker = encode_anchor(&NarrationAnchor {
        paragraph: "猫看猫。".into(),
        paragraph_occurrence: 1,
        phrase: Some("猫".into()),
        phrase_occurrence: 2,
        edge: AnchorEdge::Start,
    })
    .expect("valid phrase");
    let bare_draft = narration_timeline(
        "card: Rect, opacity: 0\n/// 猫看猫。\ncard.opacity = 1",
        BuildQuality::Draft,
    );
    assert!(
        bare_draft.diagnostics.iter().all(|d| !d.is_error()),
        "{:?}",
        bare_draft.diagnostics
    );
    let source = format!(
        "narration: Audio, source: \"voice.wav\", transcript: \"memory://voice.json\"\ncard: Rect, opacity: 0\n/// 猫看狗。\n#narration@{marker}\ncard.opacity = 1"
    );
    let report = narration_timeline(&source, BuildQuality::Draft);

    assert!(report.diagnostics.iter().any(|d| d.code == NarrationBindingUnresolved
        && d.message.contains("changed or missing paragraph")
        && d.location.line == Some(4)));
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let card = report.output.get_entity_id("card").expect("card actor");
    let times = report
        .output
        .world()
        .property_keyframe_times(card, crate::timeline::ActorField::Opacity);
    assert_eq!(times, [0]);
    let production = narration_timeline(&source, BuildQuality::Production);
    assert!(
        production
            .diagnostics
            .iter()
            .any(|d| d.is_error() && d.code == NarrationBindingUnresolved)
    );
}
