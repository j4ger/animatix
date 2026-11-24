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
    assert!(patched.contains(
        "narration_captions: Captions, source: \"narration\", at: (50%, 85%), opacity: 1\r\n"
    ));
    assert!(patched.contains("/// 好好看这段。\r\n"));
    assert!(parse_source_complete(&patched).errors.is_empty());
}

#[test]
fn retake_preserves_legacy_audio_settings_and_binds_captions_to_its_clock() {
    let source = r#"voice: Audio, volume: 0.5, source: "old.wav", transcript: "old.speech.json" [delay: 2s]
subtitles: Captions, transcript: "old.speech.json", at: (42, 55), font_size: 20, opacity: 1
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
    assert!(patched.contains(
        "subtitles: Captions, source: \"voice\", at: (42, 55), font_size: 20, opacity: 1"
    ));
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

#[test]
fn auto_caption_draws_in_the_bottom_safe_area_on_landscape_and_portrait_canvases() {
    use animatix::scene::Draw;
    use animatix::timeline::SceneDimensions;
    use kurbo::Shape;

    for (width, height) in [(960, 540), (540, 960)] {
        let scene = format!(
            "config {{ canvas_size: ({width}, {height}), duration: 2 }}\n/// Hello.\n#0s\n  hero: Rect, opacity: 0\n"
        );
        let (dir, store) = document_store_loaded(&scene);
        let target = Target::open(&store.source, None).unwrap();
        let adopted = adopted_at(dir.path());
        std::fs::write(
            &adopted.transcript,
            r#"{"segments":[{"start":0,"end":1,"text":"Hello."}]}"#,
        )
        .unwrap();
        let new = target.patch(&store.source, &adopted).unwrap().apply(&store.source).unwrap();
        let old = new.replace("at: (50%, 85%), ", "");

        for (source, y) in [(old, 0.5), (new, 0.85)] {
            let mut document = crate::app::document::DocumentSession::from_source(
                store.source.file_path().to_owned(),
                source,
            )
            .unwrap();
            document.rebuild().unwrap();
            let frame = document
                .active_timeline()
                .unwrap()
                .evaluate(0.5, SceneDimensions { width, height });
            assert!(
                frame.draws().iter().any(|draw| {
                    let Draw::Fill {
                        path, transform, ..
                    } = draw
                    else {
                        return false;
                    };
                    let bounds = transform.transform_rect_bbox(path.bounding_box());
                    let center = bounds.center();
                    bounds.width() < f64::from(width) / 2.0
                        && (center.x - f64::from(width) / 2.0).abs() < 1.0
                        && (center.y - f64::from(height) * y).abs() < 1.0
                }),
                "no caption backdrop at y={y} for {width}×{height}"
            );
        }
    }
}
