//! Merged benchmark target: GPU demo frame cost.
//!
//! The original `benches/demos_frame_cost.rs` stays in place as a module, so
//! Criterion group names, benchmark IDs and measurement parameters are
//! unchanged by the target merge. `autobenches = false` in `Cargo.toml` keeps
//! the suite module from being built as a target of its own.

mod demos_frame_cost;

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_gpu_target(c: &mut Criterion) {
    demos_frame_cost::bench_demo_frame_cost(c);
}

criterion_group!(benches, bench_gpu_target);
criterion_main!(benches);
