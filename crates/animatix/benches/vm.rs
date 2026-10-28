//! Merged benchmark target: expression/modifier VM and property sampling.
//!
//! Each original `benches/<suite>.rs` stays in place as a module, so
//! Criterion group names, benchmark IDs and measurement parameters are
//! unchanged by the target merge. `autobenches = false` in `Cargo.toml` keeps
//! the suite modules from being built as targets of their own.

mod common;
mod modifier_runtime;
mod property_interpolation;
mod vm_regression;
mod vm_vs_ir;

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_vm_target(c: &mut Criterion) {
    vm_regression::bench_reactive_timeline(c);
    vm_vs_ir::bench_modifier_overhead(c);
    modifier_runtime::bench_modifier_evaluation(c);
    property_interpolation::bench_property_interpolation(c);
}

criterion_group!(benches, bench_vm_target);
criterion_main!(benches);
