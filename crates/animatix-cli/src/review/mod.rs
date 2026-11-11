//! `animatix audit`: CPU-first deterministic animation review.
//!
//! The structural tier builds the target, evaluates `SceneProgram`s at sample
//! times with no filter backend, and derives facts that rules judge. The
//! optional render tier adds pixel measurements (whole-frame ink, per-actor
//! ink, beat changes) through the shared `frame_source` helpers, so `audit`
//! and `measure` cannot drift apart.

pub(crate) mod budget;
pub(crate) mod delta;
pub(crate) mod facts;
pub(crate) mod markers;
pub(crate) mod probe;
pub(crate) mod render;
pub(crate) mod report;
pub(crate) mod rules;
pub(crate) mod samples;
pub(crate) mod types;

pub(crate) use report::run_review;
pub(crate) use types::{
    Detail, FailOn, Policy, RenderMode, ReviewOptions, SampleSpec, parse_sample_spec,
};
