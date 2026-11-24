use super::*;
use crate::app::document::narration::{NarrationTrack, VoiceoverStatus, voiceover};
use crate::app::narration_takes::worker::ImportTiming;
use crate::app::test_support::document_store_loaded;
use animatix_syntax::narration::paragraphs;

#[test]
fn pending_import_survives_reopen_and_stays_with_its_paragraph() {
    let original =
        "# Intro\n/// 同一句。\n#0s\n  shape: Rect\n# Demo\n/// 同一句。\n#0s\n  shape: Rect\n";
    let (dir, store) = document_store_loaded(original);
    let path = store.source.file_path().to_owned();
    let audio = dir.path().join("plain.wav");
    let mut wav = hound::WavWriter::create(
        &audio,
        hound::WavSpec {
            channels: 1,
            sample_rate: 24000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for _ in 0..2_400 {
        wav.write_sample(100i16).unwrap();
    }
    wav.finalize().unwrap();

    let script = paragraphs(original);
    let mut shell = GuiShell::load(path.clone(), false);
    shell.document_store = store;
    let ctx = Context::default();
    shell.narration_takes.open_paragraph(
        &shell.document_store.source,
        script[1].scene.clone(),
        &script[1].id(),
    );
    shell
        .narration_action(
            &ctx,
            Action::Start(
                Operation::Import {
                    audio: Some(audio),
                    timing: ImportTiming::Missing,
                },
                None,
            ),
        )
        .unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while shell.narration_takes.job.is_some() {
        assert!(std::time::Instant::now() < until, "import worker did not finish");
        shell.finish_narration_job();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(shell.document_store.source.text(), original);

    // Opening the source is enough to show pending alignment in the editor.
    drop(shell);
    let mut reopened = GuiShell::load(path, false);
    let source = &reopened.document_store.source;
    reopened.narration_takes.update_voiceover_history(source);
    let check = |shell: &GuiShell, text: &str| {
        let source = &shell.document_store.source;
        let track = source.document.active_timeline().and_then(NarrationTrack::find);
        paragraphs(text)
            .iter()
            .map(|paragraph| {
                voiceover(&shell.narration_takes.voiceover_history, track.as_ref(), paragraph, &[])
                    .0
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(check(&reopened, original), [VoiceoverStatus::Missing, VoiceoverStatus::Pending]);
    let revision = reopened.narration_takes.voiceover_history.revision;
    reopened
        .narration_takes
        .update_voiceover_history(&reopened.document_store.source);
    assert_eq!(reopened.narration_takes.voiceover_history.revision, revision);

    reopened.document_store.source.document.invalidate_media_identity();
    reopened
        .narration_takes
        .update_voiceover_history(&reopened.document_store.source);
    assert_ne!(reopened.narration_takes.voiceover_history.revision, revision);

    let mut edited = original.to_owned();
    let end = edited.rfind("同一句。").unwrap() + "同一句。".len();
    edited.insert_str(end, " [draft]");
    reopened.document_store.source.replace_text(edited.clone());
    reopened
        .narration_takes
        .update_voiceover_history(&reopened.document_store.source);
    assert_eq!(check(&reopened, &edited), [VoiceoverStatus::Missing, VoiceoverStatus::Missing]);
}
