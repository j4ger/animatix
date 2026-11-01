//! `animatix review`: CPU-first deterministic animation review.
//!
//! The structural tier builds the target, evaluates `SceneProgram`s at sample
//! times with no filter backend, and derives facts that rules judge. The
//! optional render tier adds pixel measurements (whole-frame ink, per-actor
//! ink, beat changes) through the shared `frame_source` helpers, so `review`
//! and `verify` cannot drift apart.

pub(crate) mod facts;
pub(crate) mod render;
pub(crate) mod report;
pub(crate) mod rules;
pub(crate) mod samples;
pub(crate) mod types;
pub(crate) mod verify_rules;

pub(crate) use report::{run_review, write_verify_file};
pub(crate) use types::{
    Detail, FailOn, Policy, RenderMode, ReviewOptions, SampleSpec, parse_sample_spec,
};
