//! Benchmark the full pipeline: parse → module load → typecheck → expand → build.
//!
//! Contract revision 2 distinguishes source-override rebuilds from file reads.
//! None of these includes GUI scheduling, rendering, or a persistent module cache.

use std::hint::black_box;
use std::path::Path;

use criterion::Criterion;

#[path = "full_pipeline_support.rs"]
mod support;
use support::{WORKLOADS, build_only, file_pipeline, parsed, required_source, source_pipeline};

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

pub(crate) fn bench_full_pipeline(c: &mut Criterion) {
    let simple = simple_scene_source();
    let reactive = reactive_scene_source();
    // Prepare ASTs once, outside Criterion's timed iterations.
    let simple_ast = parsed(&simple);
    let reactive_ast = parsed(&reactive);

    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let paths = WORKLOADS.map(|relative| examples_dir.join(relative));
    // Preflight every mandatory input, even the file-I/O case. No conditional rows.
    let sources = paths.each_ref().map(|path| required_source(path));
    let virtual_path = examples_dir.join("_bench_virtual.amx");
    let mut group = c.benchmark_group("full_pipeline");

    group.bench_function("simple_parse_only", |b| {
        b.iter(|| black_box(animatix_syntax::parser::parse_source(black_box(&simple))))
    });
    group.bench_function("simple_build_only_v2", |b| {
        b.iter(|| black_box(build_only(black_box(&simple_ast))))
    });
    group.bench_function("simple_source_full_v2", |b| {
        b.iter(|| black_box(source_pipeline(&virtual_path, black_box(&simple))))
    });
    group.bench_function("reactive_parse_only", |b| {
        b.iter(|| black_box(animatix_syntax::parser::parse_source(black_box(&reactive))))
    });
    group.bench_function("reactive_build_only_v2", |b| {
        b.iter(|| black_box(build_only(black_box(&reactive_ast))))
    });
    group.bench_function("reactive_source_full_v2", |b| {
        b.iter(|| black_box(source_pipeline(&virtual_path, black_box(&reactive))))
    });
    group.bench_function("showcase_source_full_v2", |b| {
        b.iter(|| black_box(source_pipeline(&paths[0], black_box(&sources[0]))))
    });
    group.bench_function("components_source_full_v2", |b| {
        b.iter(|| black_box(source_pipeline(&paths[1], black_box(&sources[1]))))
    });
    // Existing ID and boundary retained: read the real modules entry on every iteration.
    group.bench_function("modules_full", |b| b.iter(|| black_box(file_pipeline(&paths[2]))));
    group.finish();
}
