//! Text and math compilation: the typst/font stack as a pure service.
//!
//! Inputs are plain data (content strings, font settings, a font database);
//! outputs are glyph paths and metrics. No engine types, no scene graph, no
//! GPU — the engine calls this during build (text measurement feeds layout)
//! and frame evaluation (glyph runs), and the render layer only ever sees the
//! produced paths.

use std::collections::HashMap;
// Only the rich-text fallback-font path shares face data as `Arc<[u8]>`; the
// plain fast path qualifies `std::sync::Arc` at its (always-compiled) uses.
#[cfg(feature = "rich-text")]
use std::sync::Arc;

use animatix_core::error::RenderError;
use kurbo::{Affine, BezPath, Point, Shape};

pub mod transcript;

/// A glyph path extracted from text, with color and opacity.
#[derive(Debug, Clone)]
pub struct TextPath {
    /// The glyph path.
    pub path: BezPath,
    /// The solid glyph fill as RGBA bytes. (Rich-text paints degrade to
    /// their solid RGBA; gradients already fell back to white.)
    pub color: [u8; 4],
    /// The opacity of the glyph (0.0–1.0).
    pub opacity: f32,
    /// Line index this glyph belongs to (0-indexed).
    pub line_idx: u16,
    /// Word index this glyph belongs to (0-indexed).
    pub word_idx: u16,
    /// Character index within the text content (0-indexed).
    pub char_idx: u16,
    /// Bounding box center of this glyph in local coordinates.
    pub glyph_center: [f32; 2],
    /// Cached bounding box of this glyph in local coordinates.
    pub bounds: kurbo::Rect,
}

impl TextPath {
    /// Create a new TextPath with basic fields and default layout indices.
    pub fn new(path: BezPath, color: [u8; 4], opacity: f32) -> Self {
        use kurbo::Shape as _;
        let b = path.bounding_box();
        let center = if b.width().is_finite() && b.height().is_finite() {
            [((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32]
        } else {
            [0.0, 0.0]
        };
        Self {
            path,
            color,
            opacity,
            line_idx: 0,
            word_idx: 0,
            char_idx: 0,
            glyph_center: center,
            bounds: b,
        }
    }
}

/// Convert an f32 RGBA color (0.0–1.0 components) to RGBA bytes.
pub fn f32_color_to_rgba8(color: &[f32; 4]) -> [u8; 4] {
    [
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0) as u8,
    ]
}

/// Degrade a rich-text paint to solid RGBA bytes (gradients fall back to
/// white — matching the engine's previous behaviour for non-solid paints).
#[cfg(feature = "rich-text")]
pub fn paint_to_rgba(paint: &typst::visualize::Paint) -> [u8; 4] {
    match paint {
        typst::visualize::Paint::Solid(c) => {
            let v = c.to_vec4_u8();
            [v[0], v[1], v[2], v[3]]
        },
        _ => [255, 255, 255, 255],
    }
}

// ─────────────────────────────────────────────────────────────
// Text metrics for the plain-text fast path
// ─────────────────────────────────────────────────────────────

/// Font metrics extracted from `ttf_parser::Face`.
/// Used by the plain-text fast path to compute line layout.
pub struct TextMetrics {
    /// Ascent in font units (positive).
    pub ascent: f32,
    /// Descent in font units (negative).
    pub descent: f32,
    /// Line gap in font units.
    pub line_gap: f32,
    /// Units per em from the font face.
    pub units_per_em: f32,
}

/// Compiled text output containing glyph paths and font metrics.
///
/// The glyph paths are centered around (0, 0) for layout positioning.
/// The metrics provide baseline information for vertical alignment.
#[derive(Clone)]
pub struct CompiledText {
    /// Glyph paths, centered around the origin.
    pub glyphs: Vec<TextPath>,
    /// Font ascent in scene units (points), i.e. distance from baseline to top of em.
    pub ascent: f32,
    /// Font descent in scene units (points), i.e. distance from baseline to bottom of em
    /// (negative).
    pub descent: f32,
    /// Offset of the baseline from the text's center (0, 0) after centering.
    /// A positive value means the baseline is above the center.
    pub baseline_offset: f32,
    /// Optional hardware glyph layout for direct Vello draw_glyphs execution.
    pub hardware_layout: Option<HardwareTextLayout>,
}

#[cfg(feature = "rich-text")]
use typst::foundations::{Bytes, Datetime};
#[cfg(feature = "rich-text")]
use typst::layout::{Frame, FrameItem, Transform};
#[cfg(feature = "rich-text")]
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
#[cfg(feature = "rich-text")]
use typst::text::{Font, FontBook};
#[cfg(feature = "rich-text")]
use typst::utils::LazyHash;
#[cfg(feature = "rich-text")]
use typst::{Library, LibraryExt, World};

// ─────────────────────────────────────────────────────────────
// Persistent font database (FontContext)
// ─────────────────────────────────────────────────────────────

/// Owns a persistent `fontdb::Database` to avoid redundant font scanning (~45-60ms per call).
///
/// Create one `FontContext` early and share it throughout the build pipeline.
///
/// The system font database is loaded **once per process** and shared behind an
/// `Arc`, so every `FontContext::new()` (one per `Timeline::build`/rebuild)
/// reuses the same scanned faces instead of re-scanning the system on each call.
/// This turns repeated builds (e.g. per keystroke in the GUI) from O(fonts) disk
/// + parse work into an O(1) `Arc` clone.
#[derive(Clone, Debug)]
pub struct FontContext {
    /// The underlying font database, shared process-wide (only read after load).
    db: std::sync::Arc<fontdb::Database>,
    /// Whether the plain-text fast path (bypassing Typst) is enabled.
    /// Default: true. Set `text_fast_path: false` in the config block to disable.
    pub text_fast_path: bool,
    /// Font-environment generation this context was created under.
    ///
    /// Compiled-text memoization keys include this value, so every context
    /// created after [`advance_font_env_epoch`] automatically misses the cache
    /// instead of serving glyphs shaped with stale font data. Contexts created
    /// under the same epoch are guaranteed to see identical font data because
    /// the shared database is immutable after load.
    epoch: u64,
}

/// Monotonic generation counter for the process font environment.
static FONT_ENV_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn font_env_epoch() -> u64 {
    std::sync::atomic::AtomicU64::load(&FONT_ENV_EPOCH, std::sync::atomic::Ordering::Relaxed)
}

/// Declare that the process font environment changed (e.g. fonts were reloaded
/// or registered at runtime) and invalidate every memoized text compilation.
///
/// Any [`FontContext`] created *after* this call carries the new epoch, so its
/// compilations cannot collide with entries shaped under the previous font
/// data. Call this before creating contexts against the new font state; there
/// is currently no in-process font mutation, so today this is only exercised
/// by tests — it exists so a future font-reload feature has one sanctioned,
/// hard-to-miss invalidation point wired into the cache key itself.
pub fn advance_font_env_epoch() -> u64 {
    use std::sync::atomic::Ordering;
    let next = FONT_ENV_EPOCH.fetch_add(1, Ordering::Relaxed) + 1;
    // Entries from older epochs can never be hit again; reclaim them now
    // instead of waiting for capacity eviction.
    clear_text_compile_cache();
    next
}

/// Load (once) and cache the system font database for the whole process.
///
/// `fontdb::Database` is only ever read after construction in this codebase, so
/// sharing one immutable instance across all [`FontContext`]s is safe and avoids
/// re-scanning the system font dirs on every build.
fn system_fonts_db() -> &'static std::sync::Arc<fontdb::Database> {
    use std::sync::OnceLock;
    static DB: OnceLock<std::sync::Arc<fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        std::sync::Arc::new(db)
    })
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new()
    }
}

impl FontContext {
    /// Create a new font context with system fonts loaded (shared process-wide).
    /// The plain-text fast path is enabled by default.
    pub fn new() -> Self {
        Self {
            db: std::sync::Arc::clone(system_fonts_db()),
            text_fast_path: true,
            epoch: font_env_epoch(),
        }
    }

    /// Create a new font context with fast path explicitly enabled/disabled.
    pub fn with_fast_path(text_fast_path: bool) -> Self {
        Self {
            db: std::sync::Arc::clone(system_fonts_db()),
            text_fast_path,
            epoch: font_env_epoch(),
        }
    }
    #[cfg(feature = "rich-text")]
    /// Load the bold/italic emphasis faces of a family into the Typst world.
    ///
    /// `with_fonts_and_fallback` loads only the default regular face per family,
    /// so `*bold*`/`_italic_`/`font_weight` selected by Typst markup or the
    /// `font_weight`/`font_style` properties had no matching face and fell back
    /// to regular. This returns the regular + bold + italic + bold-italic faces
    /// so those variants actually render. Empty if the family has none.
    fn load_font_emphasis_faces(&self, family: &str) -> Vec<Font> {
        let combos = [
            (fontdb::Weight::NORMAL, fontdb::Style::Normal),
            (fontdb::Weight::BOLD, fontdb::Style::Normal),
            (fontdb::Weight::NORMAL, fontdb::Style::Italic),
            (fontdb::Weight::BOLD, fontdb::Style::Italic),
        ];
        let mut out = Vec::with_capacity(combos.len());
        for (weight, style) in combos {
            let id = self.db.query(&fontdb::Query {
                families: &[fontdb::Family::Name(family)],
                weight,
                style,
                ..Default::default()
            });
            let Some(id) = id else {
                continue;
            };
            let Some(data) = Self::face_data(&self.db, id) else {
                continue;
            };
            let Some(face) = self.db.face(id) else {
                continue;
            };
            let Some(font) = Font::new(Bytes::new(data), face.index) else {
                continue;
            };
            // De-dup by (family, weight, style) so repeated variants aren't re-imported.
            if out.iter().any(|f: &Font| {
                f.info().family == font.info().family
                    && f.info().variant.weight == font.info().variant.weight
                    && f.info().variant.style == font.info().variant.style
            }) {
                continue;
            }
            out.push(font);
        }
        out
    }

    fn has_family(&self, family: &str) -> bool {
        self.db
            .query(&fontdb::Query {
                families: &[fontdb::Family::Name(family)],
                ..Default::default()
            })
            .is_some()
    }

    /// Return at most one regular system face per probe character.
    ///
    /// This is a startup-only discovery helper; callers should cache the result
    /// for the application lifetime instead of calling it per frame. Preferring
    /// normal sans-serif faces keeps the egui fallback list small while still
    /// covering non-Latin scripts with a few broad fonts.
    pub fn font_for_glyphs(&self, probes: &[char]) -> Vec<(Vec<u8>, u32)> {
        let mut covered = vec![false; probes.len()];
        let mut fonts = Vec::new();

        // First pass prefers the generic UI face shape; second pass fills any
        // remaining probes from other installed faces.
        for pass in 0..2 {
            if covered.iter().all(|covered| *covered) {
                break;
            }
            for face in self.db.faces() {
                let pass_matches = if pass == 0 {
                    face.style == fontdb::Style::Normal
                        && face.weight == fontdb::Weight::NORMAL
                        && !face.monospaced
                } else {
                    face.style == fontdb::Style::Normal && !face.monospaced
                };
                if !pass_matches {
                    continue;
                }

                let Some(data) = Self::probe_face_data(&self.db, face, probes, &mut covered) else {
                    continue;
                };
                fonts.push((data, face.index));
                if covered.iter().all(|covered| *covered) {
                    return fonts;
                }
            }
        }

        // A second full scan for the rare case where every remaining probe is
        // only covered by a mono/bold face. Keep the result bounded by probes.
        for face in self.db.faces() {
            if covered.iter().all(|covered| *covered) {
                break;
            }
            let Some(data) = Self::probe_face_data(&self.db, face, probes, &mut covered) else {
                continue;
            };
            fonts.push((data, face.index));
        }

        fonts
    }

    fn probe_face_data(
        db: &fontdb::Database,
        face: &fontdb::FaceInfo,
        probes: &[char],
        covered: &mut [bool],
    ) -> Option<Vec<u8>> {
        db.with_face_data(face.id, |data, face_index| {
            let parsed = ttf_parser::Face::parse(data, face_index).ok()?;
            let mut covers_any = false;
            for (index, probe) in probes.iter().enumerate() {
                if !covered[index] && parsed.glyph_index(*probe).is_some() {
                    covered[index] = true;
                    covers_any = true;
                }
            }
            covers_any.then(|| data.to_vec())
        })
        .flatten()
    }

    /// Return a sorted list of all available font family names from the system.
    pub fn families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .db
            .faces()
            .filter_map(|face| {
                self.db.face(face.id)?.families.first().map(|(name, _)| name.clone())
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    fn face_data(db: &fontdb::Database, id: fontdb::ID) -> Option<Vec<u8>> {
        let (source, _index) = db.face_source(id)?;
        match source {
            fontdb::Source::Binary(data) => Some(data.as_ref().as_ref().to_vec()),
            fontdb::Source::File(path) => std::fs::read(path).ok(),
            fontdb::Source::SharedFile(_path, data) => Some(data.as_ref().as_ref().to_vec()),
        }
    }

    /// Load a `ttf_parser::Face` for the given family, weight and style.
    /// Returns `None` if the font cannot be found or is not a TrueType/OpenType font.
    /// Register an in-memory font so `font_family` can name it.
    ///
    /// The shared database is immutable after init, so this builds a private
    /// copy containing `bytes`, swaps it in, and bumps the font epoch to
    /// invalidate memoized text compiles. The family comes from the font's own
    /// name table — write `font_family` to match it. Both text paths pick the
    /// face up automatically: the fast path resolves it through fontdb, and the
    /// Typst path loads its emphasis faces and uses it for glyph fallback.
    pub fn load_font_bytes(&mut self, bytes: Vec<u8>) {
        let mut db = (*self.db).clone();
        db.load_font_data(bytes);
        self.db = std::sync::Arc::new(db);
        self.epoch = advance_font_env_epoch();
    }

    pub fn load_face(
        &self,
        family: &str,
        weight: f32,
        style: &str,
    ) -> Option<ttf_parser::Face<'static>> {
        let fontdb_style = match style {
            "italic" | "oblique" => fontdb::Style::Italic,
            _ => fontdb::Style::Normal,
        };
        let id = self.db.query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight(weight.round() as u16),
            style: fontdb_style,
            ..Default::default()
        })?;
        let data = leaked_face_data(self, id)?;
        ttf_parser::Face::parse(data, 0).ok()
    }

    /// Load the raw face bytes and face index for a queried font.
    pub fn load_face_data(
        &self,
        family: &str,
        weight: f32,
        style: &str,
    ) -> Option<(&'static [u8], u32)> {
        let fontdb_style = match style {
            "italic" | "oblique" => fontdb::Style::Italic,
            _ => fontdb::Style::Normal,
        };
        let id = self.db.query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight(weight.round() as u16),
            style: fontdb_style,
            ..Default::default()
        })?;
        let data = leaked_face_data(self, id)?;
        let face_index = self.db.face(id).map(|f| f.index).unwrap_or(0);
        Some((data, face_index))
    }
}

// ─────────────────────────────────────────────────────────────
// System glyph-fallback fonts
// ─────────────────────────────────────────────────────────────

/// Process-wide cache: character → first installed face covering it.
///
/// Scan order mirrors [`FontContext::font_for_glyphs`]: normal weight and
/// style, non-monospaced faces first. `None` caches a known-uncovered char so
/// repeated compiles skip the scan.
#[cfg(feature = "rich-text")]
fn char_face_cache() -> &'static std::sync::Mutex<HashMap<char, Option<fontdb::ID>>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<char, Option<fontdb::ID>>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Shared face data keyed by fontdb face id.
#[cfg(feature = "rich-text")]
type FaceData = Option<Arc<[u8]>>;
/// Process-wide cache: fontdb face id → shared face data.
#[cfg(feature = "rich-text")]
fn face_data_cache() -> &'static std::sync::Mutex<HashMap<fontdb::ID, FaceData>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<fontdb::ID, FaceData>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}
#[cfg(feature = "rich-text")]
/// Process-wide cache: fontdb face id → constructed Typst font.
fn typst_font_cache() -> &'static std::sync::Mutex<HashMap<fontdb::ID, Font>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<fontdb::ID, Font>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Process-wide cache: fontdb face id → `'static` view of the face bytes.
///
/// [`FontContext::load_face`] returns `Face<'static>` because its callers hold
/// the face across frames; `ttf_parser` borrows the buffer, so that buffer has
/// to live for the process. Memoizing the allocation per face id bounds it to
/// one per distinct face instead of one per call. The Typst path does not use
/// this at all — it shares `Arc<[u8]>` via [`cached_face_data`].
fn leaked_face_data(font_ctx: &FontContext, id: fontdb::ID) -> Option<&'static [u8]> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<fontdb::ID, &'static [u8]>>> =
        std::sync::OnceLock::new();
    let mut cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new())).lock().unwrap();
    if let Some(data) = cache.get(&id) {
        return Some(*data);
    }
    let data: &'static [u8] =
        Box::leak(FontContext::face_data(&font_ctx.db, id)?.into_boxed_slice());
    cache.insert(id, data);
    Some(data)
}

/// Shared copy of a system face's bytes (loaded once per process).
#[cfg(feature = "rich-text")]
fn cached_face_data(font_ctx: &FontContext, id: fontdb::ID) -> Option<Arc<[u8]>> {
    let mut cache = face_data_cache().lock().unwrap();
    if let Some(data) = cache.get(&id) {
        return data.clone();
    }
    let data = FontContext::face_data(&font_ctx.db, id).map(Arc::from);
    cache.insert(id, data.clone());
    data
}

/// Whether a parsed face can render `ch`.
#[cfg(feature = "rich-text")]
fn face_covers_char(data: &[u8], index: u32, ch: char) -> bool {
    ttf_parser::Face::parse(data, index)
        .map(|face| face.glyph_index(ch).is_some())
        .unwrap_or(false)
}

/// Find the first installed face covering `ch`, preferring normal weight and
/// style, non-monospaced faces. Result is cached per character.
#[cfg(feature = "rich-text")]
fn probe_char_face(font_ctx: &FontContext, ch: char) -> Option<fontdb::ID> {
    if let Some(cached) = char_face_cache().lock().unwrap().get(&ch) {
        return *cached;
    }
    let db = &font_ctx.db;
    let mut found: Option<fontdb::ID> = None;
    'passes: for prefer_plain in [true, false] {
        for face in db.faces() {
            let matches = if prefer_plain {
                face.style == fontdb::Style::Normal
                    && face.weight == fontdb::Weight::NORMAL
                    && !face.monospaced
            } else {
                face.style == fontdb::Style::Normal && !face.monospaced
            };
            if !matches {
                continue;
            }
            let Some(data) = cached_face_data(font_ctx, face.id) else {
                continue;
            };
            if face_covers_char(data.as_ref(), face.index, ch) {
                found = Some(face.id);
                break 'passes;
            }
        }
    }
    char_face_cache().lock().unwrap().insert(ch, found);
    found
}
#[cfg(feature = "rich-text")]
/// Construct (or fetch from cache) a Typst font for a system face.
fn cached_typst_font(font_ctx: &FontContext, id: fontdb::ID) -> Option<Font> {
    let mut cache = typst_font_cache().lock().unwrap();
    if let Some(font) = cache.get(&id) {
        return Some(font.clone());
    }
    let data = cached_face_data(font_ctx, id)?;
    let index = font_ctx.db.face(id)?.index;
    let font = Font::new(Bytes::new(data), index)?;
    cache.insert(id, font.clone());
    Some(font)
}
#[cfg(feature = "rich-text")]
/// Collect Typst fonts covering every character of `content` that the primary
/// family cannot render.
///
/// Gives the Typst text path glyph coverage for non-Latin content (CJK,
/// Cyrillic, ...) without requiring an explicit `font_family`. Bounded at 8
/// distinct fallback faces per compile; results are cached per face id.
pub(crate) fn collect_fallback_fonts(
    font_ctx: &FontContext,
    content: &str,
    primary_family: &str,
    primary_weight: f32,
    primary_style: &str,
) -> Vec<Font> {
    // Resolve the primary face through shared cached data instead of
    // `load_face`, which leaks its buffer on every call.
    let fontdb_style = match primary_style {
        "italic" | "oblique" => fontdb::Style::Italic,
        _ => fontdb::Style::Normal,
    };
    let primary_id = font_ctx.db.query(&fontdb::Query {
        families: &[fontdb::Family::Name(primary_family)],
        weight: fontdb::Weight(primary_weight.round() as u16),
        style: fontdb_style,
        ..Default::default()
    });
    let primary = primary_id.and_then(|id| {
        let index = font_ctx.db.face(id)?.index;
        let data = cached_face_data(font_ctx, id)?;
        Some((data, index))
    });

    let mut ids: Vec<fontdb::ID> = Vec::new();
    for ch in content.chars() {
        if ch.is_whitespace() {
            continue;
        }
        let covered = primary
            .as_ref()
            .map(|(data, index)| face_covers_char(data, *index, ch))
            .unwrap_or(false);
        if covered {
            continue;
        }
        if let Some(id) = probe_char_face(font_ctx, ch) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if ids.len() >= 8 {
            break;
        }
    }
    ids.iter().filter_map(|id| cached_typst_font(font_ctx, *id)).collect()
}

// ─────────────────────────────────────────────────────────────
// Font bundle
// ─────────────────────────────────────────────────────────────

/// Resolve the face that shapes `c`: the primary face first, then the other
/// A font entry in the bundled font set.
///
/// `weight`/`style` are the face's own CSS values, recorded so the plain-text
/// fast path can choose among a family's faces the way fontdb's query does for
/// system fonts. Without them the fast path could only ever take the first entry
/// of a family, which made `font_weight: 700` and `font_style: "italic"` no-ops
/// on bundled text.
struct BundledFont {
    family: &'static str,
    data: &'static [u8],
    weight: f32,
    style: &'static str,
}

/// The bundled face of `family` that answers a weight/style query, chosen the
/// way a font database chooses: style is a different shape and so decides first,
/// then the nearest available weight. A family that ships one face — every
/// family in a slim build — therefore gets that face for any query, which is
/// what it got before this existed.
fn bundled_face(family: &str, weight: f32, style: &str) -> Option<&'static BundledFont> {
    let want_italic = style == "italic" || style == "oblique";
    BUNDLED_FONTS.iter().filter(|bf| bf.family == family).min_by_key(|bf| {
        let has_italic = bf.style == "italic" || bf.style == "oblique";
        let style_cost = u32::from(has_italic != want_italic);
        let weight_cost = (bf.weight - weight).abs().round() as u32;
        (style_cost, weight_cost)
    })
}

/// Fonts embedded at compile time. Add new fonts here.
static BUNDLED_FONTS: &[BundledFont] = &[
    // Open Sans static faces (Apache-2.0; see assets/fonts/README.md for
    // provenance + SHA-256). These are what the plain-text fast path can use —
    // it shapes with `ttf-parser`, which reads `fvar` but never applies the
    // deltas — so the set has to cover the weights authors ask for by name, and
    // `bundled_face` chooses among them the way a font database does. The rich
    // path gets the variable pair as well (see `VARIABLE_FONTS`) and can answer
    // any weight in the axis range.
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Regular.ttf"),
        weight: 400.0,
        style: "normal",
    },
    // Noto Sans SC subset (SIL OFL 1.1; see assets/fonts/README.md for
    // provenance + SHA-256): the 3755 GB2312 level-1 common hanzi plus CJK
    // punctuation, fullwidth forms and ASCII, instanced to weight 400. Gives
    // the wasm sandbox (no system fonts) zero-config CJK coverage — before
    // this, CJK text shaped to nothing, every glyph silently skipped.
    // NOT gated on rich-text: in the slim build the fast path is the only
    // shaper, and its per-character fallback (pick_glyph) needs this face.
    BundledFont {
        family: "Noto Sans SC",
        data: include_bytes!("../assets/fonts/NotoSansSC-Common.ttf"),
        weight: 400.0,
        style: "normal",
    },
    // Not gated: `font_weight: "bold"` is asked for by plain-Latin titles that
    // run on the slim embed (every `web/demos/epicycles/*` scene), and the fast
    // path can only answer it if the face is in the binary. Costs ~220 KB. The
    // two italic faces stay gated: the only italic in the corpus is
    // `examples/layout/27_layout_text.amx`, which no site page embeds — on a
    // slim build that line draws regular, and the desktop build draws italic.
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Bold.ttf"),
        weight: 700.0,
        style: "normal",
    },
    // Gated because the plain fast path cannot reach it in a slim build, which
    // ships Regular alone for this family; `bundled_face` then answers every
    // weight and style query with that one face.
    #[cfg(feature = "rich-text")]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Italic.ttf"),
        weight: 400.0,
        style: "italic",
    },
    // Gated because the plain fast path cannot reach it in a slim build, which
    // ships Regular alone for this family; `bundled_face` then answers every
    // weight and style query with that one face.
    #[cfg(feature = "rich-text")]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-BoldItalic.ttf"),
        weight: 700.0,
        style: "italic",
    },
    // Gated because the plain fast path cannot reach it in a slim build, which
    // ships Regular alone for this family; `bundled_face` then answers every
    // weight and style query with that one face.
    #[cfg(feature = "rich-text")]
    BundledFont {
        family: "Fira Math",
        data: include_bytes!("../assets/fonts/FiraMath-Regular.otf"),
        weight: 400.0,
        style: "normal",
    },
];

/// Default font family used when the requested one is not available.
pub const DEFAULT_FONT_FAMILY: &str = "Open Sans";
/// Default math font family used for math rendering.
pub const DEFAULT_MATH_FONT_FAMILY: &str = "Fira Math";
/// The weight a caller that never says otherwise compiles at.
pub const DEFAULT_FONT_WEIGHT: f32 = 400.0;
/// The Open Sans variable pair, registered into the Typst font book *in
/// addition to* the static faces — but only for a weight the statics cannot
/// answer (see `wants_variable_faces`). Typst reads the `wght` axis off these
/// faces and instances it during shaping (`Font::instantiate` →
/// `FontVariations::resolve`), and its face selection scores an axis-capable
/// face at distance 0 for any in-range request, so registering them is enough to
/// make `font_weight: 625` draw 625 rather than the nearest keyword.
///
/// They are deliberately absent from [`BUNDLED_FONTS`]: the plain-text fast path
/// shapes with `ttf-parser`, which reads `fvar`/`gvar` but never applies the
/// deltas to outlines, so a variable face there would render only its default
/// instance — the statics are what that path needs.
/// The parsed variable pair, built once per process.
///
/// `build_world` runs per text compile, and parsing a 530 KB variable face every
/// time showed up as +8.3% on `scrub_layout_scene_100frames` — a cost the static
/// faces already pay, but which adding two more faces made worth fixing rather
/// than absorbing. `typst::Font` is refcounted internally, so the clone per
/// world is an `Arc` bump; the `FontInfo` still gets cloned into the book because
/// the book owns its entries.
#[cfg(feature = "rich-text")]
static VARIABLE_FONT_FACES: std::sync::LazyLock<Vec<Font>> = std::sync::LazyLock::new(|| {
    VARIABLE_FONTS
        .iter()
        .filter_map(|bf| {
            let font = Font::new(Bytes::new(bf.data), 0);
            if font.is_none() {
                tracing::error!(
                    "variable face {} failed to parse; continuous font_weight falls back to the static faces",
                    bf.family
                );
            }
            font
        })
        .collect()
});

#[cfg(feature = "rich-text")]
static VARIABLE_FONTS: &[BundledFont] = &[
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Variable.ttf"),
        weight: 400.0,
        style: "normal",
    },
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Italic-Variable.ttf"),
        weight: 400.0,
        style: "italic",
    },
];

/// Build a TypstWorld with bundled fonts + any requested system fonts loaded.
#[cfg(feature = "rich-text")]
fn build_world(
    source: Source,
    extra_fonts: &[&str],
    fallback_fonts: &[Font],
    font_ctx: &FontContext,
) -> Result<TypstWorld, RenderError> {
    let mut fonts = Vec::with_capacity(BUNDLED_FONTS.len() + extra_fonts.len());
    let mut book = FontBook::new();

    // Load bundled fonts
    for bf in BUNDLED_FONTS {
        let font = Font::new(Bytes::new(bf.data), 0).ok_or_else(|| {
            RenderError::TextCompilation(format!("Failed to load bundled font: {}", bf.family))
        })?;
        book.push(font.info().clone());
        fonts.push(font);
    }

    // The variable pair, always. Serving only the weights the statics have faces
    // for is tempting and wrong: measured on the compiled-ink ramp, a face swap
    // at each canonical value makes the axis non-monotone — `font_weight: 450`
    // drew *heavier* (516.3 px) than `500` (508.8 px), and 599 / 600 / 601 came
    // out 533.2 / 553.8 / 533.5. Typst scores an axis-capable face at distance 0
    // for any in-range request (`FontBook::distance`), so registering these faces
    // makes it the one that answers, and the ramp becomes monotone; the statics
    // stay in the book as the fallback the request would land on anyway.
    for font in VARIABLE_FONT_FACES.iter() {
        book.push(font.info().clone());
        fonts.push(font.clone());
    }

    // Load requested extra fonts via persistent FontContext.
    // Skip fonts that are already available as bundled fonts to avoid
    // override with potentially different metrics (e.g. system vs mock).
    // Load the full emphasis set (regular/bold/italic/bold-italic) so Typst
    // can honor `*bold*`/`_italic_`/`font_weight`/`font_style` for the family.
    for family in extra_fonts {
        if fonts.iter().any(|f| f.info().family == *family) {
            continue;
        }
        for font in font_ctx.load_font_emphasis_faces(family) {
            if fonts.iter().any(|f| {
                f.info().family == font.info().family
                    && f.info().variant.weight == font.info().variant.weight
                    && f.info().variant.style == font.info().variant.style
            }) {
                continue;
            }
            book.push(font.info().clone());
            fonts.push(font);
        }
    }

    // Append glyph-fallback faces covering characters the requested families
    // cannot render (CJK, Cyrillic, ...). One representative face per family.
    for font in fallback_fonts {
        if fonts.iter().any(|f| f.info().family == font.info().family) {
            continue;
        }
        book.push(font.info().clone());
        fonts.push(font.clone());
    }

    let library = typst::Library::builder().build();
    Ok(TypstWorld {
        source,
        fonts,
        book: LazyHash::new(book),
        library: LazyHash::new(library),
    })
}

/// Resolve a font family name to the family string that should appear in Typst markup.
/// Searches bundled fonts first, then system fonts (via FontContext).
/// Falls back to `DEFAULT_FONT_FAMILY` if not found anywhere.
pub fn resolve_font_family(requested: &str, font_ctx: &FontContext) -> String {
    if requested.is_empty() {
        return DEFAULT_FONT_FAMILY.to_string();
    }

    // 1. Check bundled fonts (exact case-insensitive match)
    for bf in BUNDLED_FONTS {
        if bf.family.eq_ignore_ascii_case(requested) {
            return bf.family.to_string();
        }
    }

    // 2. Check system fonts via FontContext
    if font_ctx.has_family(requested) {
        return requested.to_string();
    }

    DEFAULT_FONT_FAMILY.to_string()
}

/// Return a sorted list of all available font family names (bundled + system).
pub fn available_font_families(font_ctx: &FontContext) -> Vec<String> {
    let mut families: Vec<String> = BUNDLED_FONTS.iter().map(|bf| bf.family.to_string()).collect();
    families.extend(font_ctx.families());
    families.sort();
    families.dedup();
    families
}

struct PathBuilder(BezPath);

impl ttf_parser::OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(Point::new(x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(Point::new(x as f64, y as f64));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(Point::new(x1 as f64, y1 as f64), Point::new(x as f64, y as f64));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.curve_to(
            Point::new(x1 as f64, y1 as f64),
            Point::new(x2 as f64, y2 as f64),
            Point::new(x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
#[cfg(feature = "rich-text")]
/// A shape extracted from a Typst frame, containing the curve and its transform.
#[derive(Clone)]
pub struct ExtractedShape {
    /// The curve geometry of the shape.
    pub curve: typst::visualize::Curve,
    /// The transform applied to the shape.
    pub transform: Transform,
}
#[cfg(feature = "rich-text")]
/// A Typst world implementation for compiling text and math.
pub struct TypstWorld {
    /// The source document.
    source: Source,
    /// Loaded fonts.
    fonts: Vec<Font>,
    /// Font book mapping indices to font info.
    book: LazyHash<FontBook>,
    /// Typst standard library.
    library: LazyHash<Library>,
}
#[cfg(feature = "rich-text")]
impl TypstWorld {
    /// Create a new Typst world with the given source and font context.
    pub fn new(source: Source, font_ctx: &FontContext) -> Result<Self, RenderError> {
        build_world(source, &[], &[], font_ctx)
    }

    /// Create a new Typst world with additional font families.
    pub fn with_fonts(
        source: Source,
        fonts: &[&str],
        font_ctx: &FontContext,
    ) -> Result<Self, RenderError> {
        Self::with_fonts_and_fallback(source, fonts, &[], font_ctx)
    }

    /// Create a new Typst world with additional font families plus
    /// pre-collected glyph-fallback fonts (see `collect_fallback_fonts`).
    pub fn with_fonts_and_fallback(
        source: Source,
        fonts: &[&str],
        fallback_fonts: &[Font],
        font_ctx: &FontContext,
    ) -> Result<Self, RenderError> {
        build_world(source, fonts, fallback_fonts, font_ctx)
    }
}
#[cfg(feature = "rich-text")]
impl World for TypstWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> typst::diag::FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(typst::diag::FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        Err(typst::diag::FileError::NotFound(id.vpath().get_without_slash().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}
#[cfg(feature = "rich-text")]
/// Compile Typst math markup into a frame.
///
/// Build a Typst `#set text(...)` rule string from the given typography parameters.
fn typst_text_set_rules(
    font_weight: f32,
    font_style: &str,
    letter_spacing: f32,
    word_spacing: f32,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Weight: Typst accepts a CSS keyword *or* an integer. The number goes
    // through as authored, because the keyword form is a nine-value ladder and
    // mapping to it threw away every weight between the rungs — which is what
    // made `font_weight` un-animatable even where a face could answer it.
    if (font_weight - DEFAULT_FONT_WEIGHT).abs() > f32::EPSILON {
        parts.push(format!("weight: {}", font_weight.round() as i32));
    }

    // Style
    if font_style == "italic" {
        parts.push("style: \"italic\"".to_string());
    }

    // Letter spacing (tracking)
    if letter_spacing != 0.0 {
        parts.push(format!("tracking: {}pt", letter_spacing));
    }

    // Word spacing
    if word_spacing != 0.0 {
        parts.push(format!("spacing: {}pt", word_spacing));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("#set text({}); ", parts.join(", "))
    }
}
#[cfg(feature = "rich-text")]
/// Build a Typst `#set par(leading: ...)` rule for line height.
fn typst_par_leading_rule(line_height: f32) -> String {
    if (line_height - 1.2).abs() < f32::EPSILON {
        return String::new();
    }
    let leading_em = line_height - 1.0;
    if leading_em.abs() < 0.001 {
        return String::new();
    }
    format!("#set par(leading: {}em); ", leading_em)
}
#[cfg(feature = "rich-text")]
/// Build a `#set page` rule sized to the content.
///
/// Without an explicit page rule Typst lays text out on its default A4 page
/// (~453pt of usable width after margins), which silently wraps any line
/// wider than that regardless of scene resolution. When `max_width` is set
/// the page matches it so the wrapping preamble controls line breaks;
/// otherwise the page is sized generously enough to keep the whole content on
/// one line (worst case one em of advance per character).
fn typst_page_preamble(
    max_width: f32,
    content: &str,
    font_size: f32,
    letter_spacing: f32,
) -> String {
    let width = if max_width > 0.0 {
        max_width
    } else {
        (content.chars().count() as f32 + 1.0) * (font_size + letter_spacing.abs()) + 64.0
    };
    format!("#set page(width: {width:.2}pt, height: auto, margin: 0pt)\n")
}

/// Build a Typst wrapping preamble string for max_width, text_align, and overflow.
#[cfg(feature = "rich-text")]
fn typst_wrapping_preamble(
    max_width: f32,
    text_align: &str,
    overflow: &str,
    inner_content: &str,
) -> String {
    let mut inner = String::new();

    // Text alignment
    match text_align {
        "center" => inner.push_str("#set align(center); "),
        "right" => inner.push_str("#set align(right); "),
        "justify" => inner.push_str("#set align(justify); "),
        _ => {}, // "left" is default
    }

    inner.push_str(inner_content);

    if max_width > 0.0 {
        let clip_arg = if overflow == "clip" {
            ", clip: true"
        } else {
            ""
        };
        // Wrap in a block with the given width
        format!("#block(width: {}pt, inset: 0pt{})[\n{}]", max_width, clip_arg, inner)
    } else {
        inner
    }
}
/// Parse a font weight value (numeric or string alias) to f32.
pub fn parse_font_weight(value: &str) -> f32 {
    match value {
        "thin" => 100.0,
        "extralight" => 200.0,
        "light" => 300.0,
        "normal" => 400.0,
        "regular" => 400.0,
        "medium" => 500.0,
        "semibold" => 600.0,
        "bold" => 700.0,
        "extrabold" => 800.0,
        "black" => 900.0,
        _ => {
            // Try to parse as number
            value.parse::<f32>().unwrap_or(400.0)
        },
    }
}
#[cfg(feature = "rich-text")]
/// Compile a math expression string (Typst math syntax) into a rendered frame.
///
/// Uses Typst's layout engine to parse and render the math expression, returning
/// a [`Frame`] containing the positioned glyphs ready for Vello scene assembly.
pub fn compile_math(
    math: &str,
    font_size: f32,
    color: typst::visualize::Color,
    font_family: &str,
    font_ctx: &FontContext,
    max_width: f32,
    text_align: &str,
    overflow: &str,
) -> Result<Frame, RenderError> {
    let text_font = resolve_font_family(font_family, font_ctx);
    let fallback = collect_fallback_fonts(font_ctx, math, &text_font, 400.0, "normal");
    let page_preamble = typst_page_preamble(max_width, math, font_size, 0.0);
    let base_markup = format!(
        "#set text(size: {}pt, fill: rgb(\"{}\"), font: (\"{}\", \"Fira Math\")); #show math.equation: set text(font: \"Fira Math\"); $ {} $",
        font_size,
        color.to_hex(),
        text_font,
        math
    );

    let markup = format!(
        "{}{}",
        page_preamble,
        typst_wrapping_preamble(max_width, text_align, overflow, &base_markup)
    );

    let vpath = VirtualPath::new("main.typ")
        .map_err(|e| RenderError::TextCompilation(format!("invalid virtual path: {e}")))?;
    let source = Source::new(RootedPath::new(VirtualRoot::Project, vpath).intern(), markup);
    let world = TypstWorld::with_fonts_and_fallback(
        source,
        &[&text_font, DEFAULT_MATH_FONT_FAMILY],
        &fallback,
        font_ctx,
    )?;
    let document: typst_layout::PagedDocument = typst::compile(&world).output.map_err(|e| {
        RenderError::TextCompilation(format!("failed to compile Typst math document: {e:?}"))
    })?;

    Ok(document.pages()[0].frame.clone())
}
#[cfg(feature = "rich-text")]
/// Compile Typst markup into a frame.
pub fn compile_typst(
    typst_markup: &str,
    font_size: f32,
    color: typst::visualize::Color,
    font_family: &str,
    font_ctx: &FontContext,
    font_weight: f32,
    font_style: &str,
    line_height: f32,
    letter_spacing: f32,
    word_spacing: f32,
    max_width: f32,
    text_align: &str,
    overflow: &str,
) -> Result<Frame, RenderError> {
    let font = resolve_font_family(font_family, font_ctx);
    let fallback = collect_fallback_fonts(font_ctx, typst_markup, &font, font_weight, font_style);
    let extra_rules = typst_text_set_rules(font_weight, font_style, letter_spacing, word_spacing);
    let leading_rule = typst_par_leading_rule(line_height);
    // Include math font so that $...$ math expressions compile correctly.
    // Mirror the compile_math show-rule for math.equation font.
    let page_preamble = typst_page_preamble(max_width, typst_markup, font_size, letter_spacing);
    let base_markup = format!(
        "{}{}#set text(size: {}pt, fill: rgb(\"{}\"), font: (\"{}\", \"{}\")); #show math.equation: set text(font: \"{}\")\n{}",
        extra_rules,
        leading_rule,
        font_size,
        color.to_hex(),
        font,
        DEFAULT_MATH_FONT_FAMILY,
        DEFAULT_MATH_FONT_FAMILY,
        typst_markup
    );
    // The page rule must stay at the document top level; only the content is
    // wrapped in the width-constrained block.
    let markup = format!(
        "{}{}",
        page_preamble,
        typst_wrapping_preamble(max_width, text_align, overflow, &base_markup)
    );

    let vpath = VirtualPath::new("main.typ")
        .map_err(|e| RenderError::TextCompilation(format!("invalid virtual path: {e}")))?;
    let source = Source::new(RootedPath::new(VirtualRoot::Project, vpath).intern(), markup);
    let world = TypstWorld::with_fonts_and_fallback(
        source,
        &[&font, DEFAULT_MATH_FONT_FAMILY],
        &fallback,
        font_ctx,
    )?;
    let document: typst_layout::PagedDocument = typst::compile(&world).output.map_err(|e| {
        RenderError::TextCompilation(format!("failed to compile Typst document: {e:?}"))
    })?;

    Ok(document.pages()[0].frame.clone())
}
#[cfg(feature = "rich-text")]
/// Compile plain text into a Typst frame.
pub fn compile_text(
    text: &str,
    font_size: f32,
    color: typst::visualize::Color,
    font_family: &str,
    font_ctx: &FontContext,
    font_weight: f32,
    font_style: &str,
    line_height: f32,
    letter_spacing: f32,
    word_spacing: f32,
    max_width: f32,
    text_align: &str,
    overflow: &str,
) -> Result<Frame, RenderError> {
    let font = resolve_font_family(font_family, font_ctx);
    let fallback = collect_fallback_fonts(font_ctx, text, &font, font_weight, font_style);
    let extra_rules = typst_text_set_rules(font_weight, font_style, letter_spacing, word_spacing);
    let leading_rule = typst_par_leading_rule(line_height);
    // Use Typst raw block (4 backticks) to avoid markup interpretation of user text.
    // 4-backtick delimiter handles text containing up to 3 consecutive backticks.
    // Block raw also handles newlines, which inline raw cannot.
    let escaped = text.replace('\\', "\\\\");
    let page_preamble = typst_page_preamble(max_width, text, font_size, letter_spacing);
    let base_markup = format!(
        "{}{}#set text(size: {}pt, fill: rgb(\"{}\"), font: \"{}\")\n````\n{}````",
        extra_rules,
        leading_rule,
        font_size,
        color.to_hex(),
        font,
        escaped
    );
    // The page rule must stay at the document top level; only the content is
    // wrapped in the width-constrained block.
    let markup = format!(
        "{}{}",
        page_preamble,
        typst_wrapping_preamble(max_width, text_align, overflow, &base_markup)
    );

    let vpath = VirtualPath::new("main.typ")
        .map_err(|e| RenderError::TextCompilation(format!("invalid virtual path: {e}")))?;
    let source = Source::new(RootedPath::new(VirtualRoot::Project, vpath).intern(), markup);
    let world = TypstWorld::with_fonts_and_fallback(source, &[&font], &fallback, font_ctx)?;
    let document: typst_layout::PagedDocument = typst::compile(&world).output.map_err(|_| {
        RenderError::TextCompilation("failed to compile Typst text document".to_string())
    })?;

    Ok(document.pages()[0].frame.clone())
}

/// Typst's `raw` show rule renders code at `0.8em` of the surrounding text
/// size. The span-based highlight path reproduces that scale so a `Code` actor
/// is the same size whether or not `language` is set.
#[cfg(feature = "rich-text")]
const RAW_TEXT_EM_SCALE: f32 = 0.8;
#[cfg(feature = "rich-text")]
/// A plain (un-highlighted) Typst raw block: four backticks so the code body is
/// taken verbatim (no markup interpretation, no escaping beyond backslashes).
fn plain_code_fence(code: &str) -> String {
    let escaped = code.replace('\\', "\\\\");
    format!("````\n{escaped}````")
}

/// A role → RGBA palette that recolours syntax-highlighted `Code` tokens so
/// they follow the active colorscheme instead of Typst's fixed raw theme.
///
/// Each field corresponds to a role in Typst's built-in `RAW_THEME`;
/// [`HighlightPalette::resolve`] maps a syntect foreground colour back to its
/// role. The engine builds one per colorscheme at build time (see
/// `ResolvedColorscheme::highlight_palette`) and hands it to the compiler, so
/// — like every other primitive's colour — the highlight path only ever sees
/// concrete RGBA, never the scheme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HighlightPalette {
    /// Keywords and storage modifiers (Typst theme `#d73948`).
    pub keyword: [f32; 4],
    /// Function / entity names and macros (`#4b69c6`, `#16718d`).
    pub function: [f32; 4],
    /// Strings and inserted-diff text (`#198810`).
    pub string: [f32; 4],
    /// Constants and numbers (`#b60157`).
    pub number: [f32; 4],
    /// Comments and raw markup (`#74747c`, `#6b6b6f`).
    pub comment: [f32; 4],
    /// Interpolation and other-name scopes (`#8b41b1`).
    pub interpolation: [f32; 4],
    /// Escapes, math operators and labels (`#1d6c76`).
    pub escape: [f32; 4],
    /// Meta annotations (`#301414`).
    pub annotation: [f32; 4],
}

impl HighlightPalette {
    #[cfg(feature = "rich-text")]
    /// Map a syntect theme foreground colour to its palette colour. Colours not
    /// in Typst's fixed theme are returned unchanged (normalised to 0..1 RGBA),
    /// so an unexpected theme never drops a token.
    pub fn resolve(&self, color: &syntect::highlighting::Color) -> [f32; 4] {
        match (color.r, color.g, color.b) {
            (0xd7, 0x39, 0x48) => self.keyword,
            (0x4b, 0x69, 0xc6) | (0x16, 0x71, 0x8d) => self.function,
            (0x19, 0x88, 0x10) => self.string,
            (0xb6, 0x01, 0x57) => self.number,
            (0x74, 0x74, 0x7c) | (0x6b, 0x6b, 0x6f) => self.comment,
            (0x8b, 0x41, 0xb1) => self.interpolation,
            (0x1d, 0x6c, 0x76) => self.escape,
            (0x30, 0x14, 0x14) => self.annotation,
            _ => [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                1.0,
            ],
        }
    }

    /// A stable, hashable fingerprint of this palette for the text cache key
    /// (two colorschemes must not share a highlighted compile entry).
    fn cache_bits(&self) -> Vec<u32> {
        [
            self.keyword,
            self.function,
            self.string,
            self.number,
            self.comment,
            self.interpolation,
            self.escape,
            self.annotation,
        ]
        .into_iter()
        .flat_map(|c| c.map(f32::to_bits))
        .collect()
    }
}
#[cfg(feature = "rich-text")]
/// Format a 0..1 RGBA colour as a Typst `rgb(...)` call (tokens are opaque).
fn typst_rgb(color: [f32; 4]) -> String {
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("rgb({}, {}, {})", channel(color[0]), channel(color[1]), channel(color[2]))
}
#[cfg(feature = "rich-text")]
/// Build Typst markup that highlights `code` as `language`, using Typst's own
/// `raw` syntax set (`RAW_SYNTAXES`) and theme (`RAW_THEME`) so the colours
/// match what a `#set raw(theme: auto)` fence would produce.
///
/// Returns `None` when the language has no syntax definition (the caller then
/// falls back to a plain fence).
///
/// Two deliberate departures from a raw fence, both to defeat Typst's inline
/// line builder, which drops whitespace-only inline elements (it treats a
/// styled-run boundary as a line-break opportunity and trims the space):
/// - whitespace-only pieces are merged into their neighbouring token, so every space lives *inside*
///   a non-empty `#text` element and survives;
/// - each source line is emitted as its own run of `#text` spans joined by a forced line break
///   (`\`), because a bare newline in markup collapses to a space.
///
/// Tokens whose colour equals the theme's base foreground are emitted without a
/// `fill`, so they inherit the actor's own `color` (set by the caller's
/// `#set text`). Only the string metacharacters `\` and `"` are escaped — the
/// token text never reaches Typst as markup.
fn highlighted_code_markup(
    code: &str,
    language: &str,
    palette: Option<&HighlightPalette>,
) -> Option<String> {
    use std::mem;

    use syntect::easy::HighlightLines;
    use syntect::highlighting::Color;
    use syntect::util::LinesWithEndings;

    let syntaxes = &*typst::text::RAW_SYNTAXES;
    let syntax = syntaxes.find_syntax_by_token(language)?;
    let theme = &*typst::text::RAW_THEME;
    let base_fg = theme.settings.foreground;
    let mut highlighter = HighlightLines::new(syntax, theme);

    let lines: Vec<&str> = LinesWithEndings::from(code).collect();
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        let line = line.strip_suffix('\n').unwrap_or(line);
        let ranges = highlighter.highlight_line(line, syntaxes).ok()?;

        // Merge whitespace-only pieces into the previous token (or the next,
        // for leading whitespace) so no standalone whitespace element survives
        // to be trimmed by the line builder.
        let mut merged: Vec<(String, Option<Color>)> = Vec::new();
        let mut pending = String::new();
        for (style, piece) in ranges {
            if piece.is_empty() {
                continue;
            }
            if piece.trim().is_empty() {
                match merged.last_mut() {
                    Some(last) => last.0.push_str(piece),
                    None => pending.push_str(piece),
                }
                continue;
            }
            let mut text = mem::take(&mut pending);
            text.push_str(piece);
            let color = if Some(style.foreground) == base_fg {
                None
            } else {
                Some(style.foreground)
            };
            merged.push((text, color));
        }
        if !pending.is_empty() {
            match merged.last_mut() {
                Some(last) => last.0.push_str(&pending),
                None => merged.push((pending.clone(), None)),
            }
        }
        // A blank line still needs to occupy its row.
        if merged.is_empty() {
            merged.push((" ".to_string(), None));
        }

        for (text, color) in merged {
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
            match color {
                Some(c) => {
                    let fill = match palette {
                        Some(palette) => palette.resolve(&c),
                        None => [
                            c.r as f32 / 255.0,
                            c.g as f32 / 255.0,
                            c.b as f32 / 255.0,
                            1.0,
                        ],
                    };
                    out.push_str(&format!("#text(fill: {}, \"{escaped}\")", typst_rgb(fill)));
                },
                None => out.push_str(&format!("#text(\"{escaped}\")")),
            }
        }
        if index + 1 < lines.len() {
            out.push_str("\\\n");
        }
    }
    Some(out)
}
#[cfg(feature = "rich-text")]
/// Compile code text into a Typst frame.
///
/// A non-empty `language` that Typst recognizes turns on syntax highlighting
/// (Typst's built-in `raw` theme colors the tokens; un-highlighted text keeps
/// the actor's own `color`). Empty, or unknown after a warning, renders plain.
///
/// When `palette` is `Some`, token colours are remapped through it so the
/// highlight follows the active colorscheme; `None` keeps Typst's fixed theme.
pub fn compile_code(
    code: &str,
    language: &str,
    palette: Option<&HighlightPalette>,
    font_size: f32,
    color: typst::visualize::Color,
    font_family: &str,
    font_ctx: &FontContext,
    font_weight: f32,
    font_style: &str,
    line_height: f32,
    letter_spacing: f32,
    word_spacing: f32,
    max_width: f32,
    text_align: &str,
    overflow: &str,
) -> Result<Frame, RenderError> {
    let font = resolve_font_family(font_family, font_ctx);
    let fallback = collect_fallback_fonts(font_ctx, code, &font, font_weight, font_style);
    let extra_rules = typst_text_set_rules(font_weight, font_style, letter_spacing, word_spacing);
    let leading_rule = typst_par_leading_rule(line_height);
    // Highlighted code is emitted as `#text` spans (see `highlighted_code_markup`);
    // plain code uses a Typst raw block (4 backticks) to avoid markup
    // interpretation of the code text.
    //
    // Typst's `raw` show rule renders at `0.8em` of the set text size. The span
    // path has no such show rule, so it sets the size to the same `0.8em`
    // effective value — otherwise toggling `language` would resize the block.
    let highlighted_markup = if language.is_empty() {
        None
    } else {
        highlighted_code_markup(code, language, palette)
    };
    if !language.is_empty() && highlighted_markup.is_none() {
        tracing::warn!(
            "Code: unknown 'language' {language:?}; rendering without syntax highlighting \
             (only Typst's built-in raw languages are supported)"
        );
    }
    let (content, set_size) = match highlighted_markup {
        Some(markup) => (markup, font_size * RAW_TEXT_EM_SCALE),
        None => (plain_code_fence(code), font_size),
    };
    let page_preamble = typst_page_preamble(max_width, code, set_size, letter_spacing);
    let base_markup = format!(
        "{}{}#set text(size: {}pt, fill: rgb(\"{}\"), font: \"{}\")\n{content}",
        extra_rules,
        leading_rule,
        set_size,
        color.to_hex(),
        font,
    );
    // The page rule must stay at the document top level; only the content is
    // wrapped in the width-constrained block.
    let markup = format!(
        "{}{}",
        page_preamble,
        typst_wrapping_preamble(max_width, text_align, overflow, &base_markup)
    );

    let vpath = VirtualPath::new("main.typ")
        .map_err(|e| RenderError::TextCompilation(format!("invalid virtual path: {e}")))?;
    let source = Source::new(RootedPath::new(VirtualRoot::Project, vpath).intern(), markup);
    let world = TypstWorld::with_fonts_and_fallback(source, &[&font], &fallback, font_ctx)?;
    let document: typst_layout::PagedDocument = typst::compile(&world).output.map_err(|_| {
        RenderError::TextCompilation("failed to compile Typst code document".to_string())
    })?;

    Ok(document.pages()[0].frame.clone())
}
#[cfg(feature = "rich-text")]
/// Extract font ascent and descent from a Typst frame (uses the first text item found).
/// Returns (ascent, descent) in scene units (points).
pub fn extract_frame_metrics(frame: &Frame) -> (f32, f32) {
    // Walk the frame to find the first text item with a usable font
    let mut result: Option<(f32, f32)> = None;
    let mut stack: Vec<(&Frame, Transform)> = vec![(frame, Transform::identity())];

    while let Some((current, current_transform)) = stack.pop() {
        for (_pos, item) in current.items() {
            match item {
                FrameItem::Group(group) => {
                    let group_transform = current_transform.pre_concat(group.transform);
                    stack.push((&group.frame, group_transform));
                },
                FrameItem::Text(text) => {
                    let size = text.size.to_pt() as f32;
                    let face = text.font.ttf();
                    let units_per_em = text.font.units_per_em() as f32;
                    let font_scale = size / units_per_em;
                    let ascent = face.ascender() as f32 * font_scale;
                    let descent = face.descender() as f32 * font_scale;
                    result = Some((ascent, descent));
                    break;
                },
                _ => {},
            }
        }
        if result.is_some() {
            break;
        }
    }

    result.unwrap_or((0.0, 0.0))
}
#[cfg(feature = "rich-text")]
/// Extract glyph paths from a Typst frame.
pub fn extract_glyphs(frame: &Frame) -> Vec<TextPath> {
    let mut glyphs = Vec::new();
    walk_frame_for_glyphs(frame, Transform::identity(), &mut glyphs);
    let _ = center_text_paths(&mut glyphs);
    glyphs
}
#[cfg(feature = "rich-text")]
/// Extract glyph paths and font metrics from a Typst frame.
pub fn extract_glyphs_with_metrics(frame: &Frame) -> CompiledText {
    let mut glyphs = Vec::new();
    walk_frame_for_glyphs(frame, Transform::identity(), &mut glyphs);

    // Compute bounding box BEFORE centering
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for glyph in &glyphs {
        let bounds = glyph.path.bounding_box();
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    let baseline_offset = center_text_paths(&mut glyphs);

    let (ascent, descent) = extract_frame_metrics(frame);

    CompiledText {
        glyphs,
        ascent,
        descent,
        baseline_offset,
        hardware_layout: None,
    }
}
#[cfg(feature = "rich-text")]
/// Extract glyphs from a Typst frame, grouped by top-level `FrameItem::Group`.
///
/// Each `#box()[content]` wrapper in Typst produces a top-level `Group` in the
/// output frame.  This function returns:
/// - A flat list of glyph paths (centred as a whole, not per-group).
/// - A parallel list of index ranges, one per top-level group encountered.
///
/// Non-group text items at the top level are collected into an implicit group
/// appended at the end (only when non-empty).
pub fn extract_glyphs_grouped(frame: &Frame) -> (Vec<TextPath>, Vec<std::ops::Range<usize>>) {
    let mut all_glyphs: Vec<TextPath> = Vec::new();
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut state = GlyphLayoutState::default();

    for (pos, item) in frame.items() {
        let transform = Transform::translate(pos.x, pos.y);
        match item {
            FrameItem::Group(group) => {
                let start = all_glyphs.len();
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_glyphs_with_state(
                    &group.frame,
                    group_transform,
                    &mut all_glyphs,
                    &mut state,
                );
                let end = all_glyphs.len();
                if end > start {
                    ranges.push(start..end);
                }
            },
            FrameItem::Text(_) => {
                // Top-level text not wrapped in a group — collect into an implicit group
                let start = all_glyphs.len();
                walk_frame_for_glyphs_text_item(item, transform, &mut all_glyphs, &mut state);
                let end = all_glyphs.len();
                if end > start {
                    ranges.push(start..end);
                }
            },
            _ => {},
        }
    }

    // Centre all glyphs as a whole (not per-group)
    center_text_paths(&mut all_glyphs);
    (all_glyphs, ranges)
}
#[cfg(feature = "rich-text")]
/// Helper: extract glyphs from a single `FrameItem::Text` (not recursing into groups).
fn walk_frame_for_glyphs_text_item(
    item: &FrameItem,
    transform: Transform,
    glyphs: &mut Vec<TextPath>,
    state: &mut GlyphLayoutState,
) {
    if let FrameItem::Text(text) = item {
        let current_y = transform.ty.to_pt();
        if let Some(last_y) = state.last_y {
            if (current_y - last_y).abs() > 2.0 {
                state.line_idx += 1;
                state.last_y = Some(current_y);
                state.last_end = 0;
            }
        } else {
            state.last_y = Some(current_y);
        }

        let text_str = text.text.as_str();
        let size = text.size.to_pt() as f32;
        let units_per_em = text.font.units_per_em() as f32;
        let font_scale = size / units_per_em;
        let face = text.font.ttf();

        let mut item_last_end = 0usize;
        let mut x_curr = 0.0;
        for glyph in &text.glyphs {
            let start = glyph.range.start as usize;
            let end = glyph.range.end as usize;
            let is_space = text_str
                .get(start..end)
                .map(|s| s.chars().all(|c| c.is_whitespace()))
                .unwrap_or(false);
            let is_cjk =
                text_str.get(start..end).map(|s| s.chars().any(is_cjk_char)).unwrap_or(false);
            if is_space
                || is_cjk
                || (start > item_last_end
                    && text_str
                        .get(item_last_end..start)
                        .map(|s| s.chars().any(|c| c.is_whitespace() || is_cjk_char(c)))
                        .unwrap_or(false))
            {
                state.word_idx += 1;
            }
            item_last_end = end;

            let offset_x = glyph.x_offset.at(text.size).to_pt() as f32;
            let offset_y = glyph.y_offset.at(text.size).to_pt() as f32;
            let advance = glyph.x_advance.at(text.size).to_pt() as f32;

            let mut builder = PathBuilder(BezPath::new());
            if let Some(_bounds) = face.outline_glyph(ttf_parser::GlyphId(glyph.id), &mut builder) {
                let path = builder.0;
                let scale_affine = Affine::scale_non_uniform(font_scale as f64, -font_scale as f64);
                let glyph_translate = Affine::translate(kurbo::Vec2::new(
                    (x_curr + offset_x) as f64,
                    offset_y as f64,
                ));
                let item_transform = Affine::new([
                    transform.sx.get(),
                    transform.ky.get(),
                    transform.kx.get(),
                    transform.sy.get(),
                    transform.tx.to_pt(),
                    transform.ty.to_pt(),
                ]);
                let final_affine = item_transform * glyph_translate * scale_affine;
                let mut final_path = path;
                final_path.apply_affine(final_affine);
                let char_idx = glyphs.len() as u16;
                let mut tp = TextPath::new(final_path, paint_to_rgba(&text.fill), 1.0);
                tp.line_idx = state.line_idx;
                tp.word_idx = state.word_idx;
                tp.char_idx = char_idx;
                glyphs.push(tp);
            }
            x_curr += advance;
        }
    }
}

/// Centers text paths around the origin so that layout positioning works correctly.
/// The layout system positions children by their center point, so text needs to be
/// centered around (0, 0) for layout alignment to work properly.
///
/// Returns the Y offset of the baseline relative to the new center (0, 0).
/// The baseline was at Y=0 in the pre-centering coordinate system (Typst/fast-path convention).
/// After centering, the baseline is at Y = `-(min_y + max_y) / 2.0`.
pub fn center_text_paths(paths: &mut [TextPath]) -> f32 {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for path in paths.iter() {
        let bounds = path.bounds;
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        let offset = Affine::translate((-center_x, -center_y));
        let vec_offset = kurbo::Vec2::new(-center_x, -center_y);

        for path in paths.iter_mut() {
            path.path.apply_affine(offset);
            path.bounds = path.bounds + vec_offset;
            let b = path.bounds;
            if b.width().is_finite() && b.height().is_finite() {
                path.glyph_center = [((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32];
            }
        }

        // Baseline was at Y=0 before centering; after shifting by -center_y, it is at -center_y.
        -(center_y as f32)
    } else {
        0.0
    }
}

/// Measure the bounding box of text paths and return half-width and half-height.
pub fn measure_text_paths(paths: &[TextPath]) -> [f32; 2] {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for text_path in paths {
        let bounds = text_path.bounds;
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        [
            ((max_x - min_x) as f32) / 2.0,
            ((max_y - min_y) as f32) / 2.0,
        ]
    } else {
        [0.0, 0.0]
    }
}

/// Compute resting baseline clip rectangles for Split-Text baseline reveal masking.
///
/// Precomputed once at build time to avoid scanning glyph boundaries per frame.
pub fn compute_split_mask_clip(paths: &[TextPath]) -> Option<BezPath> {
    if paths.is_empty() {
        return None;
    }
    let mut line_bounds: Vec<(f64, f64, f64, f64)> = Vec::new();
    for g in paths.iter() {
        let idx = g.line_idx as usize;
        if idx >= line_bounds.len() {
            line_bounds.resize(
                idx + 1,
                (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
            );
        }
        let b = g.bounds;
        let e = &mut line_bounds[idx];
        e.0 = e.0.min(b.x0);
        e.1 = e.1.min(b.y0);
        e.2 = e.2.max(b.x1);
        e.3 = e.3.max(b.y1);
    }
    let mut bez = BezPath::new();
    for (x0, y0, x1, y1) in line_bounds {
        if x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() {
            let rx0 = x0 - 10.0;
            let ry0 = y0 - 1000.0;
            let rx1 = x1 + 10.0;
            let ry1 = y1 + 4.0;
            bez.move_to((rx0, ry0));
            bez.line_to((rx1, ry0));
            bez.line_to((rx1, ry1));
            bez.line_to((rx0, ry1));
            bez.close_path();
        }
    }
    Some(bez)
}
#[derive(Default)]
#[cfg(feature = "rich-text")]
struct GlyphLayoutState {
    line_idx: u16,
    word_idx: u16,
    last_y: Option<f64>,
    last_end: usize,
}

#[cfg(feature = "rich-text")]
fn walk_frame_for_glyphs_with_state(
    frame: &Frame,
    current_transform: Transform,
    glyphs: &mut Vec<TextPath>,
    state: &mut GlyphLayoutState,
) {
    for (pos, item) in frame.items() {
        let transform = current_transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_glyphs_with_state(&group.frame, group_transform, glyphs, state);
            },
            FrameItem::Text(text) => {
                let current_y = transform.ty.to_pt();
                if let Some(last_y) = state.last_y {
                    if (current_y - last_y).abs() > 2.0 {
                        state.line_idx += 1;
                        state.last_y = Some(current_y);
                        state.last_end = 0;
                    }
                } else {
                    state.last_y = Some(current_y);
                }

                let text_str = text.text.as_str();
                let size = text.size.to_pt() as f32;
                let units_per_em = text.font.units_per_em() as f32;
                let font_scale = size / units_per_em;
                let face = text.font.ttf();

                let mut item_last_end = 0usize;
                let mut x_curr = 0.0;
                for glyph in &text.glyphs {
                    let start = glyph.range.start as usize;
                    let end = glyph.range.end as usize;
                    let is_space = text_str
                        .get(start..end)
                        .map(|s| s.chars().all(|c| c.is_whitespace()))
                        .unwrap_or(false);
                    let is_cjk = text_str
                        .get(start..end)
                        .map(|s| s.chars().any(is_cjk_char))
                        .unwrap_or(false);
                    if is_space
                        || is_cjk
                        || (start > item_last_end
                            && text_str
                                .get(item_last_end..start)
                                .map(|s| s.chars().any(|c| c.is_whitespace() || is_cjk_char(c)))
                                .unwrap_or(false))
                    {
                        state.word_idx += 1;
                    }
                    item_last_end = end;

                    let offset_x = glyph.x_offset.at(text.size).to_pt() as f32;
                    let offset_y = glyph.y_offset.at(text.size).to_pt() as f32;
                    let advance = glyph.x_advance.at(text.size).to_pt() as f32;

                    let mut builder = PathBuilder(BezPath::new());
                    if let Some(_bounds) =
                        face.outline_glyph(ttf_parser::GlyphId(glyph.id), &mut builder)
                    {
                        let path = builder.0;

                        let scale_affine =
                            Affine::scale_non_uniform(font_scale as f64, -font_scale as f64);

                        let glyph_translate = Affine::translate(kurbo::Vec2::new(
                            (x_curr + offset_x) as f64,
                            offset_y as f64,
                        ));

                        let item_transform = Affine::new([
                            transform.sx.get(),
                            transform.ky.get(),
                            transform.kx.get(),
                            transform.sy.get(),
                            transform.tx.to_pt(),
                            transform.ty.to_pt(),
                        ]);

                        let final_affine = item_transform * glyph_translate * scale_affine;
                        let mut final_path = path;
                        final_path.apply_affine(final_affine);

                        let char_idx = glyphs.len() as u16;
                        let mut tp = TextPath::new(final_path, paint_to_rgba(&text.fill), 1.0);
                        tp.line_idx = state.line_idx;
                        tp.word_idx = state.word_idx;
                        tp.char_idx = char_idx;
                        glyphs.push(tp);
                    }

                    x_curr += advance;
                }
            },
            _ => {},
        }
    }
}

#[cfg(feature = "rich-text")]
fn walk_frame_for_glyphs(frame: &Frame, current_transform: Transform, glyphs: &mut Vec<TextPath>) {
    let mut state = GlyphLayoutState::default();
    walk_frame_for_glyphs_with_state(frame, current_transform, glyphs, &mut state);
}
#[cfg(feature = "rich-text")]
/// Extract shapes from a Typst frame.
pub fn extract_shapes(frame: &Frame) -> Vec<ExtractedShape> {
    let mut shapes = Vec::new();
    walk_frame_for_shapes(frame, Transform::identity(), &mut shapes);
    shapes
}
#[cfg(feature = "rich-text")]
fn walk_frame_for_shapes(
    frame: &Frame,
    current_transform: Transform,
    shapes: &mut Vec<ExtractedShape>,
) {
    for (pos, item) in frame.items() {
        let transform = current_transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_shapes(&group.frame, group_transform, shapes);
            },
            FrameItem::Shape(shape, _span) => {
                if let typst::visualize::Geometry::Curve(curve) = &shape.geometry {
                    shapes.push(ExtractedShape {
                        curve: (*curve).clone(),
                        transform,
                    });
                }
            },
            _ => {},
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Plain-text fast path helpers (Phase 4)
// ─────────────────────────────────────────────────────────────

/// Returns `true` iff the string contains no Typst markup-special characters
/// and no newlines (single-line only).
pub fn is_plain_text(content: &str) -> bool {
    !content.contains('\n')
        && !content.chars().any(|c| {
            matches!(
                c,
                '*' | '_'
                    | '$'
                    | '\\'
                    | '#'
                    | '<'
                    | '>'
                    | '~'
                    | '`'
                    | '['
                    | ']'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '/'
                    | '@'
            )
        })
}

/// Returns `true` iff the character is a CJK ideograph, syllabary, or punctuation mark.
pub fn is_cjk_char(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2EFF   // CJK Radicals Supplement
        | 0x3000..=0x303F // CJK Symbols and Punctuation
        | 0x3040..=0x309F // Hiragana
        | 0x30A0..=0x30FF // Katakana
        | 0x3100..=0x312F // Bopomofo
        | 0x3200..=0x32FF // Enclosed CJK Letters and Months
        | 0x3400..=0x4DBF // CJK Unified Ideographs Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0xAC00..=0xD7AF // Hangul Syllables
        | 0xFF00..=0xFFEF // Halfwidth and Fullwidth Forms
        | 0x20000..=0x2FA1F // CJK Unified Ideographs Ext B..F & Compat Supp
    )
}

/// Returns `true` iff the string contains only characters in Latin script ranges
/// (Basic Latin, Latin-1 Supplement, Latin Extended-A) plus common whitespace.
/// Non-Latin text (CJK, Arabic, Cyrillic, etc.) falls back to the Typst path.
pub fn is_latin_text(content: &str) -> bool {
    content.chars().all(|c| {
        let cp = c as u32;
        // Basic Latin, Latin-1 Supplement, Latin Extended-A
        cp <= 0x017F
            // Spacing modifier letters / IPA Extensions
            || (0x02B0..=0x02FF).contains(&cp)
            // General punctuation block (spaces, dashes, quotes)
            || (0x2000..=0x206F).contains(&cp)
            // Also accept the replacement character \uFFFD (common in font fallback)
            || cp == 0xFFFD
    })
}

/// Characters in `content` that the render path has no glyph for and will
/// draw as tofu. Which face set governs follows [`compile_text_cached`]'s
/// selection, cfg included: fast-path text (plain, Latin, `allow_fast_path`)
/// is checked against the family's face plus the bundled fallback set, and
/// everything else goes through the Typst engine, whose fallback chain draws
/// from the whole registered database — a character no registered face covers
/// is tofu there too.
///
/// Whitespace is never reported. Empty when the fast-path family cannot be
/// loaded at all, because [`compile_text_fast`] then fails with a proper
/// error of its own.
pub fn missing_text_glyphs(
    kind: TextKind,
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    allow_fast_path: bool,
    font_ctx: &FontContext,
) -> Vec<char> {
    #[cfg(feature = "rich-text")]
    let takes_fast_path = allow_fast_path
        && kind == TextKind::Text
        && is_plain_text(content)
        && is_latin_text(content);
    #[cfg(not(feature = "rich-text"))]
    let takes_fast_path = true;
    let _ = (kind, allow_fast_path);

    let chars: Vec<char> = content.chars().filter(|c| !c.is_whitespace()).collect();
    let mut missing: Vec<char> = Vec::new();

    if takes_fast_path {
        // Same face resolution as `compile_text_fast`: the family's bundled
        // face if bundled, else the registered database, then every other
        // bundled face as the per-char fallback set.
        let resolved_family = resolve_font_family(family, font_ctx);
        let mut faces: Vec<ttf_parser::Face<'static>> = Vec::new();
        if let Some(bf) = bundled_face(&resolved_family, weight, style) {
            if let Ok(face) = ttf_parser::Face::parse(bf.data, 0) {
                faces.push(face);
            }
        }
        if faces.is_empty() {
            match font_ctx.load_face(&resolved_family, weight, style) {
                Some(face) => faces.push(face),
                // `compile_text_fast` fails with a clear error of its own in
                // this case; the probe stays quiet rather than double-reporting.
                None => return Vec::new(),
            }
        }
        faces.extend(
            BUNDLED_FONTS
                .iter()
                .filter(|bf| bf.family != resolved_family)
                .filter_map(|bf| ttf_parser::Face::parse(bf.data, 0).ok()),
        );

        for c in chars {
            if !missing.contains(&c) && faces.iter().all(|face| face.glyph_index(c).is_none()) {
                missing.push(c);
            }
        }
    } else {
        // Typst-path text: its fallback chain can draw from any registered
        // face, so the honest coverage predicate is the whole database.
        #[cfg(feature = "rich-text")]
        {
            let mut covered = vec![false; chars.len()];
            for face_info in font_ctx.db.faces() {
                font_ctx.db.with_face_data(face_info.id, |data, face_index| {
                    let Ok(face) = ttf_parser::Face::parse(data, face_index) else {
                        return;
                    };
                    for (index, c) in chars.iter().enumerate() {
                        if !covered[index] && face.glyph_index(*c).is_some() {
                            covered[index] = true;
                        }
                    }
                });
            }
            for (index, c) in chars.iter().enumerate() {
                if !covered[index] && !missing.contains(c) {
                    missing.push(*c);
                }
            }
        }
    }

    missing
}

/// Font face with both `ttf-parser` and `rustybuzz` representations for fast-path shaping.
pub struct FastPathFace {
    pub ttf: ttf_parser::Face<'static>,
    pub rb: rustybuzz::Face<'static>,
    pub family: String,
    pub font_data: &'static [u8],
    pub font_index: u32,
}

impl FastPathFace {
    pub fn from_data(data: &'static [u8], index: u32, family: String) -> Option<Self> {
        let ttf = ttf_parser::Face::parse(data, index).ok()?;
        let rb = rustybuzz::Face::from_slice(data, index)?;
        Some(Self {
            ttf,
            rb,
            family,
            font_data: data,
            font_index: index,
        })
    }
}

/// A compact hardware glyph descriptor for direct Vello hardware glyph run rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct HardwareGlyph {
    /// Font glyph ID.
    pub id: u32,
    /// X position in local scene coordinates (points) relative to text center.
    pub x: f32,
    /// Y position in local scene coordinates (points) relative to text center.
    pub y: f32,
}

/// A contiguous run of glyphs sharing the same font face and font size.
#[derive(Debug, Clone)]
pub struct HardwareGlyphRun {
    /// Pointer to static font bytes.
    pub font_data: &'static [u8],
    /// Font index inside font collection (ttc).
    pub font_index: u32,
    /// Font size in points.
    pub font_size: f32,
    /// Stream of positioned glyphs.
    pub glyphs: Vec<HardwareGlyph>,
}

/// A complete hardware glyph layout comprised of one or more runs (supporting font fallback).
#[derive(Debug, Clone, Default)]
pub struct HardwareTextLayout {
    /// Sequence of runs in draw order.
    pub runs: Vec<HardwareGlyphRun>,
}

struct HardwarePendingGlyph {
    face_idx: usize,
    id: u32,
    x: f32,
    y: f32,
}

struct ShapedGlyph {
    face_idx: usize,
    glyph_id: u16,
    x_offset: f64,
    y_offset: f64,
}

fn shape_token(
    token_text: &str,
    faces: &[FastPathFace],
    size: f32,
    letter_spacing: f32,
) -> (f64, Vec<ShapedGlyph>) {
    if token_text.is_empty() || faces.is_empty() {
        return (0.0, Vec::new());
    }

    let mut best_face_idx = 0;
    let mut best_missing = usize::MAX;
    let mut best_buffer: Option<rustybuzz::GlyphBuffer> = None;

    for (idx, face) in faces.iter().enumerate() {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(token_text);
        let shaped = rustybuzz::shape(&face.rb, &[], buffer);
        let missing_count = shaped.glyph_infos().iter().filter(|g| g.glyph_id == 0).count();
        if missing_count == 0 {
            best_face_idx = idx;
            best_buffer = Some(shaped);
            break;
        }
        if missing_count < best_missing {
            best_missing = missing_count;
            best_face_idx = idx;
            best_buffer = Some(shaped);
        }
    }

    let shaped = match best_buffer {
        Some(b) => b,
        None => {
            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(token_text);
            rustybuzz::shape(&faces[0].rb, &[], buffer)
        },
    };

    let face = &faces[best_face_idx];
    let units_per_em = face.ttf.units_per_em() as f32;
    let font_scale = (size / units_per_em) as f64;
    let letter_spacing_f64 = letter_spacing as f64;

    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();

    let mut glyphs = Vec::with_capacity(infos.len());
    let mut total_width = 0.0f64;

    for (info, pos) in infos.iter().zip(positions.iter()) {
        if info.glyph_id == 0 && token_text.trim().is_empty() {
            total_width += pos.x_advance as f64 * font_scale + letter_spacing_f64;
            continue;
        }
        let x_adv = pos.x_advance as f64 * font_scale + letter_spacing_f64;
        let x_off = pos.x_offset as f64 * font_scale;
        let y_off = pos.y_offset as f64 * font_scale;

        glyphs.push(ShapedGlyph {
            face_idx: best_face_idx,
            glyph_id: info.glyph_id as u16,
            x_offset: total_width + x_off,
            y_offset: y_off,
        });
        total_width += x_adv;
    }

    (total_width, glyphs)
}

/// Compile plain text into glyph paths using `rustybuzz` and `ttf_parser` directly,
/// bypassing Typst entirely. This is the fast path.
///
/// The returned paths use the same coordinate convention as `extract_glyphs`
/// (centered around origin), so `measure_text_paths` works unchanged.
pub fn compile_text_fast(
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    size: f32,
    color: [f32; 4],
    letter_spacing: f32,
    word_spacing: f32,
    font_ctx: &FontContext,
) -> Result<CompiledText, RenderError> {
    // Early return for empty string: no glyphs, zero metrics
    if content.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: 0.0,
            descent: 0.0,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    if content.contains('\n') {
        return compile_text_fast_wrapped(
            content,
            family,
            weight,
            style,
            size,
            1.2,
            color,
            letter_spacing,
            word_spacing,
            font_ctx,
            0.0,
            None,
            "left",
            "visible",
        );
    }

    let resolved_family = resolve_font_family(family, font_ctx);
    let (primary_data, primary_index) = 'font: {
        if let Some(bf) = bundled_face(&resolved_family, weight, style) {
            break 'font (bf.data, 0);
        }
        if let Some((data, idx)) = font_ctx.load_face_data(&resolved_family, weight, style) {
            break 'font (data, idx);
        }
        return Err(RenderError::TextCompilation(format!(
            "Failed to load font '{}' (weight={}, style={}) for fast path",
            resolved_family, weight, style
        )));
    };

    let Some(primary_face) =
        FastPathFace::from_data(primary_data, primary_index, resolved_family.clone())
    else {
        return Err(RenderError::TextCompilation(format!(
            "Failed to parse font '{}' (weight={}, style={}) for fast path",
            resolved_family, weight, style
        )));
    };

    let units_per_em = primary_face.ttf.units_per_em() as f32;
    let font_scale = size / units_per_em;
    let ascent = primary_face.ttf.ascender() as f32;
    let descent = primary_face.ttf.descender() as f32;

    let mut faces = vec![primary_face];
    faces.extend(
        BUNDLED_FONTS
            .iter()
            .filter(|bf| bf.family != resolved_family)
            .filter_map(|bf| FastPathFace::from_data(bf.data, 0, bf.family.to_string())),
    );

    let rgba = f32_color_to_rgba8(&color);

    let mut char_indices = content.char_indices().peekable();
    let mut pending_space = false;
    let mut word_start: Option<usize> = None;
    let mut raw_tokens: Vec<(&str, bool)> = Vec::new();

    while let Some((idx, ch)) = char_indices.next() {
        if ch == ' ' || ch == '\t' {
            if let Some(start) = word_start.take() {
                raw_tokens.push((&content[start..idx], pending_space));
            }
            pending_space = true;
        } else if is_cjk_char(ch) {
            if let Some(start) = word_start.take() {
                raw_tokens.push((&content[start..idx], pending_space));
                pending_space = false;
            }
            let next_idx = char_indices.peek().map(|(i, _)| *i).unwrap_or(content.len());
            raw_tokens.push((&content[idx..next_idx], pending_space));
            pending_space = false;
        } else if word_start.is_none() {
            word_start = Some(idx);
        }
    }
    if let Some(start) = word_start {
        raw_tokens.push((&content[start..], pending_space));
    }

    let space_advance = {
        let (w, _) = shape_token(" ", &faces, size, letter_spacing);
        if w > 0.0 {
            w + word_spacing as f64
        } else {
            (size as f64 * 0.25) + word_spacing as f64
        }
    };

    let mut glyphs: Vec<TextPath> = Vec::new();
    let mut x_curr: f64 = 0.0;

    struct HardwarePendingGlyph {
        face_idx: usize,
        id: u32,
        x: f32,
        y: f32,
    }
    let mut pending_hardware: Vec<HardwarePendingGlyph> = Vec::new();

    for (wi_idx, (w, space_before)) in raw_tokens.into_iter().enumerate() {
        if wi_idx > 0 && space_before {
            x_curr += space_advance;
        }
        let (token_width, shaped_glyphs) = shape_token(w, &faces, size, letter_spacing);
        for g in shaped_glyphs {
            let face = &faces[g.face_idx];
            let glyph_scale = size / (face.ttf.units_per_em() as f32);

            pending_hardware.push(HardwarePendingGlyph {
                face_idx: g.face_idx,
                id: g.glyph_id as u32,
                x: (x_curr + g.x_offset) as f32,
                y: g.y_offset as f32,
            });

            let mut builder = PathBuilder(BezPath::new());
            if face.ttf.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut builder).is_some() {
                let path = builder.0;
                let scale_affine =
                    Affine::scale_non_uniform(glyph_scale as f64, -glyph_scale as f64);
                let translate =
                    Affine::translate(kurbo::Vec2::new(x_curr + g.x_offset, g.y_offset));
                let final_affine = translate * scale_affine;
                let mut final_path = path;
                final_path.apply_affine(final_affine);

                let c_idx = glyphs.len() as u16;
                let bounds = final_path.bounding_box();
                glyphs.push(TextPath {
                    path: final_path,
                    color: rgba,
                    opacity: 1.0,
                    line_idx: 0,
                    word_idx: wi_idx as u16,
                    char_idx: c_idx,
                    glyph_center: [0.0, 0.0],
                    bounds,
                });
            }
        }
        x_curr += token_width;
    }

    // Compute bounding box BEFORE centering to find center_x and center_y
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for path in glyphs.iter() {
        let b = path.bounds;
        min_x = min_x.min(b.x0);
        max_x = max_x.max(b.x1);
        min_y = min_y.min(b.y0);
        max_y = max_y.max(b.y1);
    }

    let center_x = if min_x.is_finite() && max_x.is_finite() {
        (min_x + max_x) / 2.0
    } else {
        0.0
    };
    let center_y = if min_y.is_finite() && max_y.is_finite() {
        (min_y + max_y) / 2.0
    } else {
        0.0
    };

    // Center paths around origin (same as center_text_paths)
    // capture baseline offset: was at Y=0 before centering
    let baseline_offset = center_text_paths(&mut glyphs);

    let ascent_scaled = ascent * font_scale;
    let descent_scaled = descent * font_scale;

    let hardware_layout = if !pending_hardware.is_empty() {
        let mut runs: Vec<HardwareGlyphRun> = Vec::new();
        for pg in pending_hardware {
            let face = &faces[pg.face_idx];
            let centered_x = pg.x - center_x as f32;
            let centered_y = pg.y - center_y as f32;
            if let Some(last_run) = runs.last_mut() {
                if std::ptr::eq(last_run.font_data, face.font_data)
                    && last_run.font_index == face.font_index
                {
                    last_run.glyphs.push(HardwareGlyph {
                        id: pg.id,
                        x: centered_x,
                        y: centered_y,
                    });
                    continue;
                }
            }
            runs.push(HardwareGlyphRun {
                font_data: face.font_data,
                font_index: face.font_index,
                font_size: size,
                glyphs: vec![HardwareGlyph {
                    id: pg.id,
                    x: centered_x,
                    y: centered_y,
                }],
            });
        }
        Some(HardwareTextLayout { runs })
    } else {
        None
    };

    Ok(CompiledText {
        glyphs,
        ascent: ascent_scaled,
        descent: descent_scaled,
        baseline_offset,
        hardware_layout,
    })
}

/// Compile plain text into glyph paths with word wrapping (fast path).
///
/// Like `compile_text_fast`, but wraps text at `max_width` points.
/// Honors `text_align` per line and `overflow` mode.
#[allow(clippy::too_many_arguments)]
pub fn compile_text_fast_wrapped(
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    size: f32,
    line_height: f32,
    color: [f32; 4],
    letter_spacing: f32,
    word_spacing: f32,
    font_ctx: &FontContext,
    max_width: f32,
    max_lines: Option<usize>,
    text_align: &str,
    overflow: &str,
) -> Result<CompiledText, RenderError> {
    let resolved_family = resolve_font_family(family, font_ctx);
    let (primary_data, primary_index) = 'font: {
        if let Some(bf) = bundled_face(&resolved_family, weight, style) {
            break 'font (bf.data, 0);
        }
        if let Some((data, idx)) = font_ctx.load_face_data(&resolved_family, weight, style) {
            break 'font (data, idx);
        }
        return Err(RenderError::TextCompilation(format!(
            "Failed to load font '{}' (weight={}, style={}) for fast path wrapped",
            resolved_family, weight, style
        )));
    };

    let Some(primary_face) =
        FastPathFace::from_data(primary_data, primary_index, resolved_family.clone())
    else {
        return Err(RenderError::TextCompilation(format!(
            "Failed to parse font '{}' (weight={}, style={}) for fast path wrapped",
            resolved_family, weight, style
        )));
    };

    let units_per_em = primary_face.ttf.units_per_em() as f32;
    let font_scale = size / units_per_em;

    let ascent = primary_face.ttf.ascender() as f32;
    let descent = primary_face.ttf.descender() as f32;
    let line_gap = primary_face.ttf.line_gap() as f32;
    let _metrics = TextMetrics {
        ascent,
        descent,
        line_gap,
        units_per_em,
    };

    let mut faces = vec![primary_face];
    faces.extend(
        BUNDLED_FONTS
            .iter()
            .filter(|bf| bf.family != resolved_family)
            .filter_map(|bf| FastPathFace::from_data(bf.data, 0, bf.family.to_string())),
    );

    let rgba = f32_color_to_rgba8(&color);

    // Line height in absolute points
    let lh_mult = if line_height > 0.0 { line_height } else { 1.2 };
    let line_height_pts = (ascent - descent + line_gap) * font_scale * lh_mult;

    if content.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: ascent * font_scale,
            descent: descent * font_scale,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    struct WordInfo {
        #[allow(dead_code)]
        text: String,
        width: f64, // total advance in scene coords
        glyphs: Vec<ShapedGlyph>,
        space_before: bool,
    }

    struct LineInfo {
        words: Vec<usize>, // indices into word_infos
        total_width: f64,
    }

    let mut word_infos: Vec<WordInfo> = Vec::new();
    let mut lines: Vec<LineInfo> = Vec::new();
    let wrap_threshold = if max_width > 0.0 {
        max_width as f64
    } else {
        f64::INFINITY
    };

    let space_advance = {
        let (w, _) = shape_token(" ", &faces, size, letter_spacing);
        if w > 0.0 {
            w + word_spacing as f64
        } else {
            (size as f64 * 0.25) + word_spacing as f64
        }
    };

    for paragraph in content.split('\n') {
        let p = paragraph.strip_suffix('\r').unwrap_or(paragraph);
        let mut char_indices = p.char_indices().peekable();
        let mut pending_space = false;
        let mut word_start: Option<usize> = None;
        let mut raw_tokens: Vec<(&str, bool)> = Vec::new();

        while let Some((idx, ch)) = char_indices.next() {
            if ch == ' ' || ch == '\t' {
                if let Some(start) = word_start.take() {
                    raw_tokens.push((&p[start..idx], pending_space));
                }
                pending_space = true;
            } else if is_cjk_char(ch) {
                if let Some(start) = word_start.take() {
                    raw_tokens.push((&p[start..idx], pending_space));
                    pending_space = false;
                }
                let next_idx = char_indices.peek().map(|(i, _)| *i).unwrap_or(p.len());
                raw_tokens.push((&p[idx..next_idx], pending_space));
                pending_space = false;
            } else if word_start.is_none() {
                word_start = Some(idx);
            }
        }
        if let Some(start) = word_start {
            raw_tokens.push((&p[start..], pending_space));
        }

        if raw_tokens.is_empty() {
            lines.push(LineInfo {
                words: Vec::new(),
                total_width: 0.0,
            });
            continue;
        }

        let mut paragraph_word_indices = Vec::with_capacity(raw_tokens.len());
        for (w, space_before) in raw_tokens {
            let (token_width, shaped_glyphs) = shape_token(w, &faces, size, letter_spacing);
            let wi_idx = word_infos.len();
            word_infos.push(WordInfo {
                text: w.to_string(),
                width: token_width,
                glyphs: shaped_glyphs,
                space_before,
            });
            paragraph_word_indices.push(wi_idx);
        }

        let mut current_line = LineInfo {
            words: Vec::new(),
            total_width: 0.0,
        };

        for &wi_idx in &paragraph_word_indices {
            let wi = &word_infos[wi_idx];
            let space_needed = if current_line.words.is_empty() {
                0.0
            } else if wi.space_before {
                space_advance
            } else {
                0.0
            };

            if current_line.total_width + space_needed + wi.width > wrap_threshold
                && !current_line.words.is_empty()
            {
                lines.push(current_line);
                current_line = LineInfo {
                    words: Vec::new(),
                    total_width: 0.0,
                };
            }

            if !current_line.words.is_empty() && wi.space_before {
                current_line.total_width += space_advance;
            }
            current_line.words.push(wi_idx);
            current_line.total_width += wi.width;
        }
        if !current_line.words.is_empty() {
            lines.push(current_line);
        }
    }

    if word_infos.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: ascent * font_scale,
            descent: descent * font_scale,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    // Handle overflow. An explicit `max_lines` (the `max_lines` property) wins;
    // otherwise ellipsis/clip keeps the old "one page worth" guard so unbounded
    // growth is still prevented.
    let line_limit = max_lines.or(if overflow == "ellipsis" || overflow == "clip" {
        // Limit to roughly the visible area: one page worth (no explicit height limit yet)
        // For now, just prevent unbounded growth
        Some(100usize)
    } else {
        None
    });

    let truncated_lines: &[LineInfo] = if let Some(limit) = line_limit {
        if lines.len() > limit {
            &lines[..limit]
        } else {
            &lines
        }
    } else {
        &lines
    };

    let mut glyphs: Vec<TextPath> = Vec::new();
    let mut y_curr: f64 = -(truncated_lines.len() as f64 * line_height_pts as f64) / 2.0; // start at top, centered vertically
    let mut pending_hardware: Vec<HardwarePendingGlyph> = Vec::new();

    for (line_idx, line) in truncated_lines.iter().enumerate() {
        let mut x_curr: f64 = 0.0;

        // Compute line width from word widths
        let line_width = line.total_width;

        // Compute x offset based on text_align
        let x_offset = match text_align {
            "center" => (wrap_threshold - line_width) / 2.0,
            "right" => wrap_threshold - line_width,
            "justify" => 0.0, // handled per-word below
            _ => 0.0,         // left
        };

        let is_last_line = line_idx == truncated_lines.len() - 1;
        let is_justify = text_align == "justify" && !is_last_line && line.words.len() > 1;
        let extra_space_per_gap = if is_justify {
            (wrap_threshold - line_width) / (line.words.len() - 1) as f64
        } else {
            0.0
        };

        for (wi_idx_in_line, &wi_idx) in line.words.iter().enumerate() {
            let wi = &word_infos[wi_idx];

            // Add space before word (except first)
            if wi_idx_in_line > 0 && wi.space_before {
                if is_justify {
                    x_curr += space_advance + extra_space_per_gap;
                } else {
                    x_curr += space_advance;
                }
            }

            // Render each glyph in the word using pre-computed offsets that include kerning
            for g in &wi.glyphs {
                let face = &faces[g.face_idx];
                let glyph_scale = size / (face.ttf.units_per_em() as f32);

                pending_hardware.push(HardwarePendingGlyph {
                    face_idx: g.face_idx,
                    id: g.glyph_id as u32,
                    x: (x_curr + x_offset + g.x_offset) as f32,
                    y: (y_curr + g.y_offset) as f32,
                });

                let mut builder = PathBuilder(BezPath::new());
                if face.ttf.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut builder).is_some() {
                    let path = builder.0;
                    let scale_affine =
                        Affine::scale_non_uniform(glyph_scale as f64, -glyph_scale as f64);
                    let translate = Affine::translate(kurbo::Vec2::new(
                        x_curr + x_offset + g.x_offset,
                        y_curr + g.y_offset,
                    ));
                    let final_affine = translate * scale_affine;

                    let mut final_path = path;
                    final_path.apply_affine(final_affine);

                    let c_idx = glyphs.len() as u16;
                    let bounds = final_path.bounding_box();
                    glyphs.push(TextPath {
                        path: final_path,
                        color: rgba,
                        opacity: 1.0,
                        line_idx: line_idx as u16,
                        word_idx: wi_idx as u16,
                        char_idx: c_idx,
                        glyph_center: [0.0, 0.0],
                        bounds,
                    });
                }
            }
            // Advance x_curr by the total word width (includes kerning)
            x_curr += wi.width;
        }

        // Add ellipsis for overflow: "ellipsis" on last visible line if truncated
        if overflow == "ellipsis"
            && line_idx == truncated_lines.len() - 1
            && lines.len() > truncated_lines.len()
        {
            let (dot_w, dot_glyphs) = shape_token("…", &faces, size, 0.0);
            let (_ell_w, ell_glyphs) = if dot_glyphs.is_empty() || dot_glyphs[0].glyph_id == 0 {
                shape_token(".", &faces, size, 0.0)
            } else {
                (dot_w, dot_glyphs)
            };
            for g in ell_glyphs {
                let face = &faces[g.face_idx];
                let glyph_scale = size / (face.ttf.units_per_em() as f32);

                pending_hardware.push(HardwarePendingGlyph {
                    face_idx: g.face_idx,
                    id: g.glyph_id as u32,
                    x: (x_curr + x_offset + g.x_offset) as f32,
                    y: (y_curr + g.y_offset) as f32,
                });

                let mut builder = PathBuilder(BezPath::new());
                if face.ttf.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut builder).is_some() {
                    let path = builder.0;
                    let scale_affine =
                        Affine::scale_non_uniform(glyph_scale as f64, -glyph_scale as f64);
                    let translate = Affine::translate(kurbo::Vec2::new(
                        x_curr + x_offset + g.x_offset,
                        y_curr + g.y_offset,
                    ));
                    let final_affine = translate * scale_affine;
                    let mut final_path = path;
                    final_path.apply_affine(final_affine);
                    let c_idx = glyphs.len() as u16;
                    let last_w_idx = line.words.last().copied().unwrap_or(0) as u16;
                    let bounds = final_path.bounding_box();
                    glyphs.push(TextPath {
                        path: final_path,
                        color: rgba,
                        opacity: 1.0,
                        line_idx: line_idx as u16,
                        word_idx: last_w_idx,
                        char_idx: c_idx,
                        glyph_center: [0.0, 0.0],
                        bounds,
                    });
                }
            }
        }

        y_curr += line_height_pts as f64;
    }

    // Compute bounding box BEFORE centering to find center_x and center_y
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for path in glyphs.iter() {
        let b = path.bounds;
        min_x = min_x.min(b.x0);
        max_x = max_x.max(b.x1);
        min_y = min_y.min(b.y0);
        max_y = max_y.max(b.y1);
    }

    let center_x = if min_x.is_finite() && max_x.is_finite() {
        (min_x + max_x) / 2.0
    } else {
        0.0
    };
    let center_y = if min_y.is_finite() && max_y.is_finite() {
        (min_y + max_y) / 2.0
    } else {
        0.0
    };

    // Center all paths around origin
    let baseline_offset = center_text_paths(&mut glyphs);

    let ascent_scaled = ascent * font_scale;
    let descent_scaled = descent * font_scale;

    let hardware_layout = if !pending_hardware.is_empty() {
        let mut runs: Vec<HardwareGlyphRun> = Vec::new();
        for pg in pending_hardware {
            let face = &faces[pg.face_idx];
            let centered_x = pg.x - center_x as f32;
            let centered_y = pg.y - center_y as f32;
            if let Some(last_run) = runs.last_mut() {
                if std::ptr::eq(last_run.font_data, face.font_data)
                    && last_run.font_index == face.font_index
                {
                    last_run.glyphs.push(HardwareGlyph {
                        id: pg.id,
                        x: centered_x,
                        y: centered_y,
                    });
                    continue;
                }
            }
            runs.push(HardwareGlyphRun {
                font_data: face.font_data,
                font_index: face.font_index,
                font_size: size,
                glyphs: vec![HardwareGlyph {
                    id: pg.id,
                    x: centered_x,
                    y: centered_y,
                }],
            });
        }
        Some(HardwareTextLayout { runs })
    } else {
        None
    };

    Ok(CompiledText {
        glyphs,
        ascent: ascent_scaled,
        descent: descent_scaled,
        baseline_offset,
        hardware_layout,
    })
}

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
/// Keyed on the same inputs as [`compile_text_cached`] (including the font
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

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[path = "tests/text_tests.rs"]
#[cfg(test)]
mod tests;
