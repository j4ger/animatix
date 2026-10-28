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

// ─────────────────────────────────────────────────────────────
// Runtime text compilation
// ─────────────────────────────────────────────────────────────

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
///
/// Single source of truth so build-time and frame-time default sizes cannot
/// drift apart. Code is rendered smaller than prose by convention; Typst
/// markup and math default to the prose size.
pub fn default_font_size(kind: TextKind) -> f32 {
    match kind {
        TextKind::Code => 24.0,
        TextKind::Text | TextKind::Math | TextKind::Typst => 48.0,
    }
}

/// Memoized output of one text compilation: centered glyph paths plus the
/// metrics and half-size that every consumer derives from them.
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

/// Memoized output of one *grouped* Typst compilation (the `Equation` path).
///
/// The Equation primitive wraps every `Fragment` in a `#box()` so the frame
/// exposes one group per fragment. The flat glyph list alone cannot recover
/// those boundaries, so this shape keeps them alongside the glyphs.
#[derive(Clone)]
pub struct CachedGroupedText {
    /// Glyph paths in draw order, as returned by [`extract_glyphs_grouped`].
    pub glyphs: std::sync::Arc<[TextPath]>,
    /// One range per `#box()` wrapper, indexing into [`Self::glyphs`].
    pub ranges: std::sync::Arc<[std::ops::Range<usize>]>,
}

/// Process-wide memoization table for compiled text.
///
/// Entries are keyed by every compilation input, so a hit is bit-identical to
/// recompiling. The table deliberately lives outside [`TextCompiler`] and
/// [`Timeline`] instances: the GUI rebuilds the whole `Timeline` on every
/// keystroke, and per-instance caches would die with each rebuild, forcing a
/// full Typst parse + eval + layout pass for every unchanged text actor. With
/// the shared table, an unchanged declaration's rebuild cost drops to one hash
/// lookup. Font data is process-global and read-only after load, so it cannot
/// invalidate entries behind the key.
type TextCompileCache = std::collections::HashMap<TextCacheKey, std::sync::Arc<CachedText>>;

fn compile_cache() -> &'static std::sync::Mutex<TextCompileCache> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<std::sync::Mutex<TextCompileCache>> = OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(TextCompileCache::new()))
}

/// Lock the compile cache, recovering the map if a panic poisoned it.
///
/// Entries are immutable values, so a poisoned map is still structurally valid;
/// recovery keeps the cache available instead of panicking unrelated rebuilds.
fn lock_compile_cache() -> std::sync::MutexGuard<'static, TextCompileCache> {
    compile_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Process-wide memo for grouped (Equation) Typst compilations.
///
/// A separate table from [`TextCompileCache`] because the value shape differs,
/// but keyed by the same [`TextCacheKey`] so font-epoch invalidation applies
/// identically to both.
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

/// Upper bound on memoized compilations. When exceeded, roughly half the
/// entries are dropped to avoid a full-clear spike (same policy as before the
/// cache became process-wide).
const TEXT_COMPILE_CACHE_CAP: usize = 1000;

/// Halve a memo table once it exceeds [`TEXT_COMPILE_CACHE_CAP`].
///
/// Generic so the flat and grouped tables share one eviction policy.
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

/// Drop every memoized text compilation.
///
/// Needed when the font environment changes underneath the cache (e.g. a GUI
/// reloads system fonts) and by benchmarks measuring cold-build cost. Clears
/// the grouped table too — it is keyed on the same font-epoch-guarded inputs
/// and would otherwise serve frames laid out with the old fonts.
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

/// Compile text into glyph paths plus metrics, memoized process-wide.
///
/// All Text / Code / Typst / Math compilation funnels through here.
/// [`TextCompileRequest`] is the whole input and the memo identity is derived
/// from it (see [`TextCompileRequest::cache_key`]), so identical requests —
/// within a build, across rebuilds, and across Timelines — pay the Typst engine
/// cost exactly once. `request.allow_fast_path` only affects the
/// `TextKind::Text` plain-Latin routing; callers that previously used the Typst
/// engine unconditionally set it to `false` to preserve output-identical
/// behavior.
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

    // Slim playback profile: the Typst stack is compiled out, so every kind
    // degrades to the plain fast path (Code loses syntax highlighting, Math
    // renders its source literally, non-Latin scripts need a system font). The
    // per-kind probes below only exist in the rich-text build, so the slim build
    // binds the constant instead of computing it and discarding the result.
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
            // `max_lines` is implemented on the fast text path, which is the
            // route plain Latin `Text` takes. Say so when a caller asks for a
            // line cap on the Typst route rather than dropping it silently —
            // but only when it could bite: with no wrap width the text is a
            // single line, so the cap is vacuous, not dropped.
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

/// Compile Typst markup into per-`#box()` glyph groups, memoized process-wide.
///
/// This is the `Equation` / `Fragment` frame path: it recompiles on every frame
/// because fragments are aggregated at frame time, and without a memo each
/// frame pays a full Typst parse + eval + layout even when no fragment content
/// or font property changed — which is the norm while scrubbing or playing
/// back. Measured at ~2.5ms per frame per Equation before memoization
/// (`benches/equation_frame.rs`).
///
/// Keyed through the same [`TextCompileRequest::cache_key`] derivation as
/// [`compile_text_cached`] (including the font epoch), so a hit is
/// bit-identical to recompiling.
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
    // The Equation path always compiles unwrapped, left-aligned and visible,
    // with no Code highlighting. Recording that policy in a request keeps the
    // grouped table in the same request-derived key space as the flat table.
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
        // Slim playback profile: render the markup source as plain text in a
        // single group (Equation/Fragment degrade; ranges collapse to one).
        let compiled = compile_text_fast_wrapped(
            typst_markup,
            font_family,
            font_weight,
            "normal",
            font_size,
            1.2,
            color,
            letter_spacing,
            word_spacing,
            font_ctx,
            0.0,
            None,
            "left",
            "visible",
        )?;
        // One range covering the whole group. Written as a collect rather than
        // `vec![a..b]`, which clippy reads as an attempt to build the values in
        // the range.
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

/// Runtime text compiler backed by the process-wide compile cache.
///
/// When text properties change at runtime (e.g. via `always` reactive blocks),
/// this service recompiles glyph paths on-demand; identical inputs are served
/// from the shared table (see [`compile_text_cached`]), which also survives
/// `Timeline` rebuilds.
#[derive(Clone)]
pub struct TextCompiler {
    /// Whether the plain-text fast path is enabled (default: true).
    /// Can be disabled via `config { text_fast_path: false }` for debugging.
    pub text_fast_path: bool,
    /// Role→RGBA palette for `Code` syntax highlighting, derived from the
    /// active colorscheme. `None` keeps Typst's fixed raw theme. Set once per
    /// `Timeline` at build (see the engine's `apply_colorscheme`); the palette
    /// is a timeline-wide constant, so it lives here rather than per-actor.
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

    /// Compile text into glyph paths, using the process-wide cache when possible.
    /// Cache hits return an `Arc<[TextPath]>` — a single refcount increment
    /// instead of cloning the entire vector of BezPath objects.
    ///
    /// This flat-argument form is the adapter the engine's render path still
    /// uses: it folds `self.text_fast_path` and the timeline's highlight palette
    /// into a [`TextCompileRequest`] and delegates to [`compile_text_cached`].
    ///
    /// `language` is the `Code` syntax-highlighting tag (empty for every other
    /// kind and for plain `Code`).
    #[allow(clippy::too_many_arguments)] // Flat adapter for the engine render path.
    pub fn compile(
        &mut self,
        content: &str,
        font_family: &str,
        font_size: f32,
        font_weight: f32,
        font_style: &str,
        line_height: f32,
        letter_spacing: f32,
        word_spacing: f32,
        color: [f32; 4],
        kind: TextKind,
        language: &str,
        font_ctx: &FontContext,
        max_width: f32,
        text_align: &str,
        overflow: &str,
    ) -> Result<std::sync::Arc<[TextPath]>, RenderError> {
        let request = TextCompileRequest {
            kind,
            content,
            language,
            palette: self.highlight_palette.as_ref(),
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
            max_lines: None,
            text_align,
            overflow,
            allow_fast_path: self.text_fast_path,
        };
        let cached = compile_text_cached(&request, font_ctx)?;
        Ok(std::sync::Arc::clone(&cached.paths))
    }
}
