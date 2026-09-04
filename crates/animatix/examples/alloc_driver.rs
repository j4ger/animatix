//! Steady-state allocation driver for the frame-evaluation hot path (PF-6,
//! `docs/performance_evaluation.md` §5 P3).
//!
//! Time profiles rank where cycles go, but the hot path's dominant slice is
//! string-keyed map machinery + allocator work; allocation counts say *what*
//! is allocated and how often, which is the evidence needed to rank the next
//! hot-path targets. This example runs the same 60-actor dynamic scenario as
//! `perf_driver`, but installs a `dhat` profiler *after* the settle phase so
//! the `dhat-heap.json` capture covers only the steady-state `evaluate` loop.
//!
//! Usage:
//!
//! ```text
//! RUSTFLAGS="-C force-frame-pointers=yes" \
//!     cargo build --profile profiling --example alloc_driver -p animatix
//! target/profiling/examples/alloc_driver
//! # inspect dhat-heap.json with the dhat viewer
//! # (https://nnethercote.github.io/dh_html/dh_view.html)
//! ```
//!
//! Frame pointers matter here: release builds without
//! `-C force-frame-pointers=yes` lose backtrace frames and the attribution
//! collapses into `dhat`'s own machinery. `ALLOC_DRIVER_FRAMES` (default
//! 10000) scales the measured window; the JSON aggregates by call site, so it
//! does not grow with the frame count.

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[path = "scenario_60actors.rs"]
mod scenario;

fn main() {
    let timeline = scenario::build_timeline();

    // Settle caches before the profiler exists, so one-time setup (parse,
    // build, first-frame caches) stays out of the capture entirely.
    for i in 0..2_000 {
        std::hint::black_box(timeline.evaluate(scenario::frame_time(i), scenario::DIMS));
    }

    let profiler = dhat::Profiler::builder().file_name("dhat-heap.json").build();
    let start = dhat::HeapStats::get();

    let frames: u64 = std::env::var("ALLOC_DRIVER_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    for i in 0..frames {
        std::hint::black_box(timeline.evaluate(scenario::frame_time(i), scenario::DIMS));
    }

    let end = dhat::HeapStats::get();
    // Drop before printing so the JSON does not capture the report's own
    // allocations; `dhat` writes the file on drop.
    drop(profiler);

    // dhat mixes field types: totals are u64, current/peak are usize.
    let per_frame_u64 = |a: u64, b: u64| -> f64 { a.saturating_sub(b) as f64 / frames as f64 };
    let per_frame_usz = |a: usize, b: usize| -> f64 { a.saturating_sub(b) as f64 / frames as f64 };
    println!("steady-state per-frame allocations ({frames} frames, 60-actor dynamic scene):");
    println!("  total blocks:  {:>10.1}", per_frame_u64(end.total_blocks, start.total_blocks));
    println!("  total bytes:   {:>10.1}", per_frame_u64(end.total_bytes, start.total_bytes));
    println!("  live blocks:   {:>10.4}", per_frame_usz(end.curr_blocks, start.curr_blocks));
    println!("  live bytes:    {:>10.1}", per_frame_usz(end.curr_bytes, start.curr_bytes));
    println!("  peak blocks:   {:>10}", end.max_blocks);
    println!("  peak bytes:    {:>10}", end.max_bytes);
    println!("wrote dhat-heap.json (site-level ranking; viewer link in the doc comment)");
}
