//! Stage-tracer consumer bench (PF-8 shared stage names, end-to-end).
//!
//! Unlike the hand-rolled stage isolation in `cost_breakdown.rs`, this suite
//! measures the *shared* stage tracer (`animatix::perf`, `ScopedStage`) so the
//! bench numbers use exactly the same stage names the GUI `--perf-log` sink
//! drains and `docs/performance_evaluation.md` §3.5 defines.
//!
//! Each stage bench drives a generated scene (modifiers + container layout,
//! so `build_frame_env`/`modifier_exec`/`layout` all fire) at *distinct times
//! per iteration, guaranteeing a frame-cache miss every frame — the realistic
//! worst case for authoring/scrubbing. `iter_custom` returns the stage's
//! accumulated duration over `iters` evaluations, so Criterion reports the
//! per-evaluation stage cost.
//!
//! The `total` bench is the ordinary wall-time miss budget to compare the
//! stage sum against (stages nest, e.g. `sample` wraps `layout`, so the sum
//! overcounts nested stages — read the table as relative weights).

use std::time::Duration;

use animatix::perf::{self, stage};
use animatix::timeline::{SceneDimensions, Timeline};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

mod common;

const DIMS: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};
const N_ACTORS: usize = 60;

/// A generated scene where every frame must recompute: distinct times force a
/// full miss; `always` drives modifier exec + frame env; `dynamic_layout` plus
/// a per-frame size change force taffy layout resolution every frame.
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

fn scenario_timeline() -> Timeline {
    common::parse_timeline(&scenario_source())
}

/// Stages that fire inside a single miss-frame evaluation.
const EVAL_STAGES: [&str; 4] = [
    stage::BUILD_FRAME_ENV,
    stage::MODIFIER_EXEC,
    stage::SAMPLE,
    stage::LAYOUT,
];

fn bench_stage_breakdown(c: &mut Criterion) {
    let timeline = scenario_timeline();
    let src = scenario_source();

    // Warm up with several distinct-time evaluations. Only stages observed in
    // *every* warm frame are measured: a stage that fires once and then hits a
    // cache (e.g. static layout) would otherwise report a mostly-zero Duration
    // and trip Criterion's zero-time guard.
    let mut observed: Option<std::collections::HashSet<String>> = None;
    for k in 0..4 {
        let _ = perf::take_measurements();
        black_box(timeline.evaluate(k as f64 * 0.05, DIMS));
        let frame_stages: std::collections::HashSet<String> =
            perf::take_measurements().into_iter().map(|(n, _)| n).collect();
        observed = Some(match observed {
            None => frame_stages,
            Some(prev) => prev.intersection(&frame_stages).cloned().collect(),
        });
    }
    let observed = observed.unwrap_or_default();

    let mut group = c.benchmark_group("stage");

    // Wall-time miss budget at distinct times (a miss every iteration).
    group.bench_function("eval_total", |b| {
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for i in 0..iters {
                black_box(timeline.evaluate((i % 100) as f64 * 0.01, DIMS));
            }
            start.elapsed()
        })
    });

    for stage_name in EVAL_STAGES {
        if !observed.contains(stage_name) {
            eprintln!("stage_breakdown: skipping stage '{stage_name}' (not fired in every frame)");
            continue;
        }
        group.bench_function(format!("{stage_name}"), |b| {
            b.iter_custom(|iters| {
                // Drain any residual accumulation, then run `iters` miss
                // frames and return only this stage's accumulated time.
                let _ = perf::take_measurements();
                for i in 0..iters {
                    black_box(timeline.evaluate((i % 100) as f64 * 0.01, DIMS));
                }
                let measurements = perf::take_measurements();
                measurements
                    .iter()
                    .find(|(name, _)| name == stage_name)
                    .map(|(_, d)| *d)
                    .unwrap_or(Duration::ZERO)
            })
        });
    }
    group.finish();

    // The `rebuild` stage fires inside Timeline::build (on the build layer, not
    // per-frame). Rebuild the same AST repeatedly to get the per-build cost.
    {
        let (stmts, _) = animatix_syntax::parser::parse_source(&src);
        let stmts = stmts.expect("parse error in bench fixture");
        let mut group = c.benchmark_group("stage");
        group.bench_function(stage::REBUILD, |b| {
            b.iter_custom(|iters| {
                let _ = perf::take_measurements();
                for _ in 0..iters {
                    black_box(Timeline::build(&stmts));
                }
                perf::take_measurements()
                    .iter()
                    .find(|(name, _)| name == stage::REBUILD)
                    .map(|(_, d)| *d)
                    .unwrap_or(Duration::ZERO)
            })
        });
        group.finish();
    }
}

criterion_group!(benches, bench_stage_breakdown);
criterion_main!(benches);
