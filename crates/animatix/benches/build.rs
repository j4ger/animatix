//! Merged benchmark target: build-path costs.
//!
//! Each original `benches/<suite>.rs` stays in place as a module, so
//! Criterion group names, benchmark IDs and measurement parameters are
//! unchanged by the target merge. `autobenches = false` in `Cargo.toml` keeps
//! the suite modules from being built as targets of their own.

mod build_time;
mod common;
mod full_pipeline;

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_build_target(c: &mut Criterion) {
    build_time::bench_build_time(c);
    full_pipeline::bench_full_pipeline(c);
}

criterion_group!(benches, bench_build_target);
criterion_main!(benches);
