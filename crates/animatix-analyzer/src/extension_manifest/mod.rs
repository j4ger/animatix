//! Extension manifests for analyzer-only language intelligence.
//!
//! These manifests describe external primitives and properties without loading
//! native runtime plugins. Parsed manifests use the shared schema descriptor
//! shapes, but their property `id` is intentionally `None`: runtime ids are
//! allocated by `ExtensionRegistry`, never guessed by a manifest.

mod from_runtime;
mod manifest;
mod schema;
#[cfg(test)]
mod tests;

pub use manifest::{ExtensionManifest, ManifestEffect, ManifestEffectParam};
pub use schema::{ManifestPrimitive, ManifestProperty};
