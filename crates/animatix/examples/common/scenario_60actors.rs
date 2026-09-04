//! Shared 60-actor dynamic scenario for the steady-state profiling examples.
//!
//! Included by `perf_driver.rs` (time attribution) and `alloc_driver.rs`
//! (allocation attribution) so both rank the exact same workload. The scenario
//! mirrors `benches/stage_breakdown.rs`: a dynamic `Row` container where
//! `box0.size`/`box0.at` are overridden per frame by an `always` block, so
//! every frame is a cache miss and the full evaluate path (frame env, property
//! sampling, dynamic layout, scene encode) runs.

pub const DIMS: animatix::timeline::SceneDimensions = animatix::timeline::SceneDimensions {
    width: 1920,
    height: 1080,
};
pub const N_ACTORS: usize = 60;

pub fn scenario_source() -> String {
    let mut src = String::from(
        r##"config { colorscheme: "editorial-dark", dynamic_layout: true }

#0s
row: Row, at: (120, 300), gap: 8, align: "start" {
  box0: Rect, size: (50, 50), color: accent.primary
"##,
    );
    for i in 1..N_ACTORS {
        src.push_str(&format!("  box{i}: Rect, size: (50, 50), color: accent.primary\n"));
    }
    src.push_str(
        r##"}
always {
  box0.size = (40 + 20 * sin(t), 50)
  box0.at = (80 + 30 * sin(t * 3), 300)
}
"##,
    );
    src
}

pub fn build_timeline() -> animatix::timeline::Timeline {
    let (stmts, _) = animatix_syntax::parser::parse_source(&scenario_source());
    animatix::timeline::Timeline::build(&stmts.expect("parse error"))
}

/// The time pattern the examples loop over (identical in both drivers, so
/// caches see the same 100 distinct times).
pub fn frame_time(i: u64) -> f64 {
    (i % 100) as f64 * 0.01
}
