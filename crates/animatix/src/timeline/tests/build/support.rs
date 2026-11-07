//! Re-exports the shared timeline fixtures for the build submodule's tests.
//!
//! The helpers themselves live in [`super::super::support`]; this module only
//! narrows the visibility so `build/*.rs` can keep importing `super::support`.

pub(super) use super::super::support::{
    actor, assert_clean, assert_finite_samples, build, build_ast, build_clean, build_timeline,
    f32_property, keyframe, parse, prop, property_or_default, render_paths, text_at, vec2,
    vec2_property,
};
