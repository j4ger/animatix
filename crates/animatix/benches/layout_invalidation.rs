//! CPU scene evaluation with identical time sequence in every layout mode.
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput};

#[path = "layout_invalidation_workload.rs"]
mod workload;
use workload::{COUNTS, DIMENSIONS, GROUP, Mode, SampleClock, build};

pub(crate) fn bench_layout_invalidation(c: &mut Criterion) {
    let mut group = c.benchmark_group(GROUP);
    for count in COUNTS {
        group.throughput(Throughput::Elements(count as u64));
        for mode in [Mode::Static, Mode::Color, Mode::Size] {
            let timeline = build(count, mode);
            drop(timeline.evaluate(0.5, DIMENSIONS));
            // Persist through Criterion batches, not just individual iterations.
            let mut clock = SampleClock::default();
            group.bench_function(BenchmarkId::new(mode.id(), count), |b| {
                b.iter(|| {
                    let time = clock.next_time();
                    // Includes clock advance, evaluation and returned scene drop.
                    // No generation/build, structured item collection or GPU work.
                    black_box(timeline.evaluate(black_box(time), DIMENSIONS));
                });
            });
        }
    }
    group.finish();
}
