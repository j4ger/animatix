//! Language vocabulary and effect contract types shared by every layer.
//!
//! This crate is the bottom of the dependency graph: the built-in catalog
//! ([`animatix-std`]), the parser front-end (`animatix-syntax`), and the
//! engine (`animatix`) all depend on it, so it must stay free of engine,
//! renderer, and parser dependencies. Admission rule: something goes here
//! when `animatix-std` needs it, or when it is part of the effect definition
//! surface.
//!
//! Contents:
//! - [`caps`]: the capability vocabulary (`ActorCategory`, `ChildProcessing`,
//!   `ShapeKind`, `TextKind`, `PrimitiveCapabilities`).
//! - [`effect`]: the [`Effect`](effect::Effect) trait, parameter schema, and
//!   identity types.
//! - [`icon_glyphs`]: UI glyph constants (defined here rather than via
//!   `egui_phosphor` so nothing below the GUI depends on egui).

pub mod caps;
pub mod effect;
pub mod icon_glyphs;
