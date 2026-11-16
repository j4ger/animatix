//! CPU scene evaluation only; no GPU rendering or readback and no cold-start claim.
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput};

#[path = "geometry_count_workload.rs"]
mod workload;
use workload::{COUNTS, DIMENSIONS, GROUP, Mode, STATIC_TIME, SampleClock, build};

pub(crate) fn bench_geometry_count(c: &mut Criterion) {
    let mut group = c.benchmark_group(GROUP);
    for count in COUNTS {
        // Criterion's elements/s means authored Rects, not pixels or GPU draws.
        group.throughput(Throughput::Elements(count as u64));
        for mode in [Mode::StaticRepeatedTime, Mode::AnimatedVaryingTime] {
            let timeline = build(count, mode);
            // Warm setup outside measurement. The static case is eligible for
            // whole-frame reuse; no unobservable cache-hit count is claimed.
            drop(timeline.evaluate(STATIC_TIME, DIMENSIONS));
            // Persist across Criterion batches as well as individual iterations.
            let mut clock = SampleClock::default();
            group.bench_function(BenchmarkId::new(mode.id(), count), |b| {
                b.iter(|| {
                    let time = match mode {
                        Mode::StaticRepeatedTime => STATIC_TIME,
                        Mode::AnimatedVaryingTime => clock.next_time(),
                    };
                    // Includes dispatch/time selection, evaluate, returned Scene
                    // destruction. Parse, build, rasterization, I/O are excluded.
                    black_box(timeline.evaluate(black_box(time), DIMENSIONS));
                });
            });
        }
    }
    group.finish();
}
