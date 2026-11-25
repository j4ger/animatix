use super::*;

/// "Restore defaults" resets encoders/resolution/timing but must not touch
/// what to export (`format`, `export_scope`) or where (`output_path`).
#[test]
fn restore_defaults_keeps_target_but_resets_encoding() {
    let mut state = ExportDialogState {
        format: ExportFormat::WebP,
        width: 640,
        height: 360,
        fps: 12,
        duration_s: 9.0,
        hold_s: 4.0,
        auto_duration: false,
        export_scope: ExportScope::WholeComposition,
        output_path: "custom/path.webp".to_string(),
        video_codec: animatix_render::encode::VideoCodec::H264Vaapi,
        h264_preset: animatix_render::encode::H264Preset::Veryslow,
        ..ExportDialogState::default()
    };
    let defaults = ExportDialogState::default();
    state.width = defaults.width;
    state.height = defaults.height;
    state.fps = defaults.fps;
    state.duration_s = defaults.duration_s;
    state.hold_s = defaults.hold_s;
    state.auto_duration = defaults.auto_duration;
    state.video_codec = defaults.video_codec;
    state.h264_preset = defaults.h264_preset;

    assert_eq!(state.width, defaults.width);
    assert_eq!(state.fps, defaults.fps);
    assert_eq!(state.h264_preset, defaults.h264_preset);
    assert_eq!(state.format, ExportFormat::WebP, "format is a target choice, not a setting");
    assert_eq!(state.export_scope, ExportScope::WholeComposition);
    assert_eq!(state.output_path, "custom/path.webp");
}

#[test]
fn switching_format_updates_suggested_filename_and_preserves_custom_path() {
    let mut shell = GuiShell::load(PathBuf::from("glass.amx"), false);
    shell.update_default_export_filename();
    let video = PathBuf::from(&shell.export_store.export_state.output_path);
    assert_eq!(video.extension().unwrap(), "mp4");

    shell.set_export_format(ExportFormat::Image);
    assert_eq!(
        PathBuf::from(&shell.export_store.export_state.output_path),
        video.with_extension("png")
    );
    shell.set_export_format(ExportFormat::WebP);
    assert_eq!(
        PathBuf::from(&shell.export_store.export_state.output_path),
        video.with_extension("webp")
    );

    shell.export_store.export_state.output_path = "custom/frame.png".into();
    shell.set_export_format(ExportFormat::Image);
    assert_eq!(shell.export_store.export_state.output_path, "custom/frame.png");
}

#[test]
fn export_submission_rechecks_source_after_dialog_opened() {
    let mut shell = GuiShell::load(PathBuf::from("missing-export-boundary.amx"), false);
    shell.document_store = crate::app::test_support::make_document_store("#0s\n");
    shell.handle_action(crate::app::commands::Command::EditorChanged.into());
    shell.start_export();
    assert!(matches!(shell.export_store.export_status, ExportStatus::Failed(ref error)
        if error.canonical().contains("current source")));
    assert!(shell.export_store.export_thread.is_none());
}

#[test]
fn draft_warning_follows_the_visible_subtitle_and_clears_after_readoption() {
    use std::collections::HashMap;

    use crate::app::document::narration::{AdoptedVoiceover, VoiceoverHistory};
    use crate::app::test_support::document_store_loaded;
    use animatix_syntax::narration::paragraphs;

    let script = "/// 第一段。\n\n/// 第二段。 [draft]\n";
    let source = format!(
        "config {{ duration: 5 }}\nnarration: Audio, source: \"audio.wav\", transcript: \"timing.speech.json\"\n\
         narration_captions: Captions, source: \"narration\", opacity: 1\n{script}\n#0s\n  shape: Rect\n"
    );
    let (dir, mut store) = document_store_loaded("#0s\n  shape: Rect\n");
    let script_paragraphs = paragraphs(script);
    let first = script_paragraphs[0].id();
    let second = script_paragraphs[1].id();
    let old = paragraphs("/// 第二段。\n")[0].id();
    let timing = dir.path().join("timing.speech.json");

    let transcript = |second_id: &str, text: &str| {
        serde_json::json!({
            "narration_takes": {
                &first: {"start":0,"end":1},
                second_id: {"start":1,"end":2}
            },
            "segments": [
                {"start":0.25,"end":1.15,"text":"第一段。"},
                {"start":2.15,"end":3.65,"text":text}
            ]
        })
        .to_string()
    };
    std::fs::write(&timing, transcript(&old, "第二段。")).unwrap();
    store.source.replace_text(source.clone());
    store.source.document.rebuild().unwrap();
    let mut history = VoiceoverHistory {
        mixes: HashMap::from([(
            None,
            HashMap::from([
                (
                    first.clone(),
                    AdoptedVoiceover {
                        selector: first.clone(),
                        text: "第一段。".into(),
                    },
                ),
                (
                    second.clone(),
                    AdoptedVoiceover {
                        selector: old,
                        text: "第二段。".into(),
                    },
                ),
            ]),
        )]),
        ..Default::default()
    };
    let mut state = ExportDialogState {
        format: ExportFormat::Image,
        time_s: 3.0,
        ..Default::default()
    };
    let affected = |store: &crate::app::stores::DocumentStore,
                    history: &VoiceoverHistory,
                    state: &ExportDialogState| {
        let target = store.source.document.export_target(ExportScope::ActiveScene).unwrap();
        narration::affected_paragraphs(&store.source, history, target, state)
    };

    let warning = affected(&store, &history, &state);
    assert_eq!(warning.len(), 1);
    assert!(warning[0].snippet.contains("第二段"));
    assert_eq!(warning[0].effect, "gui-export-draft-subtitles");
    state.time_s = 0.5;
    assert!(affected(&store, &history, &state).is_empty());
    state.time_s = 3.0;
    state.format = ExportFormat::WebP;
    assert_eq!(affected(&store, &history, &state), warning);

    let hidden =
        source.replace("source: \"narration\", opacity: 1", "source: \"narration\", opacity: 0");
    store.source.replace_text(hidden);
    store.source.document.rebuild().unwrap();
    assert!(affected(&store, &history, &state).is_empty());
    state.format = ExportFormat::Video;
    assert_eq!(affected(&store, &history, &state)[0].effect, "gui-export-draft-audio");
    state.format = ExportFormat::Gif;
    assert!(affected(&store, &history, &state).is_empty());

    let updated = "第二段。 [draft]";
    std::fs::write(dir.path().join("timing-new.speech.json"), transcript(&second, updated))
        .unwrap();
    store
        .source
        .replace_text(source.replace("timing.speech.json", "timing-new.speech.json"));
    store.source.document.rebuild().unwrap();
    history.mixes.get_mut(&None).unwrap().insert(
        second.clone(),
        AdoptedVoiceover {
            selector: second,
            text: updated.into(),
        },
    );
    state.format = ExportFormat::Image;
    assert!(affected(&store, &history, &state).is_empty());
}

#[test]
fn draft_confirmation_cannot_bypass_an_unresolved_time_binding() {
    use animatix_syntax::narration::{AnchorEdge, NarrationAnchor, encode_anchor};

    let marker = encode_anchor(&NarrationAnchor {
        paragraph: "Old passage.".into(),
        paragraph_occurrence: 1,
        phrase: None,
        phrase_occurrence: 1,
        edge: AnchorEdge::Start,
    })
    .unwrap();
    let source = format!(
        "card: Rect, opacity: 0\n/// New passage.\n#narration@{marker}\n  card.opacity = 1\n"
    );
    let mut shell = GuiShell::load(PathBuf::from("missing-export-boundary.amx"), false);
    shell.document_store = crate::app::test_support::make_document_store(&source);
    shell.submit_export(Some(vec![narration::AffectedParagraph {
        scene: None,
        number: 1,
        snippet: "New passage.".into(),
        effect: "gui-export-draft-subtitles",
    }]));
    assert!(matches!(shell.export_store.export_status, ExportStatus::Failed(ref error)
        if error.canonical().contains("Resolve the narration binding")));
    assert!(shell.export_store.export_thread.is_none());
}
