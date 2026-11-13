mod cache;

#[cfg(feature = "rich-text")]
use cache::cached_typst_font;
#[cfg(feature = "rich-text")]
pub(crate) use cache::collect_fallback_fonts;
use cache::{cached_face_data, probe_char_face};

use std::collections::HashMap;
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
    pub(crate) include_bundled: bool,
    /// Content fingerprint of the immutable base database inherited by this
    /// context; overlays are folded into `font_set_id` separately.
    base_font_digest: [u8; 32],
    /// Content identity for the ordered font set visible to this context.
    pub(crate) font_set_id: FontSetId,
}

/// Stable content identity for one ordered font environment.
///
/// This intentionally excludes fontdb's process-local face ids: isolated font
/// overlays can assign the same id to different bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontSetId(pub [u8; 32]);

/// Owned font bytes and the index of a face inside a font collection.
#[derive(Clone, Debug)]
pub struct FontFaceData {
    pub family: String,
    pub bytes: std::sync::Arc<[u8]>,
    pub face_index: u32,
    pub id: Option<fontdb::ID>,
}

/// Validate that bytes contain at least one font face supported by fontdb.
///
/// Parsing happens in a temporary database so validation cannot mutate a
/// process-wide font context or affect any registered font set.
pub fn validate_supported_font_data(bytes: &[u8]) -> Result<(), &'static str> {
    let mut database = fontdb::Database::new();
    database.load_font_data(bytes.to_vec());
    if database.faces().next().is_some() {
        Ok(())
    } else {
        Err("font data contains no supported faces")
    }
}

impl FontSetId {
    fn for_context(
        db: &fontdb::Database,
        registered_faces: &[fontdb::ID],
        base_font_digest: [u8; 32],
        include_bundled: bool,
    ) -> Self {
        use sha2::{Digest, Sha256};

        let mut hash = Sha256::new();
        hash.update(
            b"animatix-font-set-v3\0fallback:registered-overlay-then-bundled\0variation:wght-ital-slnt\0",
        );
        hash.update([u8::from(include_bundled)]);
        if include_bundled {
            hash.update(bundle_font_fingerprint());
        }
        hash.update(base_font_digest);

        for id in registered_faces {
            let Some(face) = db.face(*id) else { continue };
            let family = face.families.first().map(|(name, _)| name.as_bytes()).unwrap_or_default();
            hash.update((family.len() as u64).to_le_bytes());
            hash.update(family);
            hash.update(face.index.to_le_bytes());
            hash.update(face.weight.0.to_le_bytes());
            hash.update([match face.style {
                fontdb::Style::Normal => 0,
                fontdb::Style::Italic => 1,
                fontdb::Style::Oblique => 2,
            }]);
            if let Some(bytes) = FontContext::face_data(db, *id) {
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(Sha256::digest(bytes));
            }
        }
        Self(hash.finalize().into())
    }

    fn database_fingerprint(db: &fontdb::Database) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        let mut source_digests = HashMap::new();
        hash.update(b"animatix-font-database-content-v1\0");
        for face in db.faces() {
            let Some(info) = db.face(face.id) else {
                continue;
            };
            let family = info.families.first().map(|(name, _)| name.as_bytes()).unwrap_or_default();
            hash.update((family.len() as u64).to_le_bytes());
            hash.update(family);
            hash.update(info.index.to_le_bytes());
            hash.update(info.weight.0.to_le_bytes());
            hash.update([match info.style {
                fontdb::Style::Normal => 0,
                fontdb::Style::Italic => 1,
                fontdb::Style::Oblique => 2,
            }]);
            let (source, _) = db.face_source(face.id).expect("enumerated face has a source");
            let key = match source {
                fontdb::Source::Binary(data) => format!("binary:{:p}", Arc::as_ptr(&data)),
                fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
                    format!("file:{}", path.display())
                },
            };
            // Collections expose many faces with the same source. Hash their
            // bytes once, while still recording every face and its ordering.
            let digest = source_digests.entry(key).or_insert_with(|| {
                FontContext::face_data(db, face.id)
                    .map(|bytes| (bytes.len() as u64, <[u8; 32]>::from(Sha256::digest(bytes))))
            });
            if let Some((length, digest)) = digest {
                hash.update(length.to_le_bytes());
                hash.update(digest);
            }
        }
        hash.finalize().into()
    }
}

fn bundle_font_fingerprint() -> [u8; 32] {
    static FINGERPRINT: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();
    *FINGERPRINT.get_or_init(|| {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        // Bundle order is fallback order; identify each face by its content,
        // metadata and collection index rather than fontdb's local id.
        for face in BUNDLED_FONTS {
            hash.update((face.family.len() as u64).to_le_bytes());
            hash.update(face.family.as_bytes());
            hash.update(face.weight.to_bits().to_le_bytes());
            hash.update(face.style.as_bytes());
            hash.update(0u32.to_le_bytes());
            hash.update(Sha256::digest(face.data));
        }
        for face in VARIABLE_FONTS {
            hash.update((face.family.len() as u64).to_le_bytes());
            hash.update(face.family.as_bytes());
            hash.update(face.weight.to_bits().to_le_bytes());
            hash.update(face.style.as_bytes());
            hash.update(0u32.to_le_bytes());
            hash.update(Sha256::digest(face.data));
        }
        hash.finalize().into()
    })
}

/// Shared owned copies of bundled bytes for hardware layouts. Static byte
/// arrays cannot be represented as `Arc<[u8]>` without copying on every
/// compile, so materialize each one exactly once.
pub(crate) static SHARED_BUNDLED_FONT_DATA: std::sync::LazyLock<Vec<Arc<[u8]>>> =
    std::sync::LazyLock::new(|| BUNDLED_FONTS.iter().map(|face| Arc::from(face.data)).collect());

pub(crate) static SHARED_VARIABLE_FONT_DATA: std::sync::LazyLock<Vec<Arc<[u8]>>> =
    std::sync::LazyLock::new(|| VARIABLE_FONTS.iter().map(|face| Arc::from(face.data)).collect());

pub(crate) fn bundled_face_data(face: &'static BundledFont) -> FontFaceData {
    let index = BUNDLED_FONTS
        .iter()
        .position(|candidate| std::ptr::eq(candidate, face))
        .expect("bundled face is a member of BUNDLED_FONTS");
    FontFaceData {
        family: face.family.to_string(),
        bytes: Arc::clone(&SHARED_BUNDLED_FONT_DATA[index]),
        face_index: 0,
        // Bundled face ids are local to this ordered bundle. The text cache
        // additionally includes the bundle content in FontSetId.
        id: None,
    }
}

pub(crate) fn bundled_variable_face_data(
    family: &str,
    style: &str,
    _weight: f32,
) -> Option<FontFaceData> {
    let want_italic = matches!(style, "italic" | "oblique");
    for (index, face) in VARIABLE_FONTS.iter().enumerate() {
        let face_italic = matches!(face.style, "italic" | "oblique");
        if !face.family.eq_ignore_ascii_case(family) || face_italic != want_italic {
            continue;
        }
        return Some(FontFaceData {
            family: face.family.to_string(),
            bytes: Arc::clone(&SHARED_VARIABLE_FONT_DATA[index]),
            face_index: 0,
            id: None,
        });
    }
    None
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

fn system_font_base_digest() -> [u8; 32] {
    static DIGEST: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();
    *DIGEST.get_or_init(|| FontSetId::database_fingerprint(system_fonts_db()))
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new()
    }
}

impl FontContext {
    /// Create an isolated font context with no system or bundled faces.
    ///
    /// This is intended for deterministic callers that provide their complete
    /// font set explicitly (for example, cross-target text parity probes).
    pub fn empty() -> Self {
        let db = std::sync::Arc::new(fontdb::Database::new());
        let base_font_digest = FontSetId::database_fingerprint(&db);
        let font_set_id = FontSetId::for_context(&db, &[], base_font_digest, false);
        Self {
            db,
            registered_faces: Vec::new(),
            include_bundled: false,
            base_font_digest,
            font_set_id,
        }
    }

    /// Create a new font context with system fonts loaded (shared process-wide).
    pub fn new() -> Self {
        let db = std::sync::Arc::clone(system_fonts_db());
        let base_font_digest = system_font_base_digest();
        let font_set_id =
            FontSetId::for_context(&db, &[], base_font_digest, cfg!(feature = "bundled-fonts"));
        Self {
            db,
            registered_faces: Vec::new(),
            include_bundled: cfg!(feature = "bundled-fonts"),
            base_font_digest,
            font_set_id,
        }
    }

    /// Identity of this context's fonts and fallback ordering.
    pub fn font_set_id(&self) -> FontSetId {
        self.font_set_id
    }

    #[cfg(test)]
    pub(crate) fn with_database(db: std::sync::Arc<fontdb::Database>) -> Self {
        let base_font_digest = FontSetId::database_fingerprint(&db);
        let font_set_id =
            FontSetId::for_context(&db, &[], base_font_digest, cfg!(feature = "bundled-fonts"));
        Self {
            db,
            registered_faces: Vec::new(),
            include_bundled: cfg!(feature = "bundled-fonts"),
            base_font_digest,
            font_set_id,
        }
    }

    #[cfg(feature = "rich-text")]
    /// Load the bold/italic emphasis faces of a family into the Typst world.
    ///
    /// `with_fonts_and_fallback` loads only the regular face, so `*bold*`,
    /// `_italic_`, and `font_weight`/`font_style` variants fell back to regular.
    /// Returns regular + bold + italic + bold-italic faces, empty if none.
    pub(crate) fn load_font_emphasis_faces(&self, family: &str) -> Vec<Font> {
        // Explicitly registered fonts are an overlay, not another source to
        // merge with the host database. Return every registered face in reverse
        // registration order: Typst retains the first equally ranked face, which
        // matches the fast path's latest-registration-wins policy for exact ties.
        let registered: Vec<Font> = self
            .registered_faces
            .iter()
            .rev()
            .filter_map(|id| {
                let face = self.db.face(*id)?;
                if !face.families.iter().any(|(name, _)| name.eq_ignore_ascii_case(family)) {
                    return None;
                }
                let data = Self::face_data(&self.db, *id)?;
                Font::new(Bytes::new(data), face.index)
            })
            .collect();
        if !registered.is_empty() {
            return registered;
        }

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

    #[cfg(feature = "rich-text")]
    pub(crate) fn registered_typst_fonts(&self) -> Vec<Font> {
        let mut families: Vec<(String, Vec<Font>)> = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        for id in &self.registered_faces {
            if !seen_ids.insert(*id) {
                continue;
            }
            let Some(font) = cached_typst_font(self, *id) else {
                continue;
            };
            let family = font.info().family.clone();
            let position = families.iter().position(|(name, _)| name.eq_ignore_ascii_case(&family));
            let group = match position {
                Some(position) => &mut families[position].1,
                None => {
                    families.push((family, Vec::new()));
                    &mut families.last_mut().expect("just pushed a family").1
                },
            };
            group.push(font);
        }

        families
            .into_iter()
            .flat_map(|(_, mut faces)| {
                faces.reverse();
                faces
            })
            .collect()
    }

    #[cfg(feature = "rich-text")]
    pub(crate) fn registered_family_names(&self) -> Vec<String> {
        let mut families = Vec::new();
        for id in &self.registered_faces {
            if let Some(face) = self.db.face(*id) {
                for (name, _) in &face.families {
                    if !families.iter().any(|existing: &String| existing.eq_ignore_ascii_case(name))
                    {
                        families.push(name.clone());
                    }
                }
            }
        }
        families
    }

    pub(crate) fn has_family(&self, family: &str) -> bool {
        (self.include_bundled
            && BUNDLED_FONTS.iter().any(|font| font.family.eq_ignore_ascii_case(family)))
            || self
                .db
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
        if self.include_bundled {
            names.extend(BUNDLED_FONTS.iter().map(|font| font.family.to_string()));
        }
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
    /// batch pays it once and computes the font-set content identity once instead
    /// of once per face. Empty batches are a no-op and do not invalidate the
    /// memoized text compilation cache.
    pub fn load_fonts<I>(&mut self, fonts: I)
    where
        I: IntoIterator<Item = Vec<u8>>,
    {
        self.load_font_data(fonts.into_iter().map(Arc::<[u8]>::from));
    }

    /// Register shared font bytes without an extra copy. The database still
    /// builds its own face records; the Arc keeps source data alive for every
    /// service that borrows or shares the registered face.
    pub fn load_font_data<I>(&mut self, fonts: I)
    where
        I: IntoIterator<Item = Arc<[u8]>>,
    {
        let mut db = (*self.db).clone();
        let mut loaded = Vec::new();
        for bytes in fonts {
            let source: std::sync::Arc<dyn AsRef<[u8]> + Send + Sync> = std::sync::Arc::new(bytes);
            let ids = db.load_font_source(fontdb::Source::Binary(source));
            loaded.extend(ids);
        }
        if !loaded.is_empty() {
            self.registered_faces.extend(loaded);
            self.db = std::sync::Arc::new(db);
            self.font_set_id = FontSetId::for_context(
                &self.db,
                &self.registered_faces,
                self.base_font_digest,
                self.include_bundled,
            );
        }
    }

    /// Register one in-memory font. See [`Self::load_fonts`].
    pub fn load_font_bytes(&mut self, bytes: Vec<u8>) {
        self.load_fonts(std::iter::once(bytes));
    }

    /// Load owned data for a queried face; callers parse it only while the
    /// returned Arc is alive, so font bytes never need to be leaked.
    pub fn load_face(&self, family: &str, weight: f32, style: &str) -> Option<FontFaceData> {
        self.load_face_data(family, weight, style)
    }

    /// Load the raw face bytes and face index for a queried font.
    pub fn load_face_data(&self, family: &str, weight: f32, style: &str) -> Option<FontFaceData> {
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
        let face = self.db.face(id)?;
        Some(FontFaceData {
            family: family.to_string(),
            bytes: cached_face_data(self, id)?,
            face_index: face.index,
            id: Some(id),
        })
    }

    /// Prefer an explicitly registered family over the process defaults.
    /// Variable faces keep the same continuous weight behavior on native/web.
    pub(crate) fn registered_face_data(
        &self,
        family: &str,
        weight: f32,
        style: &str,
    ) -> Option<FontFaceData> {
        let italic = matches!(style, "italic" | "oblique");
        self.registered_faces
            .iter()
            .rev()
            .filter_map(|id| {
                let info = self.db.face(*id)?;
                if !info.families.iter().any(|(name, _)| name.eq_ignore_ascii_case(family)) {
                    return None;
                }
                let data = FontFaceData {
                    family: family.into(),
                    bytes: cached_face_data(self, *id)?,
                    face_index: info.index,
                    id: Some(*id),
                };
                let face = ttf_parser::Face::parse(&data.bytes, data.face_index).ok()?;
                let wrong_style = face.is_italic() != italic;
                let axis = face
                    .variation_axes()
                    .into_iter()
                    .find(|axis| axis.tag == ttf_parser::Tag::from_bytes(b"wght"));
                let distance = if let Some(axis) = axis {
                    (weight - weight.clamp(axis.min_value, axis.max_value)).abs()
                } else {
                    (face.weight().to_number() as f32 - weight).abs()
                };
                Some(((wrong_style, distance), data))
            })
            .min_by(|(left, _), (right, _)| {
                left.0.cmp(&right.0).then_with(|| left.1.total_cmp(&right.1))
            })
            .map(|(_, data)| data)
    }

    /// Find a system fallback only for characters absent from prepared faces.
    /// The probe and face bytes are bounded, font-environment-specific caches.
    pub(crate) fn fallback_face_for_char(&self, ch: char) -> Option<FontFaceData> {
        let id = probe_char_face(self, ch)?;
        let info = self.db.face(id)?;
        Some(FontFaceData {
            family: info.families.first()?.0.clone(),
            bytes: cached_face_data(self, id)?,
            face_index: info.index,
            id: Some(id),
        })
    }

    pub(crate) fn fallback_face_for_cluster(&self, chars: &[char]) -> Option<FontFaceData> {
        let id = cache::probe_cluster_face(self, chars)?;
        let info = self.db.face(id)?;
        Some(FontFaceData {
            family: info.families.first()?.0.clone(),
            bytes: cached_face_data(self, id)?,
            face_index: info.index,
            id: Some(id),
        })
    }

    /// Faces registered through [`Self::load_fonts`], in registration order.
    /// The fast path uses these as fallback when no bundled static faces are
    /// compiled in (the web font-pack build).
    pub(crate) fn loaded_faces(&self) -> Vec<FontFaceData> {
        self.registered_faces
            .iter()
            .filter_map(|id| {
                let face = self.db.face(*id)?;
                let family = face.families.first()?.0.clone();
                let data = cached_face_data(self, *id)?;
                Some(FontFaceData {
                    family,
                    bytes: data,
                    face_index: face.index,
                    id: Some(*id),
                })
            })
            .collect()
    }
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
    // assets/fonts/README.md). Static faces remain available to the rich
    // compiler; ordinary text instances VARIABLE_FONTS consistently.
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
    // (~220 KB). Ordinary text also supports italic in every build.
    #[cfg(feature = "bundled-fonts")]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Bold.ttf"),
        weight: 700.0,
        style: "normal",
    },
    #[cfg(feature = "bundled-fonts")]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-Italic.ttf"),
        weight: 400.0,
        style: "italic",
    },
    #[cfg(feature = "bundled-fonts")]
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../assets/fonts/OpenSans-BoldItalic.ttf"),
        weight: 700.0,
        style: "italic",
    },
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
    for bf in BUNDLED_FONTS.iter().filter(|_| font_ctx.include_bundled) {
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
    let mut families: Vec<String> = if font_ctx.include_bundled {
        BUNDLED_FONTS.iter().map(|bf| bf.family.to_string()).collect()
    } else {
        Vec::new()
    };
    families.extend(font_ctx.families());
    families.sort();
    families.dedup();
    families
}

#[cfg(test)]
mod supported_font_data_tests {
    use super::validate_supported_font_data;

    #[test]
    fn validates_supported_faces_without_accepting_empty_or_corrupt_data() {
        let valid = include_bytes!("../assets/fonts/OpenSans-Regular.ttf");
        assert!(validate_supported_font_data(valid).is_ok());
        assert!(validate_supported_font_data(&[]).is_err());
        assert!(validate_supported_font_data(b"<!doctype html><html>not a font</html>").is_err());
        assert!(validate_supported_font_data(&valid[..valid.len() / 2]).is_err());
    }
}
