//! Merged benchmark target: stage and cost breakdowns.
//!
//! Each original `benches/<suite>.rs` stays in place as a module, so
//! Criterion group names, benchmark IDs and measurement parameters are
//! unchanged by the target merge. `autobenches = false` in `Cargo.toml` keeps
//! the suite modules from being built as targets of their own.

mod common;
mod cost_breakdown;
mod equation_frame;
mod scene_costs;
mod stage_breakdown;
mod support;

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_stage_target(c: &mut Criterion) {
    stage_breakdown::bench_stage_breakdown(c);
    cost_breakdown::bench_cost_breakdown(c);
    scene_costs::bench_scene_costs(c);
    equation_frame::bench_equation_frame(c);
}

criterion_group!(benches, bench_stage_target);
criterion_main!(benches);
