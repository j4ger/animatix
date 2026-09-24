//! Web (wasm32-unknown-unknown) driver for the Animatix engine.
//!
//! Layering mirrors the GUI: parse/typecheck/expand via [`animatix_syntax`]'s
//! module system in `SourcesOnly` mode (no disk), build a `Timeline` or
//! `Composition` through the engine's font-context-aware build entry points,
//! and present each frame by rendering the evaluated [`vello::Scene`] straight
//! into a WebGPU canvas surface. Video/native-plugin/export paths are not
//! reachable from the web target.
//!
//! The `host` and `dto` modules compile on every target so the parse→build
//! pipeline is covered by ordinary `cargo test` runs; only the `web` module
//! (wgpu surface, wasm-bindgen exports) is wasm-only.

pub mod dto;
pub mod host;

#[cfg(target_arch = "wasm32")]
pub mod web;
