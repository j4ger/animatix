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

use std::time::Duration;

const DIMS: animatix::timeline::SceneDimensions = animatix::timeline::SceneDimensions {
    width: 1920,
    height: 1080,
};
const N_ACTORS: usize = 60;

fn scenario_source() -> String {
    let mut src = String::from(
        r##"config { colorscheme: "editorial-dark", dynamic_layout: true }

#0s
row: Row, at: (120, 300), gap: 8, align: "start" {
  box0: Rect, size: (50, 50), color: accent.primary
"##,
    );
    for i in 1..N_ACTORS {
        src.push_str(&format!("  box{i}: Rect, size: (50, 50), color: accent.primary\n"));
    }
    src.push_str(
        r##"}
always {
  box0.size = (40 + 20 * sin(t), 50)
  box0.at = (80 + 30 * sin(t * 3), 300)
}
"##,
    );
    src
}

fn main() {
    let timeline = {
        let (stmts, _) = animatix_syntax::parser::parse_source(&scenario_source());
        animatix::timeline::Timeline::build(&stmts.expect("parse error"))
    };

    // Settle caches / allocator so the loop below is steady state.
    for i in 0..2_000 {
        std::hint::black_box(timeline.evaluate((i % 100) as f64 * 0.01, DIMS));
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    let mut i = 0u64;
    while std::time::Instant::now() < deadline {
        std::hint::black_box(timeline.evaluate((i % 100) as f64 * 0.01, DIMS));
        i += 1;
    }
    println!("frames evaluated: {i}");
}
