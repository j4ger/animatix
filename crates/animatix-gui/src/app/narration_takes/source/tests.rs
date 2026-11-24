use super::*;
use crate::app::test_support::document_store_loaded;

fn adopted_at(folder: &Path) -> Adopted {
    let media = folder.join("narration").join("reviewed-take");
    std::fs::create_dir_all(&media).unwrap();
    let audio = media.join("chapter.wav");
    let transcript = media.join("chapter.speech.json");
    std::fs::write(&audio, []).unwrap();
    std::fs::write(&transcript, "{}").unwrap();
    Adopted { audio, transcript }
}

#[test]
fn initial_take_binds_only_the_selected_scene() {
    let source = concat!(
        "# Intro\r\n",
        "/// First paragraph.\r\n",
        "#0s\r\n",
        "  intro: Rect\r\n",
        "# Demo\r\n",
        "/// 好好看这段。\r\n",
        "#0s\r\n",
        "  shape: Ellipse\r\n",
    );
    let (dir, store) = document_store_loaded(source);
    let target = Target::open(&store.source, Some("Demo".into())).unwrap();
    assert_eq!(target.paragraphs.len(), 1);
    assert_eq!(target.paragraphs[0].text, "好好看这段。");
    assert!(target.base.is_none());

    let adopted = adopted_at(dir.path());
    let patched = target.patch(&store.source, &adopted).unwrap().apply(&store.source).unwrap();
    assert_eq!(patched.matches("narration: Audio").count(), 1);
    assert!(patched.starts_with("# Intro\r\n/// First paragraph.\r\n#0s\r\n  intro: Rect\r\n"));
    assert!(patched.contains("narration: Audio, source: \"narration/reviewed-take/chapter.wav\""));
    assert!(
        patched.contains("narration_captions: Captions, source: \"narration\", opacity: 1\r\n")
    );
    assert!(patched.contains("/// 好好看这段。\r\n"));
    assert!(parse_source_complete(&patched).errors.is_empty());
}

#[test]
fn retake_preserves_legacy_audio_settings_and_binds_captions_to_its_clock() {
    let source = r#"voice: Audio, volume: 0.5, source: "old.wav", transcript: "old.speech.json" [delay: 2s]
subtitles: Captions, transcript: "old.speech.json", font_size: 20, opacity: 1
/// Read this sentence.
#0s
  shape: Ellipse
"#;
    let (dir, store) = document_store_loaded(source);
    let target = Target::open(&store.source, None).unwrap();
    let (audio, transcript) = target.base.as_ref().unwrap();
    assert!(audio.ends_with("old.wav"));
    assert!(transcript.ends_with("old.speech.json"));

    let adopted = adopted_at(dir.path());
    let patched = target.patch(&store.source, &adopted).unwrap().apply(&store.source).unwrap();
    assert!(
        patched
            .contains("voice: Audio, volume: 0.5, source: \"narration/reviewed-take/chapter.wav\"")
    );
    assert!(
        patched.contains("transcript: \"narration/reviewed-take/chapter.speech.json\" [delay: 2s]")
    );
    assert!(patched.contains("subtitles: Captions, source: \"voice\", font_size: 20, opacity: 1"));
    assert!(!patched.contains("narration_captions"));
    assert!(parse_source_complete(&patched).errors.is_empty());
}

#[test]
fn same_source_reopened_during_take_adoption_is_a_different_revision() {
    let source = "/// Read this sentence.\n#0s\n  shape: Ellipse\n";
    let (dir, mut store) = document_store_loaded(source);
    let target = Target::open(&store.source, None).unwrap();
    let path = store.source.file_path().to_owned();
    store.source.editor.set_document(&path, source.into());

    let adopted = adopted_at(dir.path());
    assert!(target.patch(&store.source, &adopted).is_err());
}
