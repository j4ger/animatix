//! Plain-data text compilation requests.
//!
//! A [`TextCompileRequest`] is the complete input to one compilation: content,
//! kind, typography, wrapping policy, and the fast-path switch. It borrows every
//! string, so callers build one per compile (including frames) without copying
//! content.
//!
//! The memo-table identity is derived from the request instead of restated at
//! each call site: [`TextCompileRequest::cache_key`] walks the request's fields
//! plus the font-set identity, so adding a field cannot silently leave the
//! cache key behind.

use crate::compiler::TextKind;
use crate::font_context::{DEFAULT_FONT_WEIGHT, FontContext, FontSetId};
use crate::typst_world::HighlightPalette;

/// Typography and solid fill for one text compilation.
///
/// Borrowed so a request can be built on the per-frame path without allocating.
#[derive(Clone, Debug)]
pub struct TextStyle<'a> {
    /// Requested font family; resolved against the font context at compile time.
    pub font_family: &'a str,
    /// Font size in scene units (points).
    pub font_size: f32,
    /// Requested weight; the nearest available face wins.
    pub font_weight: f32,
    /// `"normal"`, `"italic"`, or `"oblique"`.
    pub font_style: &'a str,
    /// Line-height multiplier (1.2 when the caller has no opinion).
    pub line_height: f32,
    /// Extra advance between glyphs, in points.
    pub letter_spacing: f32,
    /// Extra advance between words, in points.
    pub word_spacing: f32,
    /// Solid RGBA fill in 0.0–1.0 components.
    pub color: [f32; 4],
}

impl<'a> TextStyle<'a> {
    /// Base style for `font_family` at `font_size`: regular weight, normal
    /// style, 1.2 line height, no extra spacing, white fill.
    pub fn new(font_family: &'a str, font_size: f32) -> Self {
        Self {
            font_family,
            font_size,
            font_weight: DEFAULT_FONT_WEIGHT,
            font_style: "normal",
            line_height: 1.2,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// One text compilation: content, kind, typography, and wrapping policy.
#[derive(Clone, Debug)]
pub struct TextCompileRequest<'a> {
    /// Which text engine understands `content`.
    pub kind: TextKind,
    /// The text to compile.
    pub content: &'a str,
    /// Syntax-highlighting language for `kind == TextKind::Code`; ignored (and
    /// normally empty) for every other kind.
    pub language: &'a str,
    /// Role palette for `Code` highlighting; `None` keeps Typst's fixed theme.
    pub palette: Option<&'a HighlightPalette>,
    /// Typography and fill.
    pub style: TextStyle<'a>,
    /// Wrap width in points; `0.0` disables wrapping.
    pub max_width: f32,
    /// Optional cap on the number of wrapped lines.
    pub max_lines: Option<usize>,
    /// `"left"`, `"center"`, `"right"`, or `"justify"`.
    pub text_align: &'a str,
    /// `"visible"`, `"clip"`, or `"ellipsis"`.
    pub overflow: &'a str,
    /// Whether plain Latin `Text` may take the direct-shaping fast path.
    ///
    /// Fast-path and Typst output are visually equivalent but not bit-identical,
    /// so they are cached under distinct keys.
    pub allow_fast_path: bool,
}

impl<'a> TextCompileRequest<'a> {
    /// Minimal request: `content` on `kind`'s Typst-equivalent route (no fast
    /// path), unwrapped, left-aligned, overflow visible, no language or palette.
    pub fn new(kind: TextKind, content: &'a str, style: TextStyle<'a>) -> Self {
        Self {
            kind,
            content,
            language: "",
            palette: None,
            style,
            max_width: 0.0,
            max_lines: None,
            text_align: "left",
            overflow: "visible",
            allow_fast_path: false,
        }
    }

    /// Solid fill as RGBA bytes for rendering.
    ///
    /// Plain `Text` compiles white and is tinted at draw time, so its requested
    /// `style.color` is deliberately not part of the compiled output.
    pub(crate) fn solid_rgba(&self) -> [u8; 4] {
        if self.kind == TextKind::Text {
            [255, 255, 255, 255]
        } else {
            [
                (self.style.color[0] * 255.0) as u8,
                (self.style.color[1] * 255.0) as u8,
                (self.style.color[2] * 255.0) as u8,
                (self.style.color[3] * 255.0) as u8,
            ]
        }
    }

    /// Derive the memo-table key for this request.
    ///
    /// `font_ctx` contributes the content identity of its font set. Every
    /// other output-affecting input is read from the request itself.
    pub fn cache_key(&self, font_ctx: &FontContext) -> TextCacheKey {
        TextCacheKey {
            content: self.content.to_string(),
            language: self.language.to_string(),
            palette_bits: self.palette.map(HighlightPalette::cache_bits),
            font_family: self.style.font_family.to_string(),
            font_size_bits: self.style.font_size.to_bits(),
            font_weight_bits: self.style.font_weight.to_bits(),
            font_style: self.style.font_style.to_string(),
            line_height_bits: self.style.line_height.to_bits(),
            letter_spacing_bits: self.style.letter_spacing.to_bits(),
            word_spacing_bits: self.style.word_spacing.to_bits(),
            color: self.solid_rgba(),
            kind: self.kind,
            max_width_bits: self.max_width.to_bits(),
            max_lines: self.max_lines,
            text_align: self.text_align.to_string(),
            overflow: self.overflow.to_string(),
            fast_path: self.allow_fast_path || self.kind == TextKind::Text,
            font_set_id: font_ctx.font_set_id(),
        }
    }
}

/// Identity of one memoized text compilation, derived from a
/// [`TextCompileRequest`] plus the font-set content identity.
///
/// `f32` inputs are bit-cast to `u32` because floats are not `Hash`. The fields
/// stay private: the only supported way to build a key is
/// [`TextCompileRequest::cache_key`], which keeps the key exhaustive.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextCacheKey {
    content: String,
    /// Syntax-highlighting language for `TextKind::Code` (part of the key:
    /// highlighting changes glyph colors, so different languages must not share
    /// an entry). Empty for every other kind.
    language: String,
    /// Highlight palette fingerprint (see `HighlightPalette::cache_bits`).
    /// Two colorschemes recolour the same snippet differently, so they must not
    /// share a cache entry. `None` for un-highlighted text.
    palette_bits: Option<Vec<u32>>,
    font_family: String,
    font_size_bits: u32,
    font_weight_bits: u32,
    font_style: String,
    line_height_bits: u32,
    letter_spacing_bits: u32,
    word_spacing_bits: u32,
    color: [u8; 4],
    kind: TextKind,
    max_width_bits: u32,
    /// Wrapped-line cap (`max_lines`). Part of the key: the same text capped at
    /// two lines must not share an entry with the uncapped compile.
    max_lines: Option<usize>,
    text_align: String,
    overflow: String,
    fast_path: bool,
    font_set_id: FontSetId,
}
