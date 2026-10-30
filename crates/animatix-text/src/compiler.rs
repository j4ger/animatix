use animatix_core::error::RenderError;

use crate::fast_layout::{HardwareTextLayout, compile_text_fast, compile_text_fast_wrapped};
use crate::font_context::FontContext;
use crate::request::{TextCacheKey, TextCompileRequest, TextStyle};
use crate::typst_world::{HighlightPalette, measure_text_paths};
#[cfg(feature = "rich-text")]
use crate::typst_world::{
    compile_code, compile_math, compile_text, compile_typst, extract_glyphs_grouped,
    extract_glyphs_with_metrics, is_latin_text, is_plain_text,
};
use crate::{CompiledText, TextPath};

// Runtime text compilation

/// Identifies the kind of text being compiled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextKind {
    /// Plain text.
    Text,
    /// Typst math.
    Math,
    /// Code text.
    Code,
    /// Typst markup.
    Typst,
}

/// Default font size used when a text primitive has no `font_size` property.
/// Code renders smaller by convention; Typst markup and math use the prose size.
pub fn default_font_size(kind: TextKind) -> f32 {
    match kind {
        TextKind::Code => 24.0,
        TextKind::Text | TextKind::Math | TextKind::Typst => 48.0,
    }
}

/// Memoized output of one text compilation: centered glyph paths plus metrics.
#[derive(Clone)]
pub struct CachedText {
    /// Glyph paths, centered around the origin.
    pub paths: std::sync::Arc<[TextPath]>,
    /// Font ascent in scene units (see [`CompiledText::ascent`]).
    pub ascent: f32,
    /// Font descent in scene units (see [`CompiledText::descent`]).
    pub descent: f32,
    /// Baseline offset from center (see [`CompiledText::baseline_offset`]).
    pub baseline_offset: f32,
    /// Half size `[w/2, h/2]` as returned by [`measure_text_paths`].
    pub half_size: [f32; 2],
    /// Optional hardware glyph layout for direct Vello draw_glyphs execution.
    pub hardware_layout: Option<std::sync::Arc<HardwareTextLayout>>,
}

/// Memoized grouped Typst output (the `Equation` path): glyph paths plus one
/// range per `#box()` wrapper.
#[derive(Clone)]
pub struct CachedGroupedText {
    /// Glyph paths in draw order, as returned by [`extract_glyphs_grouped`].
    pub glyphs: std::sync::Arc<[TextPath]>,
    /// One range per `#box()` wrapper, indexing into [`Self::glyphs`].
    pub ranges: std::sync::Arc<[std::ops::Range<usize>]>,
}

/// Process-wide memo table for compiled text, keyed by every input. It lives
/// outside `Timeline` because the GUI rebuilds per keystroke; a shared table makes
/// an unchanged actor cost one hash lookup instead of a full Typst pass.
type TextCompileCache = std::collections::HashMap<TextCacheKey, std::sync::Arc<CachedText>>;

fn compile_cache() -> &'static std::sync::Mutex<TextCompileCache> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<std::sync::Mutex<TextCompileCache>> = OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(TextCompileCache::new()))
}

/// Lock the compile cache, recovering the map if a panic poisoned it.
fn lock_compile_cache() -> std::sync::MutexGuard<'static, TextCompileCache> {
    compile_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Process-wide memo for grouped (Equation) compilations, keyed by the same
/// [`TextCacheKey`] so font-epoch invalidation applies to both tables.
type GroupedTextCache = std::collections::HashMap<TextCacheKey, std::sync::Arc<CachedGroupedText>>;

fn grouped_compile_cache() -> &'static std::sync::Mutex<GroupedTextCache> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<std::sync::Mutex<GroupedTextCache>> = OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(GroupedTextCache::new()))
}

/// Lock the grouped compile cache, recovering the map if a panic poisoned it.
fn lock_grouped_cache() -> std::sync::MutexGuard<'static, GroupedTextCache> {
    grouped_compile_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Upper bound on memoized compilations; exceeding it drops roughly half to
/// avoid a full-clear spike.
const TEXT_COMPILE_CACHE_CAP: usize = 1000;

/// Halve a memo table once it exceeds [`TEXT_COMPILE_CACHE_CAP`].
fn evict_text_compile_cache<K, V>(cache: &mut std::collections::HashMap<K, V>)
where
    K: Clone + Eq + std::hash::Hash,
{
    if cache.len() > TEXT_COMPILE_CACHE_CAP {
        let to_remove: Vec<_> = cache.keys().take(cache.len() / 2).cloned().collect();
        for k in to_remove {
            cache.remove(&k);
        }
    }
}

/// Drop every memoized text compilation when the font environment changes (or
/// for cold-build benchmarks). Clears the grouped table too: it is keyed on the
/// same font-epoch-guarded inputs.
pub fn clear_text_compile_cache() {
    lock_compile_cache().clear();
    lock_grouped_cache().clear();
}

/// Number of memoized text compilations (for tests and diagnostics).
pub fn text_compile_cache_len() -> usize {
    lock_compile_cache().len()
}

/// Number of memoized grouped (Equation) compilations (for tests and diagnostics).
pub fn grouped_text_compile_cache_len() -> usize {
    lock_grouped_cache().len()
}

/// Compile text into glyph paths plus metrics, memoized process-wide. All text
/// kinds funnel through here; identical requests pay the Typst cost once.
pub fn compile_text_cached(
    request: &TextCompileRequest<'_>,
    font_ctx: &FontContext,
) -> Result<std::sync::Arc<CachedText>, RenderError> {
    let kind = request.kind;
    let content = request.content;
    let style = &request.style;
    let font_family = style.font_family;
    let font_size = style.font_size;
    let font_weight = style.font_weight;
    let font_style = style.font_style;
    let line_height = style.line_height;
    let letter_spacing = style.letter_spacing;
    let word_spacing = style.word_spacing;
    let color = style.color;
    let max_width = request.max_width;
    let max_lines = request.max_lines;
    let text_align = request.text_align;
    let overflow = request.overflow;
    let key = request.cache_key(font_ctx);
    #[cfg(feature = "rich-text")]
    let key_color = request.solid_rgba();

    if let Some(hit) = lock_compile_cache().get(&key) {
        return Ok(std::sync::Arc::clone(hit));
    }

    // Slim playback profile: Typst is compiled out, so every kind degrades to
    // the fast path (Code loses highlighting, Math renders its source literally).
    #[cfg(feature = "rich-text")]
    let use_fast_path = request.allow_fast_path
        && kind == TextKind::Text
        && is_plain_text(content)
        && is_latin_text(content);
    #[cfg(not(feature = "rich-text"))]
    let use_fast_path = true;
    let compiled: CompiledText = if use_fast_path {
        tracing::debug!(
            "text cache miss (fast path): '{}' (family={}, size={}, max_width={})",
            content,
            font_family,
            font_size,
            max_width
        );
        let fast_color = if kind == TextKind::Text {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            color
        };
        if max_width > 0.0 || content.contains('\n') {
            compile_text_fast_wrapped(
                content,
                font_family,
                font_weight,
                font_style,
                font_size,
                line_height,
                fast_color,
                letter_spacing,
                word_spacing,
                font_ctx,
                max_width,
                max_lines,
                text_align,
                overflow,
            )?
        } else {
            compile_text_fast(
                content,
                font_family,
                font_weight,
                font_style,
                font_size,
                fast_color,
                letter_spacing,
                word_spacing,
                font_ctx,
            )?
        }
    } else {
        #[cfg(feature = "rich-text")]
        {
            tracing::debug!(
                "text cache miss (typst): '{}' (kind={:?}, max_width={})",
                content,
                kind,
                max_width
            );
            // `max_lines` only exists on the fast text path. Warn when a caller
            // asks for a line cap on the Typst route with a wrap width.
            if max_lines.is_some() && max_width > 0.0 {
                tracing::warn!(
                    "max_lines is not applied on the Typst text path (kind={:?}); \
                     the request is dropped for this content",
                    kind
                );
            }
            let typst_color = typst::visualize::Color::from_u8(
                key_color[0],
                key_color[1],
                key_color[2],
                key_color[3],
            );
            let frame = match kind {
                TextKind::Text => compile_text(
                    content,
                    font_size,
                    typst_color,
                    font_family,
                    font_ctx,
                    font_weight,
                    font_style,
                    line_height,
                    letter_spacing,
                    word_spacing,
                    max_width,
                    text_align,
                    overflow,
                )?,
                TextKind::Math => compile_math(
                    content,
                    font_size,
                    typst_color,
                    font_family,
                    font_ctx,
                    max_width,
                    text_align,
                    overflow,
                )?,
                TextKind::Code => compile_code(
                    content,
                    request.language,
                    request.palette,
                    font_size,
                    typst_color,
                    font_family,
                    font_ctx,
                    font_weight,
                    font_style,
                    line_height,
                    letter_spacing,
                    word_spacing,
                    max_width,
                    text_align,
                    overflow,
                )?,
                TextKind::Typst => compile_typst(
                    content,
                    font_size,
                    typst_color,
                    font_family,
                    font_ctx,
                    font_weight,
                    font_style,
                    line_height,
                    letter_spacing,
                    word_spacing,
                    max_width,
                    text_align,
                    overflow,
                )?,
            };
            extract_glyphs_with_metrics(&frame)
        }

        #[cfg(not(feature = "rich-text"))]
        {
            tracing::debug!(
                "text cache miss (fast fallback, rich-text disabled): '{}' (kind={:?})",
                content,
                kind
            );
            if max_width > 0.0 || content.contains('\n') {
                compile_text_fast_wrapped(
                    content,
                    font_family,
                    font_weight,
                    font_style,
                    font_size,
                    line_height,
                    color,
                    letter_spacing,
                    word_spacing,
                    font_ctx,
                    max_width,
                    max_lines,
                    text_align,
                    overflow,
                )?
            } else {
                compile_text_fast(
                    content,
                    font_family,
                    font_weight,
                    font_style,
                    font_size,
                    color,
                    letter_spacing,
                    word_spacing,
                    font_ctx,
                )?
            }
        }
    };

    let half_size = measure_text_paths(&compiled.glyphs);
    let entry = std::sync::Arc::new(CachedText {
        paths: compiled.glyphs.into(),
        ascent: compiled.ascent,
        descent: compiled.descent,
        baseline_offset: compiled.baseline_offset,
        half_size,
        hardware_layout: compiled.hardware_layout.map(std::sync::Arc::new),
    });

    {
        let mut cache = lock_compile_cache();
        cache.insert(key, std::sync::Arc::clone(&entry));
        evict_text_compile_cache(&mut cache);
    }
    Ok(entry)
}

/// Resolve the slim grouped path's typography. Only valid CSS font-style names
/// and positive finite line-height multipliers pass through; every other value
/// falls back to the plain-text defaults. Without this the slim profile
/// silently replaced the caller's style with `"normal"` and `1.2`.
#[cfg(not(feature = "rich-text"))]
pub(crate) fn slim_grouped_typography(font_style: &str, line_height: f32) -> (&str, f32) {
    let font_style = match font_style {
        "normal" | "italic" | "oblique" => font_style,
        _ => "normal",
    };
    let line_height = if line_height.is_finite() && line_height > 0.0 {
        line_height
    } else {
        1.2
    };
    (font_style, line_height)
}

/// Compile Typst markup into per-`#box()` glyph groups, memoized process-wide.
/// The Equation path recompiles per frame (~2.5ms each without this memo;
/// `benches/equation_frame.rs`), keyed like [`compile_text_cached`].
#[allow(clippy::too_many_arguments)] // Mirrors the compile_typst parameter set it wraps.
pub fn compile_typst_grouped_cached(
    typst_markup: &str,
    font_size: f32,
    color: [f32; 4],
    font_family: &str,
    font_ctx: &FontContext,
    font_weight: f32,
    font_style: &str,
    line_height: f32,
    letter_spacing: f32,
    word_spacing: f32,
) -> Result<std::sync::Arc<CachedGroupedText>, RenderError> {
    // Record the Equation path's fixed policy (unwrapped, left-aligned, visible)
    // in a request so the grouped table shares the flat table's key space.
    let request = TextCompileRequest {
        kind: TextKind::Typst,
        content: typst_markup,
        language: "",
        palette: None,
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
        max_width: 0.0,
        max_lines: None,
        text_align: "left",
        overflow: "visible",
        allow_fast_path: false,
    };
    let key = request.cache_key(font_ctx);

    if let Some(hit) = lock_grouped_cache().get(&key) {
        return Ok(std::sync::Arc::clone(hit));
    }

    #[cfg(feature = "rich-text")]
    {
        let rgba = request.solid_rgba();
        let typst_color = typst::visualize::Color::from_u8(rgba[0], rgba[1], rgba[2], rgba[3]);
        let frame = compile_typst(
            typst_markup,
            font_size,
            typst_color,
            font_family,
            font_ctx,
            font_weight,
            font_style,
            line_height,
            letter_spacing,
            word_spacing,
            0.0,
            "left",
            "visible",
        )?;
        let (glyphs, ranges) = extract_glyphs_grouped(&frame);
        let entry = std::sync::Arc::new(CachedGroupedText {
            glyphs: glyphs.into(),
            ranges: ranges.into(),
        });

        {
            let mut cache = lock_grouped_cache();
            cache.insert(key, std::sync::Arc::clone(&entry));
            evict_text_compile_cache(&mut cache);
        }
        Ok(entry)
    }

    #[cfg(not(feature = "rich-text"))]
    {
        // Slim profile: render the markup source as plain text in one group.
        // The caller's requested style and line height are meaningful here;
        // only invalid values fall back to the plain-text defaults.
        let (font_style, line_height) = slim_grouped_typography(font_style, line_height);
        let compiled = compile_text_fast_wrapped(
            typst_markup,
            font_family,
            font_weight,
            font_style,
            font_size,
            line_height,
            color,
            letter_spacing,
            word_spacing,
            font_ctx,
            0.0,
            None,
            "left",
            "visible",
        )?;
        // One range covering the whole group; a `vec![a..b]` here reads as an
        // attempt to build the values in the range (clippy).
        let ranges: Vec<_> = std::iter::once(0..compiled.glyphs.len()).collect();
        let entry = std::sync::Arc::new(CachedGroupedText {
            glyphs: compiled.glyphs.into(),
            ranges: ranges.into(),
        });
        {
            let mut cache = lock_grouped_cache();
            cache.insert(key, std::sync::Arc::clone(&entry));
            evict_text_compile_cache(&mut cache);
        }
        Ok(entry)
    }
}

/// Runtime text compiler backed by the process-wide compile cache, so identical
/// inputs survive `Timeline` rebuilds.
#[derive(Clone)]
pub struct TextCompiler {
    /// Whether the plain-text fast path is enabled (default true); can be
    /// disabled via `config { text_fast_path: false }`.
    pub text_fast_path: bool,
    /// Role→RGBA palette for `Code` syntax highlighting, derived from the active
    /// colorscheme. `None` keeps Typst's fixed raw theme. Timeline-wide constant.
    pub highlight_palette: Option<HighlightPalette>,
}

impl Default for TextCompiler {
    fn default() -> Self {
        Self {
            text_fast_path: true,
            highlight_palette: None,
        }
    }
}

impl TextCompiler {
    /// Create a new text compiler with fast path enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compile a request into glyph paths, using the process-wide cache. Cache
    /// hits return an `Arc<[TextPath]>` — one refcount bump instead of cloning
    /// paths.
    ///
    /// The request is the complete input. Engine callers fold this compiler's
    /// timeline-wide policy into the request they build once: `allow_fast_path`
    /// comes from [`TextCompiler::text_fast_path`] and `palette` from
    /// [`TextCompiler::highlight_palette`]. Keeping those fields in the request
    /// means the compiler and direct [`compile_text_cached`] callers share one
    /// cache key.
    pub fn compile_request(
        &self,
        request: &TextCompileRequest<'_>,
        font_ctx: &FontContext,
    ) -> Result<std::sync::Arc<[TextPath]>, RenderError> {
        let cached = compile_text_cached(request, font_ctx)?;
        Ok(std::sync::Arc::clone(&cached.paths))
    }
}
