//! A compact DSL → timeline → GPU plate, not a replacement for isolation tests.
use animatix::timeline::{SceneDimensions, Timeline};
use animatix_render::offscreen::OffscreenRenderer;

#[path = "support/plate_oracle.rs"]
mod oracle;
use oracle::{CELLS, HEIGHT, WIDTH, failures, mutate};

#[test]
fn geometry_plate_and_local_faults() {
    let source = include_str!("fixtures/geometry_plate.amx");
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "plate parse: {errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.expect("plate AST"), &Default::default());
    assert!(report.diagnostics.is_empty(), "plate build: {:?}", report.diagnostics);
    let mut renderer = match OffscreenRenderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            // Same presence-based policy as skip_if_no_gpu, retaining the cause.
            assert!(
                std::env::var_os("ANIMATIX_REQUIRE_GPU").is_none(),
                "required geometry plate GPU initialization failed: {error}"
            );
            tracing::warn!(%error, "skipping geometry plate: GPU unavailable");
            return;
        },
    };
    let frame = renderer
        .render_timeline(
            &report.output,
            0.0,
            SceneDimensions {
                width: WIDTH as u32,
                height: HEIGHT as u32,
            },
        )
        .expect("geometry plate render");
    let errors = failures(&frame.rgba);
    assert!(errors.is_empty(), "geometry plate:\n{}", errors.join("\n"));

    // Corrupt the real readback, not a separately rendered reference. Every fault
    // must fail exactly its own cell: seven healthy cells cannot hide the eighth.
    for (index, cell) in CELLS.iter().enumerate() {
        let mut faults = vec!["disappear", "wrong-color", "offset"];
        if index == 3 || index == 4 {
            // Same nominal bounds and area: only shape-sensitive probes distinguish
            // the diamond and triangle. Recolor to the destination's expected color
            // so swapping cannot be rejected merely because the palette differs.
            faults.push("shape-swap");
        }
        for fault in faults {
            let damaged = mutate(&frame.rgba, index, fault);
            let errors = failures(&damaged);
            assert!(!errors.is_empty(), "{}: missed {fault}", cell.name);
            assert!(
                errors.iter().all(|error| error.starts_with(cell.name)),
                "{} / {fault}: unrelated cell failures: {errors:?}",
                cell.name
            );
        }
    }
}
