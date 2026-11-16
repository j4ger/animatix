//! Correctness of benchmark inputs and boundaries, not performance assertions.
#[path = "../benches/geometry_count_workload.rs"]
mod workload;

use animatix::timeline::DebugRenderOptions;
use workload::{
    COUNTS, DIMENSIONS, GROUP, Mode, SAMPLE_COUNT, STATIC_TIME, SampleClock, build, source,
};

#[test]
fn geometry_count_manifest_matches_frozen_parameters() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../benches/fixtures/geometry-count-v1/manifest.json"))
            .unwrap();
    assert_eq!(manifest["group"], GROUP);
    assert_eq!(manifest["counts"], serde_json::json!(COUNTS));
    assert_eq!(manifest["revision"], 1);
    assert_eq!(manifest["primitive"], "Rect");
    assert_eq!(manifest["size"], serde_json::json!([24, 24]));
    assert_eq!(
        manifest["grid"],
        serde_json::json!({"columns": 16, "pitch": 56, "origin": [32, 32]})
    );
    assert_eq!(manifest["dimensions"], serde_json::json!([DIMENSIONS.width, DIMENSIONS.height]));
    assert_eq!(manifest["static_time_seconds"], STATIC_TIME);
    assert_eq!(manifest["varying_time_samples"], SAMPLE_COUNT);
    assert_eq!(
        manifest["modes"],
        serde_json::json!([
            Mode::StaticRepeatedTime.id(),
            Mode::AnimatedVaryingTime.id()
        ])
    );
}

#[test]
fn varying_time_changes_every_quantized_frame_even_at_wrap() {
    let mut clock = SampleClock::default();
    let mut previous_ms = None;
    for i in 0..SAMPLE_COUNT * 3 {
        let time = clock.next_time();
        assert_eq!(time, (i % SAMPLE_COUNT) as f64 / 64.0);
        assert!((0.0..1.0).contains(&time));
        let ms = (time * 1000.0) as u64;
        assert_ne!(Some(ms), previous_ms);
        previous_ms = Some(ms);
    }
}

#[test]
fn geometry_inputs_have_expected_visible_count_and_motion() {
    for count in COUNTS {
        for mode in [Mode::StaticRepeatedTime, Mode::AnimatedVaryingTime] {
            let timeline = build(count, mode);
            let mut backend = None;
            for (time_ms, time) in [
                (0, 0.0),
                (250, 0.25),
                (500, 0.5),
                (750, 0.75),
                (984, 63.0 / 64.0),
            ] {
                let frame = timeline.evaluate_program_with_debug(
                    time,
                    DIMENSIONS,
                    DebugRenderOptions::default(),
                    &mut backend,
                );
                assert!(frame.diagnostics.is_empty());
                assert_eq!(frame.items.len(), count, "{mode:?} count at {time}");
                for item in &frame.items {
                    assert_eq!(item.opacity, 1.0);
                    assert!(!item.commands.is_empty());
                    let index: usize = item.label.strip_prefix('r').unwrap().parse().unwrap();
                    assert!(index < count);
                    let base_x = (32 + (index % 16) * 56) as f64;
                    let base_y = (32 + (index / 16) * 56) as f64;
                    let movement = if mode == Mode::AnimatedVaryingTime {
                        8.0 * time_ms as f64 / 1000.0
                    } else {
                        0.0
                    };
                    // Check rendered item transforms as well as the independent
                    // public lookup: endpoints alone would allow a step function
                    // or stale frame/subtree to masquerade as cheap animation.
                    let rendered = item.transform.as_coeffs();
                    let lookup = timeline
                        .actor_world_affine(&item.label, time_ms, DIMENSIONS)
                        .unwrap()
                        .as_coeffs();
                    for actual in [rendered, lookup] {
                        assert!(
                            (actual[4] - (base_x + movement)).abs() < 0.001,
                            "{mode:?} {} at {time_ms}ms: x={} expected {}",
                            item.label,
                            actual[4],
                            base_x + movement
                        );
                        assert!((actual[5] - base_y).abs() < 0.001);
                    }
                }
            }
            for i in 0..count {
                let label = format!("r{i}");
                let first = timeline.actor_world_affine(&label, 0, DIMENSIONS).unwrap().as_coeffs();
                let last =
                    timeline.actor_world_affine(&label, 1000, DIMENSIONS).unwrap().as_coeffs();
                let expected_x = (32 + (i % 16) * 56) as f64;
                let expected_y = (32 + (i / 16) * 56) as f64;
                assert_eq!(first[4], expected_x);
                assert_eq!(first[5], expected_y);
                let movement = if mode == Mode::AnimatedVaryingTime {
                    8.0
                } else {
                    0.0
                };
                assert_eq!(last[4] - first[4], movement);
                assert_eq!(last[5], first[5]);
                assert!(expected_x - 12.0 >= 0.0 && last[4] + 12.0 < DIMENSIONS.width as f64);
                assert!(expected_y - 12.0 >= 0.0 && last[5] + 12.0 < DIMENSIONS.height as f64);
            }
        }
    }
}

#[test]
fn source_bytes_are_frozen_and_unversioned_counts_are_rejected() {
    // Non-cryptographic snapshot checks, not provenance/security hashes. SHA256
    // fingerprints of the generator/runner are recorded separately in META.json.
    let expected = [
        (16, Mode::StaticRepeatedTime, 0xc177f60fff93a58e_u64),
        (16, Mode::AnimatedVaryingTime, 0x250ab3b8092a650c),
        (64, Mode::StaticRepeatedTime, 0x8b1a764f98d17ff2),
        (64, Mode::AnimatedVaryingTime, 0xcba773c35f6d226),
        (256, Mode::StaticRepeatedTime, 0xc71d024cbbcc011e),
        (256, Mode::AnimatedVaryingTime, 0x33096cf93663eea),
    ];
    for (count, mode, hash) in expected {
        let source = source(count, mode);
        let actual = source.bytes().fold(0xcbf29ce484222325_u64, |state, byte| {
            (state ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        assert_eq!(actual, hash, "{count} {mode:?}: change workload version if inputs change");
        assert_eq!(source.matches(": Rect,").count(), count);
    }
    assert!(std::panic::catch_unwind(|| source(17, Mode::StaticRepeatedTime)).is_err());
}
