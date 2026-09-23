//! Editor-intelligence cost: the per-keystroke analyzer path.
//!
//! The GUI re-runs [`Analyzer::update`] on every text change (see
//! `animatix-gui/src/editor.rs`, `replace_text` and the cell-edit path) and
//! calls [`Analyzer::diagnostics`] per frame, so both sit on the authoring
//! latency path. Neither the engine benchmark suite nor `perf-bench.sh`
//! covered them: every bench in the workspace lives in `animatix`, and the
//! engine deliberately does not depend on this crate (the layering is
//! one-way), so analyzer cost was invisible to the regression guard.
//!
//! Inputs are real files, matching the `full_pipeline` bench's practice, so
//! the numbers reflect content the editor actually opens rather than a
//! synthetic shape that may flatter the parser.

use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};

use animatix_analyzer::{Analyzer, LintConfig};

/// One benchmark input: a label and the file's source.
struct Sample {
    label: &'static str,
    source: String,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(relative: &str) -> Option<String> {
    std::fs::read_to_string(repo_root().join(relative)).ok()
}

/// Real-content samples, smallest to largest.
///
/// Skipped individually when absent so the bench still runs from a packaged
/// crate without the repository's example tree.
fn samples() -> Vec<Sample> {
    [
        ("small", "examples/basics/00_hello.amx"),
        ("dogfood", "dogfood/projects/taylor-sin/entry.amx"),
        ("large", "examples/gallery/dashboard_story.amx"),
    ]
    .into_iter()
    .filter_map(|(label, rel)| load(rel).map(|source| Sample { label, source }))
    .collect()
}

/// A source that differs from `source`, simulating a keystroke.
///
/// `Analyzer::update` returns early when the text is unchanged, so a bench
/// that reused one string would measure the early-out instead of a rebuild.
fn edited(source: &str, index: usize) -> String {
    format!("{source}\n// keystroke {index}\n")
}

fn bench_update(c: &mut Criterion) {
    let samples = samples();
    let mut group = c.benchmark_group("analyzer_update");

    for sample in &samples {
        let mut analyzer = Analyzer::new(&sample.source);
        let mut counter = 0usize;
        group.bench_function(sample.label, |b| {
            b.iter(|| {
                counter += 1;
                let text = edited(&sample.source, counter);
                analyzer.update(&text);
                std::hint::black_box(analyzer.parse_errors().len())
            })
        });
    }
    group.finish();
}

fn bench_diagnostics(c: &mut Criterion) {
    let samples = samples();
    let mut group = c.benchmark_group("analyzer_diagnostics");

    for sample in &samples {
        let analyzer = Analyzer::new(&sample.source);
        group.bench_function(sample.label, |b| {
            b.iter(|| std::hint::black_box(analyzer.diagnostics()))
        });
    }
    group.finish();
}

/// Diagnostics through the lint-config entry point, which is what the CLI and
/// the GUI's inline-comment lint suppression both use.
fn bench_diagnostics_with_lint_config(c: &mut Criterion) {
    let samples = samples();
    let config = LintConfig::default();
    let mut group = c.benchmark_group("analyzer_diagnostics_configured");

    for sample in &samples {
        let analyzer = Analyzer::new(&sample.source);
        group.bench_function(sample.label, |b| {
            b.iter(|| std::hint::black_box(analyzer.diagnostics_with_config(&config)))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_update, bench_diagnostics, bench_diagnostics_with_lint_config);
criterion_main!(benches);
