//! One-off diagnostic: does a fixed-time evaluate loop leak or fragment?
//!
//! Phase 1 (100k frames at t=0.5, no criterion) — dhat HeapStats sampled at
//! intervals. If `total_bytes` grows but `curr_bytes` stays flat, the memory
//! is allocator fragmentation from per-frame churn (not a live leak). If
//! `curr_bytes` grows with it, there is a genuine retained-allocation leak.
//! Phase 2 (10k frames) measures the same loop under the dhat ALLOCATOR to
//! see whether dhat's unbounded site table is what explodes RSS.

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[path = "common/scenario_60actors.rs"]
mod scenario;

use animatix::timeline::{SceneDimensions, Timeline};

fn main() {
    // Same shape as scene_costs::build_many_actors_scene (50 static rects).
    let mut source = String::from(
        r#"config { colorscheme: "editorial-dark" }

#0s
"#,
    );
    for i in 0..50 {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + i * 30,
            100 + (i % 10) * 50
        ));
    }
    let (stmts, _) = animatix_syntax::parser::parse_source(&source);
    let timeline = Timeline::build(&stmts.expect("parse error"));
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    println!("== Phase 1: dhat stats over 100k frames at fixed t ==");
    let profiler = dhat::Profiler::builder().file_name("/tmp/leak_probe_dhat.json").build();
    let start = dhat::HeapStats::get();
    for i in 0..5 {
        for _ in 0..20_000 {
            std::hint::black_box(timeline.evaluate(0.5, dims));
        }
        let s = dhat::HeapStats::get();
        println!(
            "frames {:>6}: total_blocks +{}, total_bytes +{:.1} MB, curr_bytes {:.2} MB, max_bytes {:.2} MB",
            (i + 1) * 20_000,
            s.total_blocks - start.total_blocks,
            (s.total_bytes - start.total_bytes) as f64 / 1e6,
            s.curr_bytes as f64 / 1e6,
            s.max_bytes as f64 / 1e6,
        );
    }

    println!("== Phase 2: process RSS under the dhat allocator ==");
    let rss = || {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("VmRSS"))
                    .map(|l| l.split_whitespace().nth(1).unwrap_or("?").to_string())
            })
            .unwrap_or_default()
    };
    for i in 0..4 {
        for _ in 0..50_000 {
            std::hint::black_box(timeline.evaluate(0.5, dims));
        }
        println!("frames {:>6}: RSS = {} kB", (i + 1) * 50_000, rss());
    }
    drop(profiler);
}
