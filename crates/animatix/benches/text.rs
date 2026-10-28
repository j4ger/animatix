//! Merged benchmark target: text compile and rebuild costs.
//!
//! Each original `benches/<suite>.rs` stays in place as a module, so
//! Criterion group names, benchmark IDs and measurement parameters are
//! unchanged by the target merge. `autobenches = false` in `Cargo.toml` keeps
//! the suite modules from being built as targets of their own.

mod code_highlight;
mod text_rebuild;

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_text_target(c: &mut Criterion) {
    text_rebuild::bench_text_rebuild(c);
    code_highlight::bench_code_compile(c);
}

criterion_group!(benches, bench_text_target);
criterion_main!(benches);
