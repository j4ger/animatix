use super::*;

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
    let mut writer = CaptureWav::new(24000, 2).unwrap();
    writer.limit = 6;
    writer.append(&[50, -50, 100]).unwrap();
    writer.append(&[-100, 150, -150, 500, -500]).unwrap();
    assert!(writer.full());

    let capture = writer.finish().unwrap();
    let mut reader = hound::WavReader::open(capture.path()).unwrap();
    assert_eq!(reader.spec().sample_rate, 24000);
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(reader.spec().bits_per_sample, 16);
    assert_eq!(
        reader.samples::<i16>().collect::<hound::Result<Vec<_>>>().unwrap(),
        [50, -50, 100, -100, 150, -150],
    );
    assert_eq!(capture.seconds, 3.0 / 24000.0);
    assert!(capture.reached_limit);
    let path = capture.path().to_owned();
    drop(reader);
    drop(capture);
    assert!(!path.exists());
}

#[test]
fn discarded_capture_cleans_up_partially_written_file() {
    let mut writer = CaptureWav::new(24000, 1).unwrap();
    writer.append(&[0; 64]).unwrap();
    let path = writer.wav.path().to_owned();
    drop(writer);
    assert!(!path.exists());
}

#[test]
#[ignore = "requires a default microphone or virtual audio capture device"]
fn default_microphone_can_record_a_temporary_wav() {
    let mut capture = Capture::start(egui::Context::default()).unwrap();
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
    let recording = loop {
        if let Some(result) = capture.poll() {
            break result.unwrap();
        }
        assert!(Instant::now() < timeout);
        std::thread::sleep(Duration::from_millis(2));
    };
    assert!(recording.seconds > 0.0);
    let wav = hound::WavReader::open(recording.path()).unwrap();
    assert!(wav.duration() > 0);
}
