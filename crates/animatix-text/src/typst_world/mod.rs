//! Typst text and math compilation.
//!
//! Split from the original single-module file along item groups; every public
//! path is preserved by the re-exports below.

mod compile;
mod extract;
mod geometry;
mod plain_text;
mod world;

pub use compile::*;
#[cfg(feature = "rich-text")]
pub use extract::*;
pub use geometry::*;
pub use plain_text::*;
#[cfg(feature = "rich-text")]
pub use world::*;

pub(crate) use extract::PathBuilder;
