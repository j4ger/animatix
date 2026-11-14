use super::*;

#[test]
fn stereo_channels_preserve_opposite_peaks_without_cancellation() {
    let waveform = Waveform::from_samples([0.8, -0.9, 0.2, -0.1].into_iter(), 2, 2);
    assert_eq!(waveform.duration_s, 1.0);
    assert_eq!(
        waveform.columns(0.0, 1.0, 1),
        vec![Peak {
            min: -0.9,
            max: 0.8
        }]
    );
}

#[test]
fn silence_empty_and_invalid_formats_are_safe() {
    for waveform in [
        Waveform::from_samples([0.0; 512].into_iter(), 256, 1),
        Waveform::from_samples(std::iter::empty(), 48_000, 2),
        Waveform::from_samples([1.0].into_iter(), 0, 1),
    ] {
        assert!(waveform.columns(-1.0, 4.0, 32).iter().all(|p| *p == Peak::default()));
    }
}

#[test]
fn negative_and_out_of_range_queries_do_not_leak_endpoint_peaks() {
    let waveform = Waveform::from_samples([1.0; 512].into_iter(), 256, 1);
    assert_eq!(waveform.columns(-2.0, -1.0, 4), vec![Peak::default(); 4]);
    assert_eq!(waveform.columns(2.0, 3.0, 4), vec![Peak::default(); 4]);
    assert!(waveform.columns(1.0, 1.0, 4).is_empty());
    assert!(waveform.columns(f64::NAN, 1.0, 4).is_empty());
    assert!(waveform.columns(-f64::MAX, f64::MAX, 4).is_empty());
    assert!(waveform.columns(0.0, 1.0, 0).is_empty());
}

#[test]
fn pyramid_retains_short_transients_and_partial_tail() {
    let mut samples = vec![0.0; 4097];
    samples[512] = 1.0;
    samples[4096] = -0.75;
    let waveform = Waveform::from_samples(samples.into_iter(), 1024, 1);
    assert_eq!(
        waveform.columns(0.0, 5.0, 1),
        vec![Peak {
            min: -0.75,
            max: 1.0
        }]
    );
    assert!(waveform.columns(0.5, 0.75, 8).iter().all(|peak| peak.max == 1.0));
    assert!(
        waveform
            .levels
            .windows(2)
            .all(|pair| pair[1].len() == pair[0].len().div_ceil(2))
    );
}

#[test]
fn invalid_samples_cannot_poison_summary() {
    let waveform = Waveform::from_samples([f32::NAN, f32::INFINITY, -2.0, 2.0].into_iter(), 4, 1);
    assert_eq!(
        waveform.columns(0.0, 1.0, 1),
        vec![Peak {
            min: -1.0,
            max: 1.0
        }]
    );
}

#[test]
fn long_stream_compacts_without_losing_duration_or_peaks() {
    let count = MAX_BINS * BASE_FRAMES + 1;
    let waveform = Waveform::from_samples(
        (0..count).map(|index| if index == count - 1 { -0.6 } else { 0.25 }),
        48_000,
        1,
    );
    assert!(waveform.levels[0].len() <= MAX_BINS);
    assert!((waveform.duration_s - count as f64 / 48_000.0).abs() < 1e-9);
    assert_eq!(
        waveform.columns(0.0, waveform.duration_s, 1),
        vec![Peak {
            min: -0.6,
            max: 0.25
        }]
    );
    assert_eq!(waveform.columns(0.0, 1.0, usize::MAX).len(), 4096);
}

#[test]
fn cache_sticky_failures_do_not_spawn_workers_each_frame() {
    let cache = WaveformCache::default();
    let context = egui::Context::default();
    {
        let mut state = cache.0.lock().unwrap();
        state.revision = 4;
        state.entries.insert("missing.wav".into(), Entry::Failed("not found".into()));
    }
    assert!(matches!(cache.get("missing.wav", 4, &context), WaveformState::Failed(_)));
    assert_eq!(cache.0.lock().unwrap().active_workers.load(Ordering::Relaxed), 0);
}
