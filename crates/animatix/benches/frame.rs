//! Merged benchmark target: per-frame evaluation on the CPU.
//!
//! Each original `benches/<suite>.rs` stays in place as a module, so
//! Criterion group names, benchmark IDs and measurement parameters are
//! unchanged by the target merge. `autobenches = false` in `Cargo.toml` keeps
//! the suite modules from being built as targets of their own.

mod common;
mod scaling;
mod scaling_breakdown;
mod scaling_no_cache;
mod scrubbing;
mod static_scene;
mod static_scene_item_collection;
mod timeline_eval;
mod visibility_culling;

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_frame_target(c: &mut Criterion) {
    timeline_eval::bench_timeline_evaluate(c);
    scrubbing::bench_scrubbing(c);
    static_scene::bench_static(c);
    static_scene_item_collection::bench_static_item_collection(c);
    visibility_culling::bench_visibility(c);
    scaling::bench_scaling(c);
    scaling_no_cache::bench_scaling_no_cache(c);
    scaling_breakdown::bench_scaling_breakdown(c);
}

criterion_group!(benches, bench_frame_target);
criterion_main!(benches);
