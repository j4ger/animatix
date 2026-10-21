use std::collections::HashMap;
#[cfg(feature = "rich-text")]
use std::sync::Arc;

#[cfg(feature = "rich-text")]
use typst::foundations::Bytes;
#[cfg(feature = "rich-text")]
use typst::text::Font;

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
    pub(crate) db: std::sync::Arc<fontdb::Database>,
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
    pub(crate) epoch: u64,
}

/// Monotonic generation counter for the process font environment.
static FONT_ENV_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn font_env_epoch() -> u64 {
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
    crate::compiler::clear_text_compile_cache();
    next
}

/// Load (once) and cache the system font database for the whole process.
///
/// `fontdb::Database` is only ever read after construction in this codebase, so
/// sharing one immutable instance across all [`FontContext`]s is safe and avoids
/// re-scanning the system font dirs on every build.
pub(crate) fn system_fonts_db() -> &'static std::sync::Arc<fontdb::Database> {
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
    pub(crate) fn load_font_emphasis_faces(&self, family: &str) -> Vec<Font> {
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

    pub(crate) fn has_family(&self, family: &str) -> bool {
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

    pub(crate) fn face_data(db: &fontdb::Database, id: fontdb::ID) -> Option<Vec<u8>> {
        let (source, _index) = db.face_source(id)?;
        match source {
            fontdb::Source::Binary(data) => Some(data.as_ref().as_ref().to_vec()),
            fontdb::Source::File(path) => std::fs::read(path).ok(),
            fontdb::Source::SharedFile(_path, data) => Some(data.as_ref().as_ref().to_vec()),
        }
    }

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

    /// Load a `ttf_parser::Face` for the given family, weight and style.
    /// Returns `None` if the font cannot be found or is not a TrueType/OpenType font.
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
pub(crate) fn leaked_face_data(font_ctx: &FontContext, id: fontdb::ID) -> Option<&'static [u8]> {
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

/// A font entry in the bundled font set.
///
/// `weight`/`style` are the face's own CSS values, recorded so the plain-text
/// fast path can choose among a family's faces the way fontdb's query does for
/// system fonts. Without them the fast path could only ever take the first entry
/// of a family, which made `font_weight: 700` and `font_style: "italic"` no-ops
/// on bundled text.
pub(crate) struct BundledFont {
    pub(crate) family: &'static str,
    pub(crate) data: &'static [u8],
    pub(crate) weight: f32,
    pub(crate) style: &'static str,
}

/// The bundled face of `family` that answers a weight/style query, chosen the
/// way a font database chooses: style is a different shape and so decides first,
/// then the nearest available weight. A family that ships one face — every
/// family in a slim build — therefore gets that face for any query, which is
/// what it got before this existed.
pub(crate) fn bundled_face(family: &str, weight: f32, style: &str) -> Option<&'static BundledFont> {
    let want_italic = style == "italic" || style == "oblique";
    BUNDLED_FONTS.iter().filter(|bf| bf.family == family).min_by_key(|bf| {
        let has_italic = bf.style == "italic" || bf.style == "oblique";
        let style_cost = u32::from(has_italic != want_italic);
        let weight_cost = (bf.weight - weight).abs().round() as u32;
        (style_cost, weight_cost)
    })
}

/// Fonts embedded at compile time. Add new fonts here.
pub(crate) static BUNDLED_FONTS: &[BundledFont] = &[
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

/// The parsed variable pair, built once per process.
///
/// `build_world` runs per text compile, and parsing a 530 KB variable face every
/// time showed up as +8.3% on `scrub_layout_scene_100frames` — a cost the static
/// faces already pay, but which adding two more faces made worth fixing rather
/// than absorbing. `typst::Font` is refcounted internally, so the clone per
/// world is an `Arc` bump; the `FontInfo` still gets cloned into the book because
/// the book owns its entries.
#[cfg(feature = "rich-text")]
pub(crate) static VARIABLE_FONT_FACES: std::sync::LazyLock<Vec<Font>> = std::sync::LazyLock::new(
    || {
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
    },
);

#[cfg(feature = "rich-text")]
pub(crate) static VARIABLE_FONTS: &[BundledFont] = &[
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
