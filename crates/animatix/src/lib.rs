#![warn(missing_docs)]

//! Core animation engine: timeline, renderer, and evaluation.

// Re-export syntax modules internally so animatix code can use `crate::ast` etc.
pub use animatix_syntax::{ast, easing};
pub(crate) use animatix_syntax::{diagnostics, module};

// Runtime modules (stay in animatix)
pub mod composition;
pub mod extension_context;
pub use extension_context::ExtensionRegistry;
pub mod extension_plugin;
pub mod ir;
pub mod perf;
pub mod primitives;
pub mod property_descriptor;
pub mod renderer;
pub mod timeline;
pub mod verify;
