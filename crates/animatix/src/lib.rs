// Re-export syntax modules from animatix-syntax for backward compatibility
pub use animatix_syntax::ast;
pub use animatix_syntax::diagnostics;
pub use animatix_syntax::easing;
pub use animatix_syntax::icon_glyphs;
pub use animatix_syntax::module;
pub use animatix_syntax::parser;
pub use animatix_syntax::source_index;
pub use animatix_syntax::to_source;
pub use animatix_syntax::transition_registry;

// Runtime modules (stay in animatix)
pub mod composition;
pub mod ir;
pub mod primitives;
pub mod renderer;
pub mod timeline;
pub mod vm;
