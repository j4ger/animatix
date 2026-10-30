//! Code compile cost: plain fence vs syntax-highlighted spans.
//!
//! The highlight path emits one `#text` element per token and runs syntect at
//! cache-miss time; this bench quantifies that overhead so the GUI
//! rebuild-per-keystroke path (which pays the miss on every edit) stays
//! visible. Every iteration clears the process-wide cache, so each sample is
//! a fresh compile (miss), not a cache hit.

use animatix::renderer::text::{
    FontContext, HighlightPalette, TextCompileRequest, TextCompiler, TextKind, TextStyle,
    clear_text_compile_cache,
};
use criterion::{Criterion, Throughput};

const CODE: &str = "fn evaluate_frame(t: f64) -> Vec<RenderItem> {\n    let items = collect(t);\n    if items.is_empty() { return vec![]; }\n    items.iter().map(render).collect()\n}";

fn highlight_palette() -> HighlightPalette {
    HighlightPalette {
        keyword: [0.85, 0.25, 0.3, 1.0],
        function: [0.3, 0.45, 0.8, 1.0],
        string: [0.15, 0.6, 0.25, 1.0],
        number: [0.75, 0.2, 0.4, 1.0],
        comment: [0.5, 0.5, 0.55, 1.0],
        interpolation: [0.6, 0.3, 0.75, 1.0],
        escape: [0.15, 0.45, 0.5, 1.0],
        annotation: [0.25, 0.1, 0.1, 1.0],
    }
}

fn compile_once(compiler: &mut TextCompiler, font_ctx: &FontContext, language: &str) -> usize {
    let request = TextCompileRequest {
        kind: TextKind::Code,
        content: CODE,
        language,
        palette: compiler.highlight_palette.as_ref(),
        style: TextStyle::new("monospace", 24.0),
        max_width: 0.0,
        max_lines: None,
        text_align: "left",
        overflow: "visible",
        allow_fast_path: compiler.text_fast_path,
    };
    compiler
        .compile_request(&request, font_ctx)
        .map(|paths| paths.len())
        .expect("Code compile should succeed")
}

pub(crate) fn bench_code_compile(c: &mut Criterion) {
    let font_ctx = FontContext::new();
    let mut group = c.benchmark_group("code_compile");
    group.throughput(Throughput::Bytes(CODE.len() as u64));

    let palette = highlight_palette();
    for (name, language) in [
        ("plain_fence", ""),
        ("highlighted", "rust"),
        ("highlighted_palette", "rust"),
    ] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut compiler = TextCompiler::new();
                    if name == "highlighted_palette" {
                        compiler.highlight_palette = Some(palette);
                    }
                    compiler
                },
                |mut compiler| {
                    clear_text_compile_cache();
                    std::hint::black_box(compile_once(&mut compiler, &font_ctx, language))
                },
                criterion::BatchSize::PerIteration,
            )
        });
    }
    group.finish();
}
