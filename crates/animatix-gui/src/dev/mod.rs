//! Development-only visual tooling.
//!
//! This module is only compiled with the `dev-screenshots` feature. It powers
//! the bounded `widget-screenshot` binary so GUI changes can be reviewed as
//! artifacts instead of requiring a long-lived interactive session.

pub mod screenshot_harness;
