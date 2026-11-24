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
