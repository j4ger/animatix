use animatix_core::error::RenderError;

use crate::fast_layout::{HardwareTextLayout, compile_text_fast, compile_text_fast_wrapped};
use crate::font_context::FontContext;
use crate::typst_world::{HighlightPalette, measure_text_paths};
#[cfg(feature = "rich-text")]
use crate::typst_world::{
    compile_code, compile_math, compile_text, compile_typst, extract_glyphs_grouped,
    extract_glyphs_with_metrics, is_latin_text, is_plain_text,
};
use crate::{CompiledText, TextPath};

// ─────────────────────────────────────────────────────────────
// Runtime text recompilation (Phase 2)
// ─────────────────────────────────────────────────────────────

/// Identifies the kind of text being compiled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextKind {
    /// Plain text.
    Text,
    /// Typst math (kept for backward compatibility with old cached paths).
    #[allow(dead_code)] // Kept for backward-compatible cached path decoding
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

/// Cache key for compiled text paths.
/// Since `f32` is not `Hash`, we bit-cast it to `u32` for the key.
///
/// The key must cover *every* input that can change the compiled output:
/// content, typography, color, kind, the wrapping parameters
/// (`max_width`/`text_align`/`overflow`), whether the plain-text fast path
/// may be used (fast-path and Typst-engine output are visually equivalent but
/// not bit-identical, so they are cached under distinct keys), and the
/// font-environment epoch (see [`advance_font_env_epoch`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextCacheKey {
    content: String,
    /// Syntax-highlighting language for `TextKind::Code` (part of the cache
    /// key: highlighting changes glyph colors, so different languages must
    /// not share an entry). Empty for every other kind.
    language: String,
    /// Highlight palette fingerprint (see [`HighlightPalette::cache_bits`]).
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
    /// two lines must not share a cache entry with the uncapped compile.
    max_lines: Option<usize>,
    text_align: String,
    overflow: String,
    fast_path: bool,
    font_epoch: u64,
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
/// All Text / Code / Typst / Math compilation should funnel through here so
/// identical inputs — within a build, across rebuilds, and across Timelines —
/// pay the Typst engine cost exactly once. `allow_fast_path` only affects the
/// `TextKind::Text` plain-Latin routing; callers that previously used the Typst
/// engine unconditionally pass `false` to preserve output-identical behavior.
#[allow(clippy::too_many_arguments)] // Mirrors the compile_* parameter sets it dispatches to.
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
) -> Result<std::sync::Arc<CachedText>, RenderError> {
    let key_color = if kind == TextKind::Text {
        [255, 255, 255, 255]
    } else {
        [
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
            (color[3] * 255.0) as u8,
        ]
    };
    let key = TextCacheKey {
        content: content.to_string(),
        language: language.to_string(),
        palette_bits: palette.map(HighlightPalette::cache_bits),
        font_family: font_family.to_string(),
        font_size_bits: font_size.to_bits(),
        font_weight_bits: font_weight.to_bits(),
        font_style: font_style.to_string(),
        line_height_bits: line_height.to_bits(),
        letter_spacing_bits: letter_spacing.to_bits(),
        word_spacing_bits: word_spacing.to_bits(),
        color: key_color,
        kind,
        max_width_bits: max_width.to_bits(),
        max_lines,
        text_align: text_align.to_string(),
        overflow: overflow.to_string(),
        fast_path: allow_fast_path,
        font_epoch: font_ctx.epoch,
    };

    if let Some(hit) = lock_compile_cache().get(&key) {
        return Ok(std::sync::Arc::clone(hit));
    }

    // Slim playback profile: the Typst stack is compiled out, so every kind
    // degrades to the plain fast path (Code loses syntax highlighting, Math
    // renders its source literally, non-Latin scripts need a system font). The
    // per-kind probes below only exist in the rich-text build, so the slim build
    // binds the constant instead of computing it and discarding the result.
    #[cfg(feature = "rich-text")]
    let use_fast_path = allow_fast_path
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
                key.color[0],
                key.color[1],
                key.color[2],
                key.color[3],
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
                    language,
                    palette,
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
/// Keyed on the same inputs as [`crate::compile_text_cached`] (including the font
/// epoch), so a hit is bit-identical to recompiling.
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
    let key = TextCacheKey {
        content: typst_markup.to_string(),
        // The grouped Typst path (Equation/Fragment) has no Code highlighting.
        language: String::new(),
        palette_bits: None,
        font_family: font_family.to_string(),
        font_size_bits: font_size.to_bits(),
        font_weight_bits: font_weight.to_bits(),
        font_style: font_style.to_string(),
        line_height_bits: line_height.to_bits(),
        letter_spacing_bits: letter_spacing.to_bits(),
        word_spacing_bits: word_spacing.to_bits(),
        color: [
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
            (color[3] * 255.0) as u8,
        ],
        kind: TextKind::Typst,
        // The Equation path always compiles unwrapped, left-aligned, visible.
        max_width_bits: 0.0f32.to_bits(),
        max_lines: None,
        text_align: "left".to_string(),
        overflow: "visible".to_string(),
        fast_path: false,
        font_epoch: font_ctx.epoch,
    };

    if let Some(hit) = lock_grouped_cache().get(&key) {
        return Ok(std::sync::Arc::clone(hit));
    }

    #[cfg(feature = "rich-text")]
    {
        let typst_color = typst::visualize::Color::from_u8(
            key.color[0],
            key.color[1],
            key.color[2],
            key.color[3],
        );
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
/// from the shared table (see [`crate::compile_text_cached`]), which also survives
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
    /// `language` is the `Code` syntax-highlighting tag (empty for every other
    /// kind and for plain `Code`).
    #[allow(clippy::too_many_arguments)] // Mirrors the compile_* parameter sets it dispatches to.
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
        let cached = compile_text_cached(
            kind,
            content,
            language,
            self.highlight_palette.as_ref(),
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            letter_spacing,
            word_spacing,
            color,
            max_width,
            None,
            text_align,
            overflow,
            self.text_fast_path,
            font_ctx,
        )?;
        Ok(std::sync::Arc::clone(&cached.paths))
    }
}
