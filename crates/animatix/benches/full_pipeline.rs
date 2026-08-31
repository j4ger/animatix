//! Benchmark the full pipeline: parse → module load → typecheck → expand → build.
//!
//! This measures the end-to-end cost that the GUI experiences on every rebuild,
//! including all the steps before Timeline::build is called.

use std::path::Path;

use animatix::timeline::Timeline;
use animatix_syntax::module::ModuleGraph;
use criterion::{Criterion, criterion_group, criterion_main};

mod common;

/// Simple scene with no imports — isolates parse + build cost.
fn simple_scene_source() -> String {
    r#"
config { colorscheme: "editorial-dark" }

#0s
box: Rect, size: (100, 100), color: accent.primary, at: (400, 300)
circle: Ellipse, size: (50, 50), color: text.primary, at: (600, 300)

#1s
box.size = (200, 200)
box.color = accent.warning

#+1s
circle.at = (800, 300)
circle.size = (80, 80)
"#
    .to_string()
}

/// Scene with `always` blocks (reactive expressions).
fn reactive_scene_source() -> String {
    r#"
config { colorscheme: "editorial-dark" }

#0s
center: Ellipse, size: (16, 16), color: text.primary, at: (640, 390)
orbiter: Ellipse, size: (64, 64), color: accent.primary, at: (820, 390)
pulse: Rect, size: (120, 120), color: (0.88, 0.42, 0.84, 1.0), at: (280, 390)
echo: Ellipse, size: (40, 40), color: accent.warning, at: pulse.at

always {
  orbiter.at = (640 + 180 * cos(t), 390 + 120 * sin(t * 2))
  pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
  echo.size = (pulse.size.x / 3, pulse.size.x / 3)
  echo.at = orbiter.at
}
"#
    .to_string()
}

/// Full pipeline: parse + typecheck + expand + build.
///
/// Synthetic sources have no imports, so a temp file directly in `examples/`
/// is fine here. For sources with relative imports use [`full_pipeline_like`].
fn full_pipeline(source: &str) {
    let mut graph = ModuleGraph::new();
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let path = examples_dir.join("_bench_temp.amx");
    std::fs::write(&path, source).unwrap();

    let mut program =
        graph.load_program_with_source(&path, Some(source)).expect("module load failed");
    let _type_diags = program.typecheck();
    let expanded = program.expand_components(&mut Vec::new());
    let _timeline = Timeline::build(&expanded);

    // Clean up
    let _ = std::fs::remove_file(&path);
}

/// Full pipeline variant anchored to a real example file: writes the source to
/// a temp file *next to `anchor`* so that relative imports
/// (`../lib/theme.amx`) resolve exactly as they do for the real file, then
/// removes it.
fn full_pipeline_like(anchor: &Path, source: &str) {
    let mut graph = ModuleGraph::new();
    let path = anchor.with_file_name("_bench_temp.amx");
    std::fs::write(&path, source).unwrap();

    let mut program =
        graph.load_program_with_source(&path, Some(source)).expect("module load failed");
    let _type_diags = program.typecheck();
    let expanded = program.expand_components(&mut Vec::new());
    let _timeline = Timeline::build(&expanded);

    // Clean up
    let _ = std::fs::remove_file(&path);
}

/// Full pipeline using an existing file path (for examples with imports).
fn full_pipeline_file(path: &Path) {
    let source = std::fs::read_to_string(path).unwrap();
    let mut graph = ModuleGraph::new();
    let mut program =
        graph.load_program_with_source(path, Some(&source)).expect("module load failed");
    let _type_diags = program.typecheck();
    let expanded = program.expand_components(&mut Vec::new());
    let _timeline = Timeline::build(&expanded);
}

/// Parse-only cost (no module load, no typecheck).
fn parse_only(source: &str) {
    let (_stmts, _errors) = animatix_syntax::parser::parse_source(source);
}

/// Build-only cost (already parsed statements).
fn build_only(source: &str) {
    let (stmts, _) = animatix_syntax::parser::parse_source(source);
    let stmts = stmts.expect("parse error");
    let _timeline = Timeline::build(&stmts);
}

fn bench_full_pipeline(c: &mut Criterion) {
    let simple = simple_scene_source();
    let reactive = reactive_scene_source();

    // Load real example files
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let showcase =
        std::fs::read_to_string(examples_dir.join("gallery/fft_explain.amx")).unwrap_or_default();
    let components = std::fs::read_to_string(examples_dir.join("components/09_components.amx"))
        .unwrap_or_default();
    let _modules =
        std::fs::read_to_string(examples_dir.join("components/10_modules.amx")).unwrap_or_default();

    let mut group = c.benchmark_group("full_pipeline");

    group.bench_function("simple_parse_only", |b| b.iter(|| parse_only(&simple)));

    group.bench_function("simple_build_only", |b| b.iter(|| build_only(&simple)));

    group.bench_function("simple_full", |b| b.iter(|| full_pipeline(&simple)));

    group.bench_function("reactive_parse_only", |b| b.iter(|| parse_only(&reactive)));

    group.bench_function("reactive_build_only", |b| b.iter(|| build_only(&reactive)));

    group.bench_function("reactive_full", |b| b.iter(|| full_pipeline(&reactive)));

    if !showcase.is_empty() {
        let showcase_path = examples_dir.join("gallery/fft_explain.amx");
        group.bench_function("showcase_full", |b| {
            b.iter(|| full_pipeline_like(&showcase_path, &showcase))
        });
    }

    if !components.is_empty() {
        let components_path = examples_dir.join("components/09_components.amx");
        group.bench_function("components_full", |b| {
            b.iter(|| full_pipeline_like(&components_path, &components))
        });
    }

    let modules_path = examples_dir.join("components/10_modules.amx");
    if modules_path.exists() {
        group.bench_function("modules_full", |b| b.iter(|| full_pipeline_file(&modules_path)));
    }

    group.finish();
}

criterion_group!(benches, bench_full_pipeline);
criterion_main!(benches);
