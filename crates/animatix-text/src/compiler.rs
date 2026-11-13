use animatix_core::error::RenderError;

use crate::fast_layout::{HardwareTextLayout, compile_text_fast, compile_text_fast_wrapped};
use crate::font_context::FontContext;
use crate::request::{TextCacheKey, TextCompileRequest, TextStyle};
use crate::typst_world::{HighlightPalette, measure_text_paths};
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
/// range per authored fragment.
#[derive(Clone)]
pub struct CachedGroupedText {
    /// Glyph paths in draw order, as returned by the rich text compiler.
    pub glyphs: std::sync::Arc<[TextPath]>,
    /// One range per authored fragment, indexing into [`Self::glyphs`].
    pub ranges: std::sync::Arc<[std::ops::Range<usize>]>,
}

/// Process-wide memo table for compiled text, keyed by every input. It lives
/// outside `Timeline` because the GUI rebuilds per keystroke; a shared table makes
/// an unchanged actor cost one hash lookup instead of a full Typst pass.
type TextCompileCache = std::collections::HashMap<TextCacheKey, std::sync::Arc<CachedText>>;

#[cfg(not(test))]
fn compile_cache() -> &'static std::sync::Mutex<TextCompileCache> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<std::sync::Mutex<TextCompileCache>> = OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(TextCompileCache::new()))
}

/// Run `f` against the process-wide compile memo table, recovering the map if a
/// panic poisoned it.
#[cfg(not(test))]
fn with_compile_cache<R>(f: impl FnOnce(&mut TextCompileCache) -> R) -> R {
    let mut cache = compile_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut cache)
}

// Test builds get a **thread-local** memo table.
//
// The unit-test binary runs many tests in parallel in one process. A shared
// table let one test's `clear_text_compile_cache` (or the cap eviction) drop an
// entry another test was still asserting on — `compile_cache_keys_on_wrapping_params`
// compared `Arc` pointers and flaked three runs in eight. Per-thread storage
// gives every test the same isolation it would get from its own process, with no
// test-order discipline to maintain.
#[cfg(test)]
thread_local! {
    static TEST_COMPILE_CACHE: std::cell::RefCell<TextCompileCache> =
        std::cell::RefCell::new(TextCompileCache::new());
}

#[cfg(test)]
fn with_compile_cache<R>(f: impl FnOnce(&mut TextCompileCache) -> R) -> R {
    TEST_COMPILE_CACHE.with(|cache| f(&mut cache.borrow_mut()))
}

/// Process-wide memo for grouped (Equation) compilations, keyed by the same
/// [`TextCacheKey`] so the same font-set identity applies to both tables.
type GroupedTextCache = std::collections::HashMap<TextCacheKey, std::sync::Arc<CachedGroupedText>>;

#[cfg(not(test))]
fn grouped_compile_cache() -> &'static std::sync::Mutex<GroupedTextCache> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<std::sync::Mutex<GroupedTextCache>> = OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(GroupedTextCache::new()))
}

#[cfg(not(test))]
fn with_grouped_cache<R>(f: impl FnOnce(&mut GroupedTextCache) -> R) -> R {
    let mut cache = grouped_compile_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut cache)
}

#[cfg(test)]
thread_local! {
    static TEST_GROUPED_CACHE: std::cell::RefCell<GroupedTextCache> =
        std::cell::RefCell::new(GroupedTextCache::new());
}

#[cfg(test)]
fn with_grouped_cache<R>(f: impl FnOnce(&mut GroupedTextCache) -> R) -> R {
    TEST_GROUPED_CACHE.with(|cache| f(&mut cache.borrow_mut()))
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

/// Drop memoized text compilations (also used by cold-build benchmarks).
/// Font changes are isolated by content identity and do not require a global
/// cache clear.
pub fn clear_text_compile_cache() {
    with_compile_cache(|cache| cache.clear());
    with_grouped_cache(|cache| cache.clear());
}

/// Number of memoized text compilations (for tests and diagnostics).
pub fn text_compile_cache_len() -> usize {
    with_compile_cache(|cache| cache.len())
}

/// Number of memoized grouped (Equation) compilations (for tests and diagnostics).
pub fn grouped_text_compile_cache_len() -> usize {
    with_grouped_cache(|cache| cache.len())
}

/// Look up a memoized compilation for `key`, cloning the cached entry.
fn lookup_cached_text(key: &TextCacheKey) -> Option<std::sync::Arc<CachedText>> {
    with_compile_cache(|cache| cache.get(key).cloned())
}

/// Compile a request that missed the memo table: pick the fast or Typst route
/// and produce centered glyph paths plus metrics. The caller memoizes the result.
fn compile_uncached(
    request: &TextCompileRequest<'_>,
    font_ctx: &FontContext,
) -> Result<CompiledText, RenderError> {
    if request.kind != TextKind::Text {
        return crate::rich_capability::compile(request, font_ctx);
    }
    let style = &request.style;
    // Ordinary Text is literal on every platform; its per-glyph color is
    // applied by the engine, independently of geometry and text cache entries.
    let color = [1.0, 1.0, 1.0, 1.0];
    if request.max_width > 0.0 || request.content.contains('\n') {
        compile_text_fast_wrapped(
            request.content,
            style.font_family,
            style.font_weight,
            style.font_style,
            style.font_size,
            style.line_height,
            color,
            style.letter_spacing,
            style.word_spacing,
            font_ctx,
            request.max_width,
            request.max_lines,
            request.text_align,
            request.overflow,
        )
    } else {
        compile_text_fast(
            request.content,
            style.font_family,
            style.font_weight,
            style.font_style,
            style.font_size,
            color,
            style.letter_spacing,
            style.word_spacing,
            font_ctx,
        )
    }
}

/// Measure a fresh compilation and memoize it under `key`, evicting when the
/// table is full. Returns the stored entry.
fn store_cached_text(key: TextCacheKey, compiled: CompiledText) -> std::sync::Arc<CachedText> {
    let half_size = measure_text_paths(&compiled.glyphs);
    let entry = std::sync::Arc::new(CachedText {
        paths: compiled.glyphs.into(),
        ascent: compiled.ascent,
        descent: compiled.descent,
        baseline_offset: compiled.baseline_offset,
        half_size,
        hardware_layout: compiled.hardware_layout.map(std::sync::Arc::new),
    });

    with_compile_cache(|cache| {
        cache.insert(key, std::sync::Arc::clone(&entry));
        evict_text_compile_cache(cache);
    });
    entry
}

/// Compile text into glyph paths plus metrics, memoized process-wide. All text
/// kinds funnel through here; identical requests pay the Typst cost once.
pub fn compile_text_cached(
    request: &TextCompileRequest<'_>,
    font_ctx: &FontContext,
) -> Result<std::sync::Arc<CachedText>, RenderError> {
    let key = request.cache_key(font_ctx);

    if let Some(hit) = lookup_cached_text(&key) {
        return Ok(hit);
    }

    Ok(store_cached_text(key, compile_uncached(request, font_ctx)?))
}

/// Compile Typst markup into metadata-marked Equation fragment groups, memoized process-wide.
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

    if let Some(hit) = with_grouped_cache(|cache| cache.get(&key).cloned()) {
        return Ok(hit);
    }

    let entry = std::sync::Arc::new(crate::rich_capability::grouped(&request, font_ctx)?);
    with_grouped_cache(|cache| {
        cache.insert(key, std::sync::Arc::clone(&entry));
        evict_text_compile_cache(cache);
    });
    Ok(entry)
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
