//! Shared utilities for criterion benchmarks.
//!
//! Centralizes the repeated parse→build boilerplate so that API changes
//! (e.g. adding a parameter to `evaluate`) only require a single edit.

use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use chumsky::Parser;

/// Default 1080p scene dimensions used by most benches.
// Reserved for use by individual benchmarks
#[allow(dead_code)]
pub const DEFAULT_DIMS: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};

/// Parse source text into a [`Timeline`]. Panics on parse or build errors.
// Reserved for use by individual benchmarks
#[allow(dead_code)]
pub fn parse_timeline(source: &str) -> Timeline {
    let (stmts, _) = animatix_syntax::parser::parser_simple()
        .parse(source)
        .into_output_errors();
    Timeline::build(&stmts.expect("parse error in bench fixture"))
}

/// Shorthand for `timeline.evaluate(time_s, DEFAULT_DIMS)`.
// Reserved for use by individual benchmarks
#[allow(dead_code)]
pub fn eval(timeline: &Timeline, time_s: f64) -> vello::Scene {
    timeline.evaluate(time_s, DEFAULT_DIMS)
}

/// Shorthand for `timeline.evaluate_with_debug(...)` with default debug options
/// and no filter backend.
// Reserved for use by individual benchmarks
#[allow(dead_code)]
pub fn eval_no_cache(timeline: &Timeline, time_s: f64) -> vello::Scene {
    let mut fb = None;
    timeline.evaluate_with_debug(time_s, DEFAULT_DIMS, DebugRenderOptions::default(), &mut fb)
}
