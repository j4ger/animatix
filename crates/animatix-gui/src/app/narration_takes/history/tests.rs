use super::*;

fn take() -> Take {
    Take {
        id: "take".into(),
        paragraph: "paragraph".into(),
        scene: None,
        text: "Hello.".into(),
        kind: "recorded",
    }
}

#[test]
fn committed_audio_survives_an_interrupted_capture_and_a_copy() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("demo.amx");
    let history = History::open(&source, true).unwrap().unwrap();
    let take = take();
    history.start_recording(&take, 8_000, 2).unwrap();
    history.append_recording(&take.id, &[1, -2, 3, -4], 2).unwrap();
    let second = History::open(&source, false).unwrap().unwrap();
    assert!(!second.is_writable());
    assert!(second.takes().unwrap().is_empty());
    assert!(second.finish_recording(&take.id, true).is_err());
    history.snapshot("live source", &[], true).unwrap();
    assert!(second.bindings("live source").unwrap().0);
    drop(second);
    drop(history);
    let history = History::open(&source, false).unwrap().unwrap();
    assert_eq!(history.takes().unwrap()[0].status, "interrupted");
    let copy = dir.path().join("other.amx");
    history.backup(&path_for(&copy)).unwrap();
    let copy = History::open(&copy, false).unwrap().unwrap();
    let audio = dir.path().join("recovered.wav");
    copy.raw_wave(&take.id, &audio).unwrap();
    let mut reader = hound::WavReader::open(audio).unwrap();
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(
        reader.samples::<i16>().collect::<std::result::Result<Vec<_>, _>>().unwrap(),
        &[1, -2, 3, -4]
    );
}

#[test]
fn unknown_schema_stays_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("demo.amx");
    let path = path_for(&source);
    fs::write(&path, b"external format").unwrap();
    assert!(History::open(&source, true).is_err());
    assert_eq!(fs::read(path).unwrap(), b"external format");
}

#[test]
fn sqlite_history_is_rejected_without_changing_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scene.amx");
    let legacy = source.with_extension("amxn");
    let bytes = b"SQLite format 3\0legacy fixture";
    fs::write(&legacy, bytes).unwrap();

    assert_eq!(path_for(&source), legacy);
    assert!(History::open(&source, false).is_err());
    assert!(History::open(&source, true).is_err());
    assert!(History::read_only(&source).is_err());
    assert_eq!(fs::read(legacy).unwrap(), bytes);
}

#[test]
fn unsupported_redb_schema_is_rejected_before_opening_for_writes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scene.amx");
    let path = path_for(&source);
    install(&path, |tx| {
        tx.open_table(STATE)?.insert((), (VERSION + 1, "", 0))?;
        Ok(())
    })
    .unwrap();
    let before = fs::read(&path).unwrap();

    assert!(matches!(History::open(&source, true), Err(Error::Invalid(UNSUPPORTED))));
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn source_bindings_prefer_exact_drafts_and_fall_back_to_last_save() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scene.amx");
    let history = History::open(&source, true).unwrap().unwrap();
    let binding = |id: &str| Binding {
        scene: Some("intro".into()),
        selector: format!("p-{id}"),
        text: format!("Text {id}"),
        id: id.into(),
    };
    history.snapshot("saved", &[binding("one"), binding("two")], true).unwrap();
    history.snapshot("draft", &[binding("three")], false).unwrap();
    history.snapshot("empty", &[], false).unwrap();
    assert!(matches!(history.bindings("empty").unwrap(), (true, rows) if rows.is_empty()));
    let (exact, rows) = history.bindings("changed").unwrap();
    assert!(!exact);
    assert_eq!(rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(rows[0].scene.as_deref(), Some("intro"));
    assert_eq!(rows[0].selector, "p-one");
    assert_eq!(rows[0].text, "Text one");

    history.snapshot("draft", &[binding("three")], true).unwrap();
    history.snapshot("draft", &[binding("four")], false).unwrap();
    drop(history);
    let history = History::open(&source, false).unwrap().unwrap();
    assert!(matches!(history.bindings("saved").unwrap(), (true, rows) if rows[1].id == "two"));
    assert!(matches!(history.bindings("draft").unwrap(), (true, rows) if rows[0].id == "four"));
    assert!(matches!(history.bindings("changed").unwrap(), (false, rows) if rows[0].id == "four"));
}

#[test]
fn reopen_discards_empty_recordings_and_orders_takes_across_commits() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scene.amx");
    let history = History::open(&source, true).unwrap().unwrap();
    let first = Take {
        id: "z".into(),
        ..take()
    };
    let second = Take {
        id: "a".into(),
        ..take()
    };
    let empty = Take {
        id: "empty".into(),
        ..take()
    };

    history.start_recording(&first, 8_000, 2).unwrap();
    assert!(history.append_recording(&first.id, &[7, 8], 1).is_err());
    history.append_recording(&first.id, &[1, 2], 2).unwrap();
    history.finish_recording(&first.id, true).unwrap();
    history.start_recording(&second, 8_000, 1).unwrap();
    history.append_recording(&second.id, &[-1, -2], 1).unwrap();
    history.start_recording(&empty, 8_000, 1).unwrap();
    drop(history);

    let history = History::open(&source, false).unwrap().unwrap();
    let takes = history.takes().unwrap();
    assert_eq!(takes.iter().map(|take| take.id.as_str()).collect::<Vec<_>>(), ["a", "z"]);
    assert_eq!(
        (takes[0].status.as_str(), takes[1].status.as_str()),
        ("interrupted", "complete")
    );
    assert_eq!(takes[0].seconds, 2.0 / 8_000.0);
    assert_eq!(takes[1].seconds, 1.0 / 8_000.0);
    history.start_recording(&empty, 8_000, 1).unwrap();
    history.finish_recording(&empty.id, false).unwrap();
    history.start_recording(&empty, 8_000, 1).unwrap();

    let other = History::open(&dir.path().join("other.amx"), true).unwrap().unwrap();
    assert!(other.takes().unwrap().is_empty());
    other.start_recording(&first, 8_000, 1).unwrap();
}

#[test]
fn alignment_replacement_and_mix_references_survive_backup() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scene.amx");
    let history = History::open(&source, true).unwrap().unwrap();
    let take = Take {
        kind: "imported",
        ..take()
    };
    let wave = dir.path().join("take.wav");
    let raw: Vec<_> = (0..40_000).map(|n| (n % 1_009) as i16 - 504).collect();
    write_fixture_wave(&wave, &raw);
    assert!(history.import_wave(&take, &dir.path().join("missing.wav")).is_err());
    history.import_wave(&take, &wave).unwrap();
    let raw_copy = dir.path().join("raw.wav");
    history.raw_wave(&take.id, &raw_copy).unwrap();
    assert_eq!(samples(&raw_copy), raw);

    let candidate = dir.path().join("take.json");
    let transcript = dir.path().join("take.speech.json");
    fs::write(&candidate, br#"{"paragraph":"old","note":"retained"}"#).unwrap();
    fs::write(&transcript, b"fixture transcript").unwrap();
    history.issue(&take.id, Some("Needs review")).unwrap();
    history.store_alignment(&take.id, &candidate).unwrap();
    assert!(history.takes().unwrap()[0].aligned);
    assert!(history.takes().unwrap()[0].issue.is_none());

    // An invalid replacement rolls back the prior stream and alignment.
    fs::write(&wave, b"invalid wave").unwrap();
    assert!(history.store_alignment(&take.id, &candidate).is_err());
    let restored = dir.path().join("restored");
    history.candidate(&take.id, "current", &restored).unwrap();
    assert_eq!(samples(&restored.join("take.wav")), raw);

    // A shorter replacement removes the old stream's trailing blocks only.
    write_fixture_wave(&wave, &[-10, 10]);
    history.store_alignment(&take.id, &candidate).unwrap();
    let audio = Path::new("narration/voice.wav");
    let timing = Path::new("narration/voice.speech.json");
    let mut mix = Mix {
        audio_digest: "audio".into(),
        transcript_digest: "transcript".into(),
        layout: vec![0, 255],
        parts: [("current".into(), "missing".into())].into(),
    };
    assert!(history.store_mix(audio, timing, &mix).is_err());
    assert!(history.mix(audio, timing).unwrap().is_none());
    mix.parts.insert("current".into(), take.id.clone());
    history.store_mix(audio, timing, &mix).unwrap();
    mix.audio_digest = "replacement".into();
    history.store_mix(audio, timing, &mix).unwrap();

    let fork = dir.path().join("fork.amx");
    let fork_path = path_for(&fork);
    history.backup(&fork_path).unwrap();
    assert!(history.backup(&fork_path).is_err());
    let fork = History::open(&fork, false).unwrap().unwrap();
    let mix = fork.mix(audio, timing).unwrap().unwrap();
    assert_eq!(mix.audio_digest, "audio");
    assert_eq!(mix.transcript_digest, "transcript");
    assert_eq!(mix.layout, [0, 255]);
    assert_eq!(mix.parts["current"], take.id);
    fork.candidate(&take.id, "new selector", &restored).unwrap();
    assert_eq!(samples(&restored.join("take.wav")), [-10, 10]);
    assert_eq!(fs::read(restored.join("take.speech.json")).unwrap(), b"fixture transcript");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(restored.join("take.json")).unwrap()).unwrap();
    assert_eq!(receipt["paragraph"], "new selector");
    assert_eq!(receipt["note"], "retained");
    fork.raw_wave(&take.id, &raw_copy).unwrap();
    assert_eq!(samples(&raw_copy), raw);
}

fn write_fixture_wave(path: &Path, samples: &[i16]) {
    let mut writer = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 1,
            sample_rate: 8_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for &sample in samples {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
}

fn samples(path: &Path) -> Vec<i16> {
    hound::WavReader::open(path)
        .unwrap()
        .samples::<i16>()
        .map(std::result::Result::unwrap)
        .collect()
}
