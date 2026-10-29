//! Shared utilities for criterion benchmarks.
//!
//! Centralizes the repeated parse→build boilerplate so that API changes
//! (e.g. adding a parameter to `evaluate`) only require a single edit.

use animatix::timeline::Timeline;

/// Parse source text into a [`Timeline`]. Panics on parse or build errors.
pub fn parse_timeline(source: &str) -> Timeline {
    let (stmts, _) = animatix_syntax::parser::parse_source(source);
    Timeline::build(&stmts.expect("parse error in bench fixture"))
}
