//! Steady-state profiling driver for the frame-evaluation hot path.
//!
//! Criterion-based profiles are contaminated by one-time setup (font database
//! scans, parser warmup, benchmark-suite neighbours) and by the harness itself;
//! `docs/performance_evaluation.md` §4 records that this skewed attribution
//! more than once. This example runs the `stage_breakdown` 60-actor dynamic
//! scenario in a tight loop after a settle phase, so
//!
//! ```text
//! perf record -e cycles:u -c 50001 -- taskset -c 0 \
//!     target/release/examples/perf_driver
//! ```
//!
//! captures steady-state `evaluate` only. Build with
//! `cargo build --release --example perf_driver -p animatix`.
//!
//! For allocation attribution (PF-6) use `alloc_driver`, which runs the same
//! scenario through the dhat allocator.

use std::time::Duration;

#[path = "common/scenario_60actors.rs"]
mod scenario;

fn main() {
    let timeline = scenario::build_timeline();

    // Settle caches / allocator so the loop below is steady state.
    for i in 0..2_000 {
        std::hint::black_box(timeline.evaluate(scenario::frame_time(i), scenario::DIMS));
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    let mut i = 0u64;
    while std::time::Instant::now() < deadline {
        std::hint::black_box(timeline.evaluate(scenario::frame_time(i), scenario::DIMS));
        i += 1;
    }
    println!("frames evaluated: {i}");
}
