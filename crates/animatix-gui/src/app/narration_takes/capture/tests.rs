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
fn microphone_samples_convert_to_signed_pcm_without_wrapping() {
    assert_eq!(
        [-2.0, -1.0, -0.5, f32::NAN, 0.0, 0.5, 1.0, 2.0].map(pcm16),
        [i16::MIN, i16::MIN, -16384, 0, 0, 16384, i16::MAX, i16::MAX],
    );
    assert_eq!(pcm16(u16::EQUILIBRIUM.to_sample::<f32>()), 0);
    assert_eq!(pcm16(i16::MIN.to_sample::<f32>()), i16::MIN);
}

#[test]
fn capture_writes_and_finalizes_interleaved_pcm_in_incremental_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let history = History::open(&dir.path().join("source.amx"), true).unwrap().unwrap();
    let take = take();
    let mut writer = Recording::new(&history, &take, 24_000, 2).unwrap();
    writer.limit = 6;
    writer.batch = 2;
    writer.append(&[50, -50, 100]).unwrap();
    writer.append(&[-100, 150, -150, 500, -500]).unwrap();
    assert!(writer.full());
    writer.finish().unwrap();

    let wave = dir.path().join("recorded.wav");
    history.raw_wave(&take.id, &wave).unwrap();
    let mut reader = hound::WavReader::open(wave).unwrap();
    assert_eq!(reader.spec().sample_rate, 24_000);
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(reader.spec().bits_per_sample, 16);
    assert_eq!(
        reader.samples::<i16>().collect::<hound::Result<Vec<_>>>().unwrap(),
        [50, -50, 100, -100, 150, -150],
    );
}

#[test]
#[ignore = "requires a default microphone or virtual audio capture device"]
fn default_microphone_commits_audio_to_history() {
    let dir = tempfile::tempdir().unwrap();
    let history = Arc::new(History::open(&dir.path().join("source.amx"), true).unwrap().unwrap());
    let mut capture =
        Capture::start(egui::Context::default(), Arc::clone(&history), take()).unwrap();
    let timeout = Instant::now() + Duration::from_secs(5);
    loop {
        if capture.elapsed().is_some_and(|elapsed| elapsed >= Duration::from_millis(250)) {
            break;
        }
        if let Some(result) = capture.poll() {
            panic!("Microphone stopped before testing Stop: {:?}", result.err());
        }
        assert!(Instant::now() < timeout);
        std::thread::sleep(Duration::from_millis(2));
    }
    capture.stop();
    loop {
        if let Some(result) = capture.poll() {
            result.unwrap();
            break;
        }
        assert!(Instant::now() < timeout);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(history.takes().unwrap()[0].seconds > 0.0);
}
