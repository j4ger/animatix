//! Benchmark rebuild latency for text-heavy scenes (Text / Code / Typst).
//!
//! The GUI rebuilds the whole `Timeline` on every keystroke, so every Text,
//! Code, and Typst declaration re-runs a full Typst parse + eval + layout
//! unless compiled results are memoized across rebuilds. These benches isolate
//! `Timeline::build` for a large mixed text scene in two regimes:
//!
//! - `*_warm`: repeated builds with unchanged input (steady-state authoring;
//!   a cross-rebuild cache should make this nearly free).
//! - `*_cold`: builds with the compile cache cleared each iteration
//!   (first build / worst case; must not regress when caching is added).

use animatix::timeline::Timeline;
use criterion::{Criterion, criterion_group, criterion_main};

mod common;

/// Generate a scene mixing `n` Text, Code, and Typst declarations with
/// distinct content per actor (realistic: dedup should not collapse them).
fn text_scene_source(n_per_kind: usize) -> String {
    let mut decls = String::new();
    for i in 0..n_per_kind {
        decls.push_str(&format!(
            "t{i}: Text, text: \"Chapter {i} — Layout & Timing\", font_size: {size}, at: (200, {y})\n",
            size = 24 + (i % 5) * 8,
            y = 100 + i * 30,
        ));
        decls.push_str(&format!(
            "c{i}: Code, code: \"fn step_{i}() {{ state += {i}; }}\", font_size: 20, at: (700, {y})\n",
            y = 100 + i * 30,
        ));
        decls.push_str(&format!(
            "m{i}: Typst, content: \"$ sum_(k=1)^{n} k = frac({n}({n}+1), 2) $\", font_size: 28, at: (1200, {y})\n",
            n = i + 2,
            y = 100 + i * 30,
        ));
    }
    format!("config {{ colorscheme: \"editorial-dark\" }}\n\n#0s\n{decls}\n")
}

fn bench_text_rebuild(c: &mut Criterion) {
    let source = text_scene_source(16); // 48 text actors total
    let (stmts, _) = animatix_syntax::parser::parse_source(&source);
    let stmts = stmts.expect("parse error in bench fixture");

    let mut group = c.benchmark_group("text_rebuild");

    // Steady-state GUI rebuild: same source, cache warm from prior iterations.
    group.bench_function("mixed_48_warm", |b| {
        b.iter(|| {
            let _timeline = Timeline::build(&stmts);
        })
    });

    // First-build cost: drop memoized compilations before every iteration.
    group.bench_function("mixed_48_cold", |b| {
        b.iter(|| {
            animatix::renderer::text::clear_text_compile_cache();
            let _timeline = Timeline::build(&stmts);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_text_rebuild);
criterion_main!(benches);
