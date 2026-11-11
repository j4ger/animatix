pub(crate) mod basic;
pub(crate) mod evaluate;
pub(crate) mod render_dependent;
pub(crate) mod structural;

#[cfg(test)]
#[path = "../rules_tests.rs"]
mod tests;

pub(crate) use evaluate::*;

use crate::review::facts::ReviewFacts;
use crate::review::types::RuleIssue;

pub(crate) fn time_key(issue: &RuleIssue) -> f64 {
    issue.time_s.unwrap_or(-1.0)
}

pub(crate) fn source_line(facts: &ReviewFacts, scene: &str, label: &str) -> Option<usize> {
    facts.actor(scene, label).and_then(|actor| actor.source_line)
}

pub(crate) fn bounds_json(bounds: crate::frame_source::Box2) -> serde_json::Value {
    serde_json::json!([bounds.x0, bounds.y0, bounds.x1, bounds.y1])
}

pub(crate) const BLANK_INK_FRACTION: f32 = 0.001;
pub(crate) const STATIC_BEAT_FRACTION: f32 = 0.0005;
pub(crate) const REST_FLOOR: f32 = 0.35;
pub(crate) const OVERLAP_RATIO_FLOOR: f64 = 0.05;
