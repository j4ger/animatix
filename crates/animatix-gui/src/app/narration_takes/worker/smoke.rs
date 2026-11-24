use super::*;

#[test]
#[ignore = "requires Python 3 on PATH"]
fn bundled_worker_supports_comparison_and_direct_voiceover_updates() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().canonicalize().unwrap();
    let audio = folder.join("recorded.wav");
    let timing = folder.join("recorded.json");
    let python = if cfg!(windows) { "python" } else { "python3" };
    let generator = r#"
import sys, wave
with wave.open(sys.argv[1], 'wb') as output:
    output.setparams((1, 2, 24000, 0, 'NONE', 'not compressed'))
    output.writeframes(bytes(4800))
"#;
    assert!(
        Command::new(python)
            .args(["-c", generator])
            .arg(&audio)
            .status()
            .unwrap()
            .success()
    );
    fs::write(&timing, r#"[{"word":"Hello.","start":0.01,"end":0.08}]"#).unwrap();
    let request = |operation| Request {
        folder: folder.clone(),
        output_prefix: folder.join("voice"),
        paragraphs: vec![ManifestParagraph {
            id: "p_ab_1".into(),
            text: "Hello.".into(),
        }],
        base: None,
        anchors: Vec::new(),
        python: python.into(),
        operation,
    };
    let cancel = AtomicBool::new(false);
    let Output::Scene(scene, Some(candidate)) = run(
        request(Operation::Import {
            paragraph: "p_ab_1".into(),
            audio: RecordingInput::File(audio.clone()),
            timing: ImportTiming::Sidecar(timing.clone()),
            apply_revision: None,
        }),
        &cancel,
    )
    .unwrap() else {
        panic!("expected staged take")
    };
    assert_eq!(scene.revision, "empty");
    assert_eq!(scene.paragraphs[0].candidates[0].word_count, 1);
    assert!(scene.paragraphs[0].active_take.is_none());

    let Output::Adopted(adopted) = run(
        request(Operation::Adopt {
            paragraph: "p_ab_1".into(),
            candidate,
            expected_revision: scene.revision,
        }),
        &cancel,
    )
    .unwrap() else {
        panic!("expected versioned assets")
    };
    assert!(adopted.audio.is_file());
    assert!(adopted.transcript.is_file());
    assert!(!folder.join("voice.takes/current.json").exists());

    let Output::Adopted(updated) = run(
        request(Operation::Import {
            paragraph: "p_ab_1".into(),
            audio: RecordingInput::File(audio),
            timing: ImportTiming::Sidecar(timing),
            apply_revision: Some("empty".into()),
        }),
        &cancel,
    )
    .unwrap() else {
        panic!("the update should return the audio and transcript in one job");
    };
    assert!(updated.audio.is_file());
    assert!(updated.transcript.is_file());
    assert!(adopted.audio.is_file());
    assert!(!folder.join("voice.takes/current.json").exists());

    let provider =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/quality/test-speech-chapter.py");
    let Output::Adopted(synthesized) = run(
        request(Operation::Synthesize {
            paragraph: "p_ab_1".into(),
            voice: "test-voice".into(),
            rate: "+0%".into(),
            provider: Provider::Local {
                executable: python.into(),
                arguments: vec![provider.to_str().unwrap().to_owned(), "--fixture".into()],
                revision: String::new(),
            },
            apply_revision: Some("empty".into()),
        }),
        &cancel,
    )
    .unwrap() else {
        panic!("the generated voiceover should also apply in one job");
    };
    assert!(synthesized.audio.is_file());
    assert!(synthesized.transcript.is_file());
}
