//! Integration tests for speech-anchored keyframes and first-class captions.

use std::fs;
use std::path::PathBuf;

use animatix::timeline::{SceneDimensions, Timeline, TrackAccessor};
use animatix_syntax::module::ModuleGraph;

const DIMS: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};

fn setup_temp_workspace(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("amx_speech_test_{test_name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn test_declarative_captions_with_cue_children() {
    let dir = setup_temp_workspace("decl_captions");
    let amx_path = dir.join("main.amx");

    let source = r#"
subs: Captions, opacity: 1.0, at: (960, 900), font_size: 36.0, caption_mode: "word", backdrop: true {
    c1: Cue, text: "Welcome to Animatix", start: 0.0, end: 2.0
    c2: Cue, text: "Layout first animation DSL", start: 2.0, end: 4.5
}
"#;
    fs::write(&amx_path, source).unwrap();

    let ast = ModuleGraph::new()
        .load_program(&amx_path)
        .expect("load program")
        .expand_components(&mut Vec::new());

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "build diagnostics: {:?}", report.diagnostics);

    let tl = report.output;
    assert!(tl.tracks().contains_key("subs"));
    assert!(tl.tracks().contains_key("c1"));
    assert!(tl.tracks().contains_key("c2"));

    // Verify evaluation renders scenes at cue times without panic
    let scene_0 = tl.evaluate(0.5, DIMS);
    let scene_1 = tl.evaluate(2.5, DIMS);
    let _scene_2 = tl.evaluate(5.0, DIMS); // inactive

    // Ensure scenes produced valid drawing output
    assert!(!scene_0.encoding().is_empty());
    assert!(!scene_1.encoding().is_empty());
}

#[test]
fn test_captions_with_speech_transcript() {
    let dir = setup_temp_workspace("transcript_captions");
    let json_path = dir.join("narration.speech.json");
    let amx_path = dir.join("main.amx");

    let transcript_json = r#"{
    "duration": 5.0,
    "markers": {
        "intro": 0.5,
        "outro": 3.0
    },
    "segments": [
        {
            "id": 0,
            "start_s": 0.0,
            "end_s": 2.0,
            "text": "Welcome to Animatix",
            "words": [
                { "word": "Welcome", "start_s": 0.0, "end_s": 0.6 },
                { "word": "to", "start_s": 0.7, "end_s": 1.0 },
                { "word": "Animatix", "start_s": 1.1, "end_s": 1.9 }
            ]
        }
    ]
}"#;
    fs::write(&json_path, transcript_json).unwrap();

    let source = format!(
        r#"
voice: Audio, source: "mock.mp3", transcript: "{}"
subs: Captions, opacity: 1.0, transcript: "{}", caption_mode: "karaoke", backdrop: true
"#,
        json_path.display(),
        json_path.display()
    );
    fs::write(&amx_path, source).unwrap();

    let ast = ModuleGraph::new()
        .load_program(&amx_path)
        .expect("load program")
        .expand_components(&mut Vec::new());

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "build diagnostics: {:?}", report.diagnostics);

    let tl = report.output;
    assert!(tl.transcript("voice").is_some());

    // Frame evaluation at word active times
    let _scene_word0 = tl.evaluate(0.3, DIMS);
    let _scene_word1 = tl.evaluate(0.8, DIMS);
    let _scene_word2 = tl.evaluate(1.5, DIMS);
}

#[test]
fn test_speech_anchored_keyframes() {
    let dir = setup_temp_workspace("speech_keyframes");
    let json_path = dir.join("narration.speech.json");
    let amx_path = dir.join("main.amx");

    let transcript_json = r#"{
    "duration": 5.0,
    "markers": {
        "intro": 0.5,
        "outro": 3.0
    },
    "segments": [
        {
            "id": 0,
            "start_s": 0.0,
            "end_s": 2.0,
            "text": "Welcome to Animatix",
            "words": [
                { "word": "Welcome", "start_s": 0.0, "end_s": 0.6 },
                { "word": "to", "start_s": 0.7, "end_s": 1.0 },
                { "word": "Animatix", "start_s": 1.1, "end_s": 1.9 }
            ]
        }
    ]
}"#;
    fs::write(&json_path, transcript_json).unwrap();

    let source = format!(
        r#"
voice: Audio, source: "mock.mp3", transcript: "{}"
box: Rect, opacity: 0.0, at: (100, 100), size: (50.0, 50.0)

#voice@intro:
    box.opacity = 1.0

#voice["Animatix"]:
    box.size = (200.0, 50.0)

#voice@outro + 500ms:
    box.opacity = 0.0
"#,
        json_path.display()
    );
    fs::write(&amx_path, source).unwrap();

    let ast = ModuleGraph::new()
        .load_program(&amx_path)
        .expect("load program")
        .expand_components(&mut Vec::new());

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "build diagnostics: {:?}", report.diagnostics);

    let tl = report.output;
    let box_id = tl.get_entity_id("box").expect("box entity");

    // #voice@intro (intro = 0.5s) -> 500ms
    // #voice["Animatix"] (Animatix start_s = 1.1s) -> 1100ms
    // #voice@outro + 500ms (outro = 3.0s + 0.5s) -> 3500ms
    use animatix::timeline::TrackAccessor;
    assert_eq!(tl.world().style(box_id).opacity.get(400, 0.0), 0.0);
    assert_eq!(tl.world().style(box_id).opacity.get(500, 0.0), 1.0);
    assert_eq!(tl.world().geometry(box_id).size.get(1100, [0.0, 0.0])[0], 100.0); // half_width = 100
    assert_eq!(tl.world().style(box_id).opacity.get(3500, 1.0), 0.0);
}

#[test]
fn test_speech_keyframe_missing_marker_diagnostic() {
    let dir = setup_temp_workspace("missing_marker");
    let json_path = dir.join("narration.speech.json");
    let amx_path = dir.join("main.amx");

    let transcript_json = r#"{
    "duration": 5.0,
    "markers": {
        "intro": 0.5
    },
    "segments": []
}"#;
    fs::write(&json_path, transcript_json).unwrap();

    let source = format!(
        r#"
voice: Audio, source: "mock.mp3", transcript: "{}"
box: Rect, at: (100, 100)

#voice@nonexistent:
    box.opacity = 1.0
"#,
        json_path.display()
    );
    fs::write(&amx_path, source).unwrap();

    let ast = ModuleGraph::new()
        .load_program(&amx_path)
        .expect("load program")
        .expand_components(&mut Vec::new());

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        !report.diagnostics.is_empty(),
        "should produce error diagnostic for nonexistent marker"
    );
    let msg = &report.diagnostics[0].message;
    assert!(msg.contains("nonexistent"), "diagnostic should mention missing marker: {msg}");
}

#[test]
fn test_demo_scene_captions_evaluation() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let amx_path = manifest_dir.join("../../examples/animation/41_speech_sync_captions.amx");
    if !amx_path.exists() {
        return;
    }
    let ast = ModuleGraph::new()
        .load_program(&amx_path)
        .expect("load program")
        .expand_components(&mut Vec::new());

    let ext_ctx = std::sync::Arc::new(animatix::extension_context::ExtensionContext::new());
    let namespaces = std::collections::HashMap::new();
    let report = animatix::composition::BuildTarget::from_ast_with_context_and_defines(
        &ast,
        &namespaces,
        Some(&amx_path),
        ext_ctx,
        &[],
    );
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.severity != animatix_syntax::diagnostics::DiagnosticSeverity::Error)
    );

    let tl = match report.output {
        animatix::composition::BuildTarget::SingleScene(tl) => tl,
        animatix::composition::BuildTarget::MultiScene(_) => panic!("Expected single scene"),
    };
    assert!(tl.transcript("voice").is_some());
    assert!(tl.transcript("subs").is_some());
    let subs_track = tl.tracks().get("subs").expect("subs track must exist");
    assert_eq!(subs_track.actor_type, "Captions");
    assert_eq!(tl.world().style(subs_track.id).opacity.get(2000, 0.0), 1.0);

    let _scene_at_2s = tl.evaluate(2.0, DIMS);
}

/// A `Cue` with no `start`/`end` has an empty window and is never shown; the
/// build must say so instead of rendering nothing silently.
#[test]
fn test_cue_without_a_window_warns() {
    let dir = setup_temp_workspace("cue_no_window");
    let amx_path = dir.join("main.amx");
    let source = r#"
subs: Captions, opacity: 1.0, at: (960, 900) {
    implicit: Cue, text: "no window"
}
"#;
    fs::write(&amx_path, source).unwrap();
    let ast = ModuleGraph::new()
        .load_program(&amx_path)
        .expect("load program")
        .expand_components(&mut Vec::new());
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let warned = report.diagnostics.iter().any(|d| {
        d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Warning
            && d.message.contains("Cue `implicit`")
            && d.message.contains("never shown")
    });
    assert!(
        warned,
        "expected a never-shown warning for a cue with no window, got: {:?}",
        report.diagnostics
    );

    // The explicit-window cue in the other fixture must stay warning-free.
    assert!(
        report.diagnostics.iter().all(|d| !d.message.contains("Cue `c1`")),
        "a cue with an explicit window must not warn: {:?}",
        report.diagnostics
    );
}
