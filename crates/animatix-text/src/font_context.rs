use std::collections::HashMap;
#[cfg(feature = "rich-text")]
use std::sync::Arc;

#[cfg(feature = "rich-text")]
use typst::foundations::Bytes;
#[cfg(feature = "rich-text")]
use typst::text::Font;

// Persistent font database (FontContext)

/// Owns a persistent `fontdb::Database` to avoid redundant scanning (~45-60ms per
/// call). The system database is loaded once per process and shared behind an `Arc`.
#[derive(Clone, Debug)]
pub struct FontContext {
    /// The underlying font database, shared process-wide (only read after load).
    pub(crate) db: std::sync::Arc<fontdb::Database>,
    /// Faces explicitly registered through [`Self::load_fonts`]. Kept separate
    /// from the system faces in `db` so the fast path can fall back to the
    /// caller-provided font pack without scanning unrelated system fonts.
    pub(crate) registered_faces: Vec<fontdb::ID>,
    /// Font-environment generation: memo keys include this value, so contexts
    /// created after [`advance_font_env_epoch`] miss the stale-font cache.
    pub(crate) epoch: u64,
}

/// Monotonic generation counter for the process font environment.
static FONT_ENV_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn font_env_epoch() -> u64 {
    std::sync::atomic::AtomicU64::load(&FONT_ENV_EPOCH, std::sync::atomic::Ordering::Relaxed)
}

/// Declare that the process font environment changed and invalidate every
/// memoized text compilation; contexts created afterwards carry the new epoch.
pub fn advance_font_env_epoch() -> u64 {
    use std::sync::atomic::Ordering;
    let next = FONT_ENV_EPOCH.fetch_add(1, Ordering::Relaxed) + 1;
    // Entries from older epochs can never be hit again; reclaim them now.
    crate::compiler::clear_text_compile_cache();
    next
}

/// Load (once) and cache the system font database for the whole process. The
/// database is only read after construction, so sharing it is safe.
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
    pub fn new() -> Self {
        Self {
            db: std::sync::Arc::clone(system_fonts_db()),
            registered_faces: Vec::new(),
            epoch: font_env_epoch(),
        }
    }

    #[cfg(feature = "rich-text")]
    /// Load the bold/italic emphasis faces of a family into the Typst world.
    ///
    /// `with_fonts_and_fallback` loads only the regular face, so `*bold*`,
    /// `_italic_`, and `font_weight`/`font_style` variants fell back to regular.
    /// Returns regular + bold + italic + bold-italic faces, empty if none.
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

    /// Return at most one regular system face per probe character. Startup-only;
    /// callers should cache the result for the application lifetime.
    pub fn font_for_glyphs(&self, probes: &[char]) -> Vec<(Vec<u8>, u32)> {
        let mut covered = vec![false; probes.len()];
        let mut fonts = Vec::new();

        // First pass prefers the generic UI face shape; second fills remaining probes.
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

        // Fallback scan for probes only covered by a mono/bold face.
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

    /// Register a batch of in-memory fonts so `font_family` can name them.
    ///
    /// Cloning the font database is the expensive part of registration, so a
    /// batch pays it once and advances the font-environment epoch once instead
    /// of once per face. Empty batches are a no-op and do not invalidate the
    /// memoized text compilation cache.
    pub fn load_fonts<I>(&mut self, fonts: I)
    where
        I: IntoIterator<Item = Vec<u8>>,
    {
        let mut db = (*self.db).clone();
        let mut loaded = Vec::new();
        for bytes in fonts {
            let ids = db.load_font_source(fontdb::Source::Binary(std::sync::Arc::new(bytes)));
            loaded.extend(ids);
        }
        if !loaded.is_empty() {
            self.registered_faces.extend(loaded);
            self.db = std::sync::Arc::new(db);
            self.epoch = advance_font_env_epoch();
        }
    }

    /// Register one in-memory font. See [`Self::load_fonts`].
    pub fn load_font_bytes(&mut self, bytes: Vec<u8>) {
        self.load_fonts(std::iter::once(bytes));
    }

    /// Load a `ttf_parser::Face`; `None` if missing or not TrueType/OpenType.
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

    /// Faces registered through [`Self::load_fonts`], in registration order.
    /// The fast path uses these as fallback when no bundled static faces are
    /// compiled in (the web font-pack build).
    pub(crate) fn loaded_faces(&self) -> Vec<LoadedFace> {
        self.registered_faces
            .iter()
            .filter_map(|id| {
                let face = self.db.face(*id)?;
                let family = face.families.first()?.0.clone();
                let data = leaked_face_data(self, *id)?;
                Some(LoadedFace {
                    family,
                    data,
                    index: face.index,
                })
            })
            .collect()
    }
}

// System glyph-fallback fonts

/// Process-wide cache: character → first installed face covering it. `None`
/// caches a known-uncovered char so repeated compiles skip the scan.
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
/// `load_face` returns `Face<'static>` because callers hold it across frames, so
/// the allocation must live for the process and is memoized per face id.
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
/// style, non-monospaced faces. Cached per character.
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
/// Collect Typst fonts covering characters of `content` the primary family
/// cannot render (CJK, Cyrillic, ...), without requiring `font_family`.
/// Bounded at 8 fallback faces per compile; results are cached per face id.
pub(crate) fn collect_fallback_fonts(
    font_ctx: &FontContext,
    content: &str,
    primary_family: &str,
    primary_weight: f32,
    primary_style: &str,
) -> Vec<Font> {
    // Resolve the primary face through shared cached data (`load_face` leaks).
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

// Font bundle

/// A font entry in the bundled font set.
///
/// `weight`/`style` are the face's own CSS values, so the fast path can choose
/// among a family's faces the way fontdb does; without them `font_weight` and
/// `font_style` were no-ops on bundled text.
pub(crate) struct BundledFont {
    pub(crate) family: &'static str,
    pub(crate) data: &'static [u8],
    pub(crate) weight: f32,
    pub(crate) style: &'static str,
}

/// One face explicitly registered at runtime, for fast-path fallback.
pub(crate) struct LoadedFace {
    pub(crate) family: String,
    pub(crate) data: &'static [u8],
    pub(crate) index: u32,
}

/// The bundled face of `family` answering a weight/style query: style decides
/// first, then the nearest available weight.
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
    // Open Sans static faces (Apache-2.0; provenance/SHA-256 in
    // assets/fonts/README.md). `ttf-parser` reads `fvar` but never applies
    // deltas, so the fast path needs static weights.
    #[cfg(feature = "bundled-fonts")]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Regular.ttf"),
        weight: 400.0,
        style: "normal",
    },
    // Noto Sans SC subset (SIL OFL 1.1): 3755 GB2312 hanzi plus CJK punctuation,
    // fullwidth forms and ASCII at weight 400. Not rich-text-gated: the slim
    // build's only shaper needs this face for zero-config CJK coverage.
    #[cfg(feature = "bundled-fonts")]
    BundledFont {
        family: "Noto Sans SC",
        data: include_bytes!("../assets/fonts/NotoSansSC-Common.ttf"),
        weight: 400.0,
        style: "normal",
    },
    // Not rich-text-gated: plain-Latin titles on the slim embed ask for bold
    // (~220 KB). The italic faces stay gated; only a non-embedded example uses
    // italics.
    #[cfg(feature = "bundled-fonts")]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Bold.ttf"),
        weight: 700.0,
        style: "normal",
    },
    // Gated: the slim build ships Regular alone, so `bundled_face` cannot reach it.
    #[cfg(all(feature = "rich-text", feature = "bundled-fonts"))]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Italic.ttf"),
        weight: 400.0,
        style: "italic",
    },
    // Gated: the slim build ships Regular alone, so `bundled_face` cannot reach it.
    #[cfg(all(feature = "rich-text", feature = "bundled-fonts"))]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-BoldItalic.ttf"),
        weight: 700.0,
        style: "italic",
    },
    // Gated: the slim build ships Regular alone, so `bundled_face` cannot reach it.
    #[cfg(all(feature = "rich-text", feature = "bundled-fonts"))]
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

/// Parsed bundled faces, built once per process. Every rich-text build used to
/// parse all bundled bytes again; the vectors are small and `typst::Font` is
/// refcounted, so caching them removes that per-build font-loading cost.
#[cfg(feature = "rich-text")]
pub(crate) static BUNDLED_FONT_FACES: std::sync::LazyLock<Vec<Option<Font>>> =
    std::sync::LazyLock::new(|| {
        BUNDLED_FONTS.iter().map(|bf| Font::new(Bytes::new(bf.data), 0)).collect()
    });

/// The parsed variable pair, built once per process: parsing a 530 KB face per
/// compile cost +8.3% on `scrub_layout_scene_100frames`. `typst::Font` is refcounted.
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
    #[cfg(feature = "bundled-fonts")]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Variable.ttf"),
        weight: 400.0,
        style: "normal",
    },
    #[cfg(feature = "bundled-fonts")]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Italic-Variable.ttf"),
        weight: 400.0,
        style: "italic",
    },
];

/// Resolve a font family to the string used in Typst markup: bundled first,
/// then system fonts, then [`DEFAULT_FONT_FAMILY`].
pub fn resolve_font_family(requested: &str, font_ctx: &FontContext) -> String {
    if requested.is_empty() {
        return DEFAULT_FONT_FAMILY.to_string();
    }

    // Exact case-insensitive match among bundled fonts.
    for bf in BUNDLED_FONTS {
        if bf.family.eq_ignore_ascii_case(requested) {
            return bf.family.to_string();
        }
    }

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
