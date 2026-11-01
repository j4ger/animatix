//! Library target for `animatix-cli` unit tests.
//!
//! The executable owns argument parsing and dispatch (`src/main.rs`); this
//! module tree exists so the command implementations, review rule engine, and
//! shared frame helpers can be unit-tested with `cargo test -p animatix-cli
//! --lib` without building the binary test harness.
//!
//! Items used only by `src/main.rs` are intentionally unreachable from this
//! crate root, so dead-code analysis is disabled for this test-library surface.
#![allow(dead_code)] // Test-library surface: binary-owned items are intentionally unreachable here.

pub use animatix::composition::BuildTarget;

pub mod build;
pub mod commands;
pub mod diagnostics;
pub mod extensions;
pub mod frame_source;
pub mod review;
pub mod verify;
