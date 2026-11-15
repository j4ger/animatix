//! Shared CLI modules, also available to focused `cargo test --lib` runs.
//! The default suite uses the binary harness, which includes these tests plus
//! argument-parsing tests; running both harnesses would repeat the shared cases.
#![allow(dead_code)] // Test-library surface: binary-owned items are intentionally unreachable here.

pub use animatix::composition::BuildTarget;

pub mod build;
pub mod commands;
pub mod diagnostics;
pub mod extensions;
pub mod frame_source;
pub mod review;
