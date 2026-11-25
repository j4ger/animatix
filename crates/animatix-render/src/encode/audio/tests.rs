use super::*;

const SAMPLE_RATE: u32 = 44_100;

fn write_wave(path: &std::path::Path) {
    let byte_count = SAMPLE_RATE * 2;
    let mut wave = Vec::new();
    wave.extend(b"RIFF");
    wave.extend((36 + byte_count).to_le_bytes());
    wave.extend(b"WAVEfmt ");
    wave.extend(16_u32.to_le_bytes()); // PCM format chunk size
    wave.extend(1_u16.to_le_bytes()); // linear PCM
    wave.extend(1_u16.to_le_bytes()); // mono
    wave.extend(SAMPLE_RATE.to_le_bytes());
    wave.extend((SAMPLE_RATE * 2).to_le_bytes());
    wave.extend(2_u16.to_le_bytes()); // bytes per frame
    wave.extend(16_u16.to_le_bytes());
    wave.extend(b"data");
    wave.extend(byte_count.to_le_bytes());
    for sample in 0..SAMPLE_RATE {
        wave.extend(((sample / 2) as i16 - 10_000).to_le_bytes());
    }
    std::fs::write(path, wave).unwrap();
}

fn mixed_samples(inputs: &[AudioInput<'_>], range: Range<f64>, format: &AudioFormat) -> Vec<f32> {
    let graph = mix_graph(inputs, format, range, 0).unwrap();
    let mut sink = graph.get_filter(c"output").unwrap();
    let mut samples = Vec::new();
    loop {
        let frame = match sink.buffersink_get_frame(None) {
            Ok(frame) => frame,
            Err(RsmpegError::BufferSinkEofError) => break,
            Err(error) => panic!("audio graph failed: {error:?}"),
        };
        assert_eq!(frame.format, ffi::AV_SAMPLE_FMT_FLTP);
        // SAFETY: planar float output owns at least nb_samples initialized
        // samples in each channel plane until the frame is dropped.
        let left_channel = unsafe {
            std::slice::from_raw_parts(frame.data[0].cast::<f32>(), frame.nb_samples as usize)
        };
        samples.extend_from_slice(left_channel);
    }
    samples
}

#[test]
fn chapter_mix_crops_global_samples_and_preserves_overlapping_gain_automation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.wav");
    write_wave(&path);
    let first = AudioSegment {
        actor_id: None,
        source: path.to_str().unwrap().into(),
        start_time_s: 0.0,
        source_offset_s: 0.0,
        duration_s: None,
        volume: 1.0,
        volume_envelope: Some(vec![(0.0, 1.0), (0.7, 0.3), (1.0, 0.3)]),
    };
    let second = AudioSegment {
        start_time_s: 0.4,
        source_offset_s: 0.0,
        volume: 0.25,
        volume_envelope: None,
        ..first.clone()
    };
    let inputs = [
        AudioInput::open(&first).unwrap(),
        AudioInput::open(&second).unwrap(),
    ];
    let format = AudioFormat::for_container(false);
    let whole = mixed_samples(&inputs, 0.0..1.4, &format);
    let chapter = mixed_samples(&inputs, 0.37..0.82, &format);
    let expected = &whole[format.samples(0.37) as usize..format.samples(0.82) as usize];
    assert_eq!(chapter, expected);
    assert!(chapter.iter().any(|sample| sample.abs() > 0.05));
}

#[test]
fn source_trim_preserves_the_selected_pcm_and_scene_offset() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.wav");
    write_wave(&path);
    let source = AudioSegment {
        actor_id: None,
        source: path.to_str().unwrap().into(),
        start_time_s: 0.0,
        source_offset_s: 0.0,
        duration_s: None,
        volume: 1.0,
        volume_envelope: None,
    };
    let clip = AudioSegment {
        start_time_s: 0.1,
        source_offset_s: 0.5,
        duration_s: Some(0.25),
        ..source.clone()
    };
    let format = AudioFormat::for_container(false);
    let original = mixed_samples(&[AudioInput::open(&source).unwrap()], 0.0..1.0, &format);
    let mixed = mixed_samples(&[AudioInput::open(&clip).unwrap()], 0.0..0.5, &format);
    let sample = |t| format.samples(t) as usize;
    assert!(mixed[..sample(0.1)].iter().all(|&value| value == 0.0));
    assert_eq!(&mixed[sample(0.1)..sample(0.35)], &original[sample(0.5)..sample(0.75)]);
    assert!(mixed[sample(0.35)..].iter().all(|&value| value == 0.0));
}
