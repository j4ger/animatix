//! Language vocabulary and effect contract types shared by every layer.
//!
//! This crate is the bottom of the dependency graph: the built-in catalog,
//! parser front-end (`animatix-syntax`), and engine (`animatix`) all depend on
//! it, so it stays free of engine, renderer, and parser dependencies. Shared
//! language contracts live at the root; built-in catalog, callable, and
//! effect implementations live under [`builtins`].
//!
//! Contents:
//! - [`caps`]: the capability vocabulary (`ActorCategory`, `ChildProcessing`, `ShapeKind`,
//!   `TextKind`, `PrimitiveCapabilities`).
//! - [`features`]: optional engine feature bits declared by primitives.
//! - [`effect`]: the [`Effect`](effect::Effect) trait, parameter schema, and identity types.
//! - [`icon_glyphs`]: UI glyph constants (defined here rather than via `egui_phosphor` so nothing
//!   below the GUI depends on egui).
//! - [`stroke_icons`]: the bundled stroke-icon table that the engine's `icon:` property expands
//!   into `Path` geometry.

pub mod builtins;
pub mod callable;
pub mod caps;
pub mod effect;
pub mod error;
pub mod features;
pub mod icon_glyphs;
pub mod property;
pub mod stroke_icons;
