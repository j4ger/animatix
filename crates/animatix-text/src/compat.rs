//! Temporary compatibility entry point for the engine's pre-Phase-3 call sites.
//!
//! The engine still calls the historical 18-parameter `compile_text_cached`
//! through the crate root. [`crate::compiler::compile_text_cached`] now takes a
//! [`TextCompileRequest`]; this module adapts the old flat shape onto it so the
//! engine keeps building untouched while the text crate moves first.
//!
//! Phase 3 migrates the engine call sites to [`TextCompileRequest`], after which
//! this module and the explicit root re-export in `lib.rs` are deleted. The root
//! name then resolves to the request-based function through the glob re-export
//! in `lib.rs` with no further rename.

use std::sync::Arc;

use animatix_core::error::RenderError;

use crate::compiler::{CachedText, TextKind};
use crate::font_context::FontContext;
use crate::request::{TextCompileRequest, TextStyle};
use crate::typst_world::HighlightPalette;

/// Compile text into glyph paths plus metrics, memoized process-wide.
///
/// Deprecated flat-argument shape of [`crate::compiler::compile_text_cached`],
/// kept only until the engine migrates to [`TextCompileRequest`]. It builds the
/// request and delegates, so old and new call sites share cache entries.
#[deprecated(
    note = "build a TextCompileRequest and call compiler::compile_text_cached; \
            this flat entry point is removed after the Phase 3 engine migration"
)]
#[allow(clippy::too_many_arguments)] // Mirrors the historical public signature.
pub fn compile_text_cached(
    kind: TextKind,
    content: &str,
    language: &str,
    palette: Option<&HighlightPalette>,
    font_family: &str,
    font_size: f32,
    font_weight: f32,
    font_style: &str,
    line_height: f32,
    letter_spacing: f32,
    word_spacing: f32,
    color: [f32; 4],
    max_width: f32,
    max_lines: Option<usize>,
    text_align: &str,
    overflow: &str,
    allow_fast_path: bool,
    font_ctx: &FontContext,
) -> Result<Arc<CachedText>, RenderError> {
    let request = TextCompileRequest {
        kind,
        content,
        language,
        palette,
        style: TextStyle {
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            letter_spacing,
            word_spacing,
            color,
        },
        max_width,
        max_lines,
        text_align,
        overflow,
        allow_fast_path,
    };
    crate::compiler::compile_text_cached(&request, font_ctx)
}
