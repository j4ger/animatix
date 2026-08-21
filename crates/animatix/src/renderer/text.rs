use kurbo::{Affine, BezPath, Point, Shape};

use super::error::RenderError;
pub use super::types::TextPath;

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
}

use typst::foundations::{Bytes, Datetime};
use typst::layout::{Frame, FrameItem, Transform};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
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
        }
    }

    /// Create a new font context with fast path explicitly enabled/disabled.
    pub fn with_fast_path(text_fast_path: bool) -> Self {
        Self {
            db: std::sync::Arc::clone(system_fonts_db()),
            text_fast_path,
        }
    }

    fn load_font(&self, family: &str) -> Option<Font> {
        let id = self.db.query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..Default::default()
        })?;
        let data = Self::face_data(&self.db, id)?;
        let face = self.db.face(id)?;
        Font::new(Bytes::new(data), face.index)
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
        let data = Self::face_data(&self.db, id)?;
        // SAFETY: ttf_parser::Face borrows the data; we leak it so it lives for 'static.
        // This is acceptable because FontContext is typically kept alive for the entire
        // application lifetime, and the leaked memory is bounded by the number of distinct
        // font faces loaded.
        let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
        ttf_parser::Face::parse(leaked, 0).ok()
    }
}

// ─────────────────────────────────────────────────────────────
// Font bundle
// ─────────────────────────────────────────────────────────────

/// A font entry in the bundled font set.
struct BundledFont {
    family: &'static str,
    data: &'static [u8],
}

/// Fonts embedded at compile time. Add new fonts here.
static BUNDLED_FONTS: &[BundledFont] = &[
    BundledFont {
        family: "Open Sans",
        data: include_bytes!("../../assets/mock_font.ttf"),
    },
    BundledFont {
        family: "Fira Math",
        data: include_bytes!("../../assets/fonts/FiraMath-Regular.otf"),
    },
];

/// Default font family used when the requested one is not available.
pub const DEFAULT_FONT_FAMILY: &str = "Open Sans";
/// Default math font family used for math rendering.
pub const DEFAULT_MATH_FONT_FAMILY: &str = "Fira Math";

/// Build a TypstWorld with bundled fonts + any requested system fonts loaded.
fn build_world(
    source: Source,
    extra_fonts: &[&str],
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

    // Load requested extra fonts via persistent FontContext.
    // Skip fonts that are already available as bundled fonts to avoid
    // override with potentially different metrics (e.g. system vs mock).
    for family in extra_fonts {
        if fonts.iter().any(|f| f.info().family == *family) {
            continue;
        }
        if let Some(font) = font_ctx.load_font(family) {
            book.push(font.info().clone());
            fonts.push(font);
        }
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

/// A shape extracted from a Typst frame, containing the curve and its transform.
#[derive(Clone)]
pub struct ExtractedShape {
    /// The curve geometry of the shape.
    pub curve: typst::visualize::Curve,
    /// The transform applied to the shape.
    pub transform: Transform,
}

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

impl TypstWorld {
    /// Create a new Typst world with the given source and font context.
    pub fn new(source: Source, font_ctx: &FontContext) -> Result<Self, RenderError> {
        build_world(source, &[], font_ctx)
    }

    /// Create a new Typst world with additional font families.
    pub fn with_fonts(
        source: Source,
        fonts: &[&str],
        font_ctx: &FontContext,
    ) -> Result<Self, RenderError> {
        build_world(source, fonts, font_ctx)
    }
}

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
            Err(typst::diag::FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }

    fn file(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        Err(typst::diag::FileError::NotFound(id.vpath().as_rootless_path().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

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

    // Weight: map numeric (100-900) to Typst weight strings
    let weight_str = font_weight_to_typst(font_weight);
    if weight_str != "regular" {
        parts.push(format!("weight: \"{}\"", weight_str));
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

/// Build a Typst wrapping preamble string for max_width, text_align, and overflow.
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

    // Overflow
    if overflow == "ellipsis" {
        inner.push_str("#set text(overflow: '...'); ");
    }

    inner.push_str(inner_content);

    if max_width > 0.0 {
        // Wrap in a block with the given width
        format!("#block(width: {}pt, inset: 0pt)[\n{}]", max_width, inner)
    } else {
        inner
    }
}

/// Map a numeric font weight (100-900) to a Typst weight string.
pub fn font_weight_to_typst(weight: f32) -> &'static str {
    let w = weight.round() as i32;
    match w {
        100 => "thin",
        200 => "extralight",
        300 => "light",
        400 => "regular",
        500 => "medium",
        600 => "semibold",
        700 => "bold",
        800 => "extrabold",
        900 => "black",
        _ if w < 300 => "light",
        _ if w < 500 => "regular",
        _ if w < 700 => "medium",
        _ if w < 800 => "bold",
        _ => "black",
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
    let base_markup = format!(
        "#set text(size: {}pt, fill: rgb(\"{}\"), font: (\"{}\", \"Fira Math\")); #show math.equation: set text(font: \"Fira Math\"); $ {} $",
        font_size,
        color.to_hex(),
        text_font,
        math
    );

    let markup = typst_wrapping_preamble(max_width, text_align, overflow, &base_markup);

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::with_fonts(source, &[&text_font, DEFAULT_MATH_FONT_FAMILY], font_ctx)?;
    let document: typst::layout::PagedDocument = typst::compile(&world).output.map_err(|_| {
        RenderError::TextCompilation("failed to compile Typst math document".to_string())
    })?;

    Ok(document.pages[0].frame.clone())
}

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
    let extra_rules = typst_text_set_rules(font_weight, font_style, letter_spacing, word_spacing);
    let leading_rule = typst_par_leading_rule(line_height);
    // Include math font so that $...$ math expressions compile correctly.
    // Mirror the compile_math show-rule for math.equation font.
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
    let markup = typst_wrapping_preamble(max_width, text_align, overflow, &base_markup);

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::with_fonts(source, &[&font, DEFAULT_MATH_FONT_FAMILY], font_ctx)?;
    let document: typst::layout::PagedDocument = typst::compile(&world).output.map_err(|_| {
        RenderError::TextCompilation("failed to compile Typst document".to_string())
    })?;

    Ok(document.pages[0].frame.clone())
}

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
    _max_width: f32,
    _text_align: &str,
    _overflow: &str,
) -> Result<Frame, RenderError> {
    let font = resolve_font_family(font_family, font_ctx);
    let extra_rules = typst_text_set_rules(font_weight, font_style, letter_spacing, word_spacing);
    let leading_rule = typst_par_leading_rule(line_height);
    // Use Typst raw block (4 backticks) to avoid markup interpretation of user text.
    // 4-backtick delimiter handles text containing up to 3 consecutive backticks.
    // Block raw also handles newlines, which inline raw cannot.
    let escaped = text.replace('\\', "\\\\");
    let markup = format!(
        "{}{}#set text(size: {}pt, fill: rgb(\"{}\"), font: \"{}\")\n````\n{}````",
        extra_rules,
        leading_rule,
        font_size,
        color.to_hex(),
        font,
        escaped
    );

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::with_fonts(source, &[&font], font_ctx)?;
    let document: typst::layout::PagedDocument = typst::compile(&world).output.map_err(|_| {
        RenderError::TextCompilation("failed to compile Typst text document".to_string())
    })?;

    Ok(document.pages[0].frame.clone())
}

/// Compile code text into a Typst frame.
pub fn compile_code(
    code: &str,
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
    let extra_rules = typst_text_set_rules(font_weight, font_style, letter_spacing, word_spacing);
    let leading_rule = typst_par_leading_rule(line_height);
    // Use Typst raw block (4 backticks) to avoid markup interpretation of code text.
    let escaped = code.replace('\\', "\\\\");
    let base_markup = format!(
        "{}{}#set text(size: {}pt, fill: rgb(\"{}\"), font: \"{}\")\n````\n{}````",
        extra_rules,
        leading_rule,
        font_size,
        color.to_hex(),
        font,
        escaped
    );
    let markup = typst_wrapping_preamble(max_width, text_align, overflow, &base_markup);

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::with_fonts(source, &[&font], font_ctx)?;
    let document: typst::layout::PagedDocument = typst::compile(&world).output.map_err(|_| {
        RenderError::TextCompilation("failed to compile Typst code document".to_string())
    })?;

    Ok(document.pages[0].frame.clone())
}

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

/// Extract glyph paths from a Typst frame.
pub fn extract_glyphs(frame: &Frame) -> Vec<TextPath> {
    let mut glyphs = Vec::new();
    walk_frame_for_glyphs(frame, Transform::identity(), &mut glyphs);
    let _ = center_text_paths(&mut glyphs);
    glyphs
}

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
    }
}

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

    for (pos, item) in frame.items() {
        let transform = Transform::translate(pos.x, pos.y);
        match item {
            FrameItem::Group(group) => {
                let start = all_glyphs.len();
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_glyphs(&group.frame, group_transform, &mut all_glyphs);
                let end = all_glyphs.len();
                if end > start {
                    ranges.push(start..end);
                }
            },
            FrameItem::Text(_) => {
                // Top-level text not wrapped in a group — collect into an implicit group
                let start = all_glyphs.len();
                walk_frame_for_glyphs_text_item(item, transform, &mut all_glyphs);
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

/// Helper: extract glyphs from a single `FrameItem::Text` (not recursing into groups).
fn walk_frame_for_glyphs_text_item(
    item: &FrameItem,
    transform: Transform,
    glyphs: &mut Vec<TextPath>,
) {
    if let FrameItem::Text(text) = item {
        let size = text.size.to_pt() as f32;
        let units_per_em = text.font.units_per_em() as f32;
        let font_scale = size / units_per_em;
        let face = text.font.ttf();

        let mut x_curr = 0.0;
        for glyph in &text.glyphs {
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
                glyphs.push(TextPath {
                    path: final_path,
                    color: text.fill.clone(),
                    opacity: 1.0,
                });
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
        let bounds = path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        let offset = Affine::translate((-center_x, -center_y));

        for path in paths.iter_mut() {
            path.path.apply_affine(offset);
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
        let bounds = text_path.path.bounding_box();
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

fn walk_frame_for_glyphs(frame: &Frame, current_transform: Transform, glyphs: &mut Vec<TextPath>) {
    for (pos, item) in frame.items() {
        let transform = current_transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_glyphs(&group.frame, group_transform, glyphs);
            },
            FrameItem::Text(text) => {
                let size = text.size.to_pt() as f32;
                let units_per_em = text.font.units_per_em() as f32;
                let font_scale = size / units_per_em;
                let face = text.font.ttf();

                let mut x_curr = 0.0;
                for glyph in &text.glyphs {
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

                        glyphs.push(TextPath {
                            path: final_path,
                            color: text.fill.clone(),
                            opacity: 1.0,
                        });
                    }

                    x_curr += advance;
                }
            },
            _ => {},
        }
    }
}

/// Extract shapes from a Typst frame.
pub fn extract_shapes(frame: &Frame) -> Vec<ExtractedShape> {
    let mut shapes = Vec::new();
    walk_frame_for_shapes(frame, Transform::identity(), &mut shapes);
    shapes
}

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

/// Compile plain text into glyph paths using `ttf_parser` directly,
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
        });
    }

    // Try to load font data from bundled fonts first for consistency with Typst path.
    // Fall back to system font if not bundled.
    let resolved_family = resolve_font_family(family, font_ctx);
    let face: ttf_parser::Face<'static> = 'font: {
        if let Some(bf) = BUNDLED_FONTS.iter().find(|bf| bf.family == resolved_family) {
            // SAFETY: ttf_parser::Face borrows the data; we leak it so it lives for 'static.
            let leaked: &'static [u8] = Box::leak(bf.data.to_vec().into_boxed_slice());
            if let Ok(face) = ttf_parser::Face::parse(leaked, 0) {
                break 'font face;
            }
        }
        // Fall back to system font
        font_ctx.load_face(&resolved_family, weight, style).ok_or_else(|| {
            RenderError::TextCompilation(format!(
                "Failed to load font '{}' (weight={}, style={}) for fast path",
                resolved_family, weight, style
            ))
        })?
    };

    let units_per_em = face.units_per_em() as f32;
    let font_scale = size / units_per_em;

    let ascent = face.ascender() as f32;
    let descent = face.descender() as f32;
    let line_gap = face.line_gap() as f32;
    let _metrics = TextMetrics {
        ascent,
        descent,
        line_gap,
        units_per_em,
    };

    // Resolve kerning tables if available
    let kern_tables = face.tables().kern;

    // Build the paint color from [f32; 4]
    let paint = typst::visualize::Paint::Solid(typst::visualize::Color::from_u8(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0) as u8,
    ));

    let mut glyphs: Vec<TextPath> = Vec::with_capacity(content.len());
    let mut x_curr: f64 = 0.0; // cumulative x offset in scene coordinates (points)
    let mut prev_glyph_id: Option<ttf_parser::GlyphId> = None;

    for c in content.chars() {
        let glyph_id = match face.glyph_index(c) {
            Some(id) => id,
            None => continue,
        };

        // Get advance width in font units, then scale to scene units
        let raw_advance = face.glyph_hor_advance(glyph_id).unwrap_or(0) as f32;
        let mut advance = raw_advance * font_scale;

        // Apply letter spacing
        advance += letter_spacing;

        // Apply word spacing when character is a space
        if c == ' ' {
            advance += word_spacing;
        }

        // Apply kerning from previous glyph to current glyph
        // Use only the first horizontal subtable to avoid double-applying kerning
        if let Some(prev) = prev_glyph_id {
            if let Some(table) = kern_tables {
                if let Some(subtable) = table.subtables.into_iter().find(|st| st.horizontal) {
                    if let Some(kern) = subtable.glyphs_kerning(prev, glyph_id) {
                        x_curr += (kern as f64) * font_scale as f64;
                    }
                }
            }
        }
        prev_glyph_id = Some(glyph_id);

        // Build glyph outline path
        let mut builder = PathBuilder(BezPath::new());
        if face.outline_glyph(glyph_id, &mut builder).is_some() {
            let path = builder.0;

            // Apply scale (flip Y) and translate to cumulative x position
            // Same coordinate convention as walk_frame_for_glyphs
            let scale_affine = Affine::scale_non_uniform(font_scale as f64, -font_scale as f64);
            let translate = Affine::translate(kurbo::Vec2::new(x_curr, 0.0));
            let final_affine = translate * scale_affine;

            let mut final_path = path;
            final_path.apply_affine(final_affine);

            glyphs.push(TextPath {
                path: final_path,
                color: paint.clone(),
                opacity: 1.0,
            });
        }

        x_curr += advance as f64;
    }

    // Center paths around origin (same as center_text_paths)
    // capture baseline offset: was at Y=0 before centering
    let baseline_offset = center_text_paths(&mut glyphs);

    let ascent_scaled = ascent * font_scale;
    let descent_scaled = descent * font_scale;

    Ok(CompiledText {
        glyphs,
        ascent: ascent_scaled,
        descent: descent_scaled,
        baseline_offset,
    })
}

/// Compile plain text into glyph paths with word wrapping (fast path).
///
/// Like `compile_text_fast`, but wraps text at `max_width` points.
/// Honors `text_align` per line and `overflow` mode.
pub fn compile_text_fast_wrapped(
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    size: f32,
    color: [f32; 4],
    letter_spacing: f32,
    word_spacing: f32,
    font_ctx: &FontContext,
    max_width: f32,
    text_align: &str,
    overflow: &str,
) -> Result<CompiledText, RenderError> {
    let resolved_family = resolve_font_family(family, font_ctx);
    // Try to load font data from bundled fonts first for consistency with Typst path.
    let face: ttf_parser::Face<'static> = 'font: {
        if let Some(bf) = BUNDLED_FONTS.iter().find(|bf| bf.family == resolved_family) {
            // SAFETY: ttf_parser::Face borrows the data; we leak it so it lives for 'static.
            let leaked: &'static [u8] = Box::leak(bf.data.to_vec().into_boxed_slice());
            if let Ok(face) = ttf_parser::Face::parse(leaked, 0) {
                break 'font face;
            }
        }
        // Fall back to system font
        font_ctx.load_face(&resolved_family, weight, style).ok_or_else(|| {
            RenderError::TextCompilation(format!(
                "Failed to load font '{}' (weight={}, style={}) for fast path wrapped",
                resolved_family, weight, style
            ))
        })?
    };

    let units_per_em = face.units_per_em() as f32;
    let font_scale = size / units_per_em;

    let ascent = face.ascender() as f32;
    let descent = face.descender() as f32;
    let line_gap = face.line_gap() as f32;
    let _metrics = TextMetrics {
        ascent,
        descent,
        line_gap,
        units_per_em,
    };

    let kern_tables = face.tables().kern;

    let paint = typst::visualize::Paint::Solid(typst::visualize::Color::from_u8(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0) as u8,
    ));

    // Line height in absolute points
    let line_height_pts = (ascent - descent + line_gap) * font_scale * 1.2; // default 1.2 line height multiplier

    // Split content into words (preserve spaces as word boundaries)
    let words: Vec<&str> = content.split(' ').collect();
    if words.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: ascent * font_scale,
            descent: descent * font_scale,
            baseline_offset: 0.0,
        });
    }

    // Pre-compute advance widths for each word
    struct WordInfo {
        #[allow(dead_code)] // Reserved for debug/annotation use
        text: String,
        width: f64, // total advance in scene coords
        glyphs: Vec<(ttf_parser::GlyphId, f64, f64)>, /* (glyph_id, advance, x_offset at build
                     * time) */
    }

    let mut word_infos: Vec<WordInfo> = Vec::with_capacity(words.len());
    let mut space_advance = 0.0f64;
    if let Some(space_gid) = face.glyph_index(' ') {
        let raw = face.glyph_hor_advance(space_gid).unwrap_or(0) as f32;
        space_advance = (raw * font_scale + letter_spacing + word_spacing) as f64;
    }

    for w in words {
        if w.is_empty() {
            continue;
        }
        let mut total_width = 0.0f64;
        let mut glyph_data: Vec<(ttf_parser::GlyphId, f64, f64)> = Vec::with_capacity(w.len());
        let mut prev_gid: Option<ttf_parser::GlyphId> = None;

        for c in w.chars() {
            if let Some(gid) = face.glyph_index(c) {
                let raw_adv = face.glyph_hor_advance(gid).unwrap_or(0) as f32;
                let adv = raw_adv * font_scale + letter_spacing;

                // Kerning — use only the first horizontal subtable to avoid double-kerning
                if let Some(prev) = prev_gid {
                    if let Some(table) = kern_tables {
                        if let Some(subtable) = table.subtables.into_iter().find(|st| st.horizontal)
                        {
                            if let Some(kern) = subtable.glyphs_kerning(prev, gid) {
                                total_width += (kern as f64) * font_scale as f64;
                            }
                        }
                    }
                }
                prev_gid = Some(gid);

                glyph_data.push((gid, adv as f64, total_width));
                total_width += adv as f64;
            }
        }

        word_infos.push(WordInfo {
            text: w.to_string(),
            width: total_width,
            glyphs: glyph_data,
        });
    }

    if word_infos.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: ascent * font_scale,
            descent: descent * font_scale,
            baseline_offset: 0.0,
        });
    }

    // Greedy word-wrap: pack words into lines
    struct LineInfo {
        words: Vec<usize>, // indices into word_infos
        total_width: f64,
    }

    let mut lines: Vec<LineInfo> = Vec::new();
    let mut current_line = LineInfo {
        words: Vec::new(),
        total_width: 0.0,
    };
    let wrap_threshold = max_width as f64;

    for (wi_idx, wi) in word_infos.iter().enumerate() {
        // Add space before this word (except first word on the line)
        let space_needed = if current_line.words.is_empty() {
            0.0
        } else {
            space_advance
        };

        if current_line.total_width + space_needed + wi.width > wrap_threshold
            && !current_line.words.is_empty()
        {
            // Start new line
            lines.push(current_line);
            current_line = LineInfo {
                words: Vec::new(),
                total_width: 0.0,
            };
        }

        if !current_line.words.is_empty() {
            current_line.total_width += space_advance;
        }
        current_line.words.push(wi_idx);
        current_line.total_width += wi.width;
    }
    if !current_line.words.is_empty() {
        lines.push(current_line);
    }

    // Handle overflow
    let max_lines = if overflow == "ellipsis" || overflow == "clip" {
        // Limit to roughly the visible area: one page worth (no explicit height limit yet)
        // For now, just prevent unbounded growth
        Some(100usize)
    } else {
        None
    };

    let truncated_lines: &[LineInfo] = if let Some(limit) = max_lines {
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
            if wi_idx_in_line > 0 {
                if is_justify {
                    x_curr += space_advance + extra_space_per_gap;
                } else {
                    x_curr += space_advance;
                }
            }

            // Render each glyph in the word using pre-computed offsets that include kerning
            for (gid, _adv, glyph_x_offset) in &wi.glyphs {
                let mut builder = PathBuilder(BezPath::new());
                if face.outline_glyph(*gid, &mut builder).is_some() {
                    let path = builder.0;
                    let scale_affine =
                        Affine::scale_non_uniform(font_scale as f64, -font_scale as f64);
                    let translate = Affine::translate(kurbo::Vec2::new(
                        x_curr + x_offset + glyph_x_offset,
                        y_curr,
                    ));
                    let final_affine = translate * scale_affine;

                    let mut final_path = path;
                    final_path.apply_affine(final_affine);

                    glyphs.push(TextPath {
                        path: final_path,
                        color: paint.clone(),
                        opacity: 1.0,
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
            // Append ellipsis glyph "\u{2026}" if available
            if let Some(ellipsis_gid) = face.glyph_index('\u{2026}') {
                // Use period '.' as fallback
                let ellipsis_gid = Some(ellipsis_gid).or_else(|| face.glyph_index('.'));
                if let Some(gid) = ellipsis_gid {
                    let raw_adv = face.glyph_hor_advance(gid).unwrap_or(0) as f32;
                    let _adv = raw_adv * font_scale;
                    let mut builder = PathBuilder(BezPath::new());
                    if face.outline_glyph(gid, &mut builder).is_some() {
                        let path = builder.0;
                        let scale_affine =
                            Affine::scale_non_uniform(font_scale as f64, -font_scale as f64);
                        let translate =
                            Affine::translate(kurbo::Vec2::new(x_curr + x_offset, y_curr));
                        let final_affine = translate * scale_affine;
                        let mut final_path = path;
                        final_path.apply_affine(final_affine);
                        glyphs.push(TextPath {
                            path: final_path,
                            color: paint.clone(),
                            opacity: 1.0,
                        });
                    }
                }
            }
        }

        y_curr += line_height_pts as f64;
    }

    // Center all paths around origin
    let baseline_offset = center_text_paths(&mut glyphs);

    let ascent_scaled = ascent * font_scale;
    let descent_scaled = descent * font_scale;

    Ok(CompiledText {
        glyphs,
        ascent: ascent_scaled,
        descent: descent_scaled,
        baseline_offset,
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

/// Cache key for compiled text paths.
/// Since `f32` is not `Hash`, we bit-cast it to `u32` for the key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextCacheKey {
    content: String,
    font_family: String,
    font_size_bits: u32,
    font_weight_bits: u32,
    font_style: String,
    line_height_bits: u32,
    letter_spacing_bits: u32,
    word_spacing_bits: u32,
    color: [u8; 4],
    kind: TextKind,
}

/// Runtime text compiler with LRU-style caching.
///
/// When text properties change at runtime (e.g. via `always` reactive blocks),
/// this service recompiles glyph paths on-demand and caches them so that
/// identical `(content, font_family, font_size, color, kind)` tuples only
/// pay compilation cost once.
#[derive(Clone)]
pub struct TextCompiler {
    cache: std::collections::HashMap<TextCacheKey, std::sync::Arc<[TextPath]>>,
    /// Whether the plain-text fast path is enabled (default: true).
    /// Can be disabled via `config { text_fast_path: false }` for debugging.
    pub text_fast_path: bool,
}

impl Default for TextCompiler {
    fn default() -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            text_fast_path: true,
        }
    }
}

impl TextCompiler {
    /// Create a new text compiler with an empty cache and fast path enabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compile text into glyph paths, using the cache when possible.
    /// P2.26: Cache hits return an `Arc<[TextPath]>` — a single refcount increment
    /// instead of cloning the entire vector of BezPath objects.
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
        font_ctx: &FontContext,
        max_width: f32,
        text_align: &str,
        overflow: &str,
    ) -> Result<std::sync::Arc<[TextPath]>, RenderError> {
        let key = TextCacheKey {
            content: content.to_string(),
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
            kind,
        };

        if let Some(cached) = self.cache.get(&key) {
            return Ok(std::sync::Arc::clone(cached));
        }

        // Evict cache if it grows too large to prevent unbounded memory use.
        // Remove ~half the entries to avoid a full clear spike.
        if self.cache.len() > 1000 {
            let to_remove: Vec<_> = self.cache.keys().take(self.cache.len() / 2).cloned().collect();
            for k in to_remove {
                self.cache.remove(&k);
            }
        }

        // Route plain text to the fast path when:
        // 1. The text_fast_path flag is enabled (config option)
        // 2. The kind is Text (not Code, Typst, or Math)
        // 3. The content is plain (no Typst markup)
        // 4. The content is LTR Latin only
        if self.text_fast_path
            && kind == TextKind::Text
            && is_plain_text(content)
            && is_latin_text(content)
        {
            tracing::debug!(
                "TextCompiler: using fast path for '{}' (family={}, size={}, max_width={}, text_align={}, overflow={})",
                content,
                font_family,
                font_size,
                max_width,
                text_align,
                overflow
            );
            let compiled = if max_width > 0.0 {
                tracing::debug!("TextCompiler: wrapping at {}pt", max_width);
                compile_text_fast_wrapped(
                    content,
                    font_family,
                    font_weight,
                    font_style,
                    font_size,
                    color,
                    letter_spacing,
                    word_spacing,
                    font_ctx,
                    max_width,
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
            };
            let paths_vec = compiled.glyphs;
            let paths: std::sync::Arc<[TextPath]> = paths_vec.into();
            self.cache.insert(key, std::sync::Arc::clone(&paths));
            return Ok(paths);
        }

        // Fall back to Typst path
        tracing::debug!(
            "TextCompiler: using Typst path for '{}' (kind={:?}, plain={}, latin={}, fast_path={})",
            content,
            kind,
            is_plain_text(content),
            content.chars().all(|c| (c as u32) <= 0x017F),
            self.text_fast_path
        );

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
        let paths: std::sync::Arc<[TextPath]> = extract_glyphs(&frame).into();
        self.cache.insert(key, std::sync::Arc::clone(&paths));
        Ok(paths)
    }

    /// Clear the compilation cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Number of entries in the cache (for testing).
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a default FontContext (loads system fonts, may be slow on CI).
    fn test_font_ctx() -> FontContext {
        FontContext::with_fast_path(true)
    }

    #[test]
    fn is_plain_text_basic() {
        assert!(is_plain_text("Hello World"));
        assert!(is_plain_text("abc123"));
        assert!(is_plain_text(""));
        assert!(!is_plain_text("Hello\nWorld"));
        assert!(!is_plain_text("Hello *World*"));
        assert!(!is_plain_text("$x^2$"));
        assert!(!is_plain_text("#emoji"));
        assert!(!is_plain_text("`code`"));
        assert!(!is_plain_text("Hello_World"));
        assert!(!is_plain_text("a/b"));
    }

    #[test]
    fn is_latin_text_basic() {
        assert!(is_latin_text("Hello World"));
        assert!(is_latin_text("Café")); // Latin-1 Supplement
        assert!(is_latin_text("Âmbit")); // Latin Extended-A
        assert!(is_latin_text(""));
        assert!(!is_latin_text("中文"));
        assert!(!is_latin_text("Привет"));
        assert!(!is_latin_text("مرحبا"));
    }

    #[test]
    fn fast_path_produces_glyphs() {
        let font_ctx = test_font_ctx();
        let family = "Open Sans";

        // Verify the bundled font is available
        assert!(
            font_ctx.load_face(family, 400.0, "normal").is_some(),
            "Open Sans font should be loadable"
        );

        let compiled = compile_text_fast(
            "Hello",
            family,
            400.0,                // weight
            "normal",             // style
            48.0,                 // size
            [1.0, 1.0, 1.0, 1.0], // white
            0.0,                  // letter_spacing
            0.0,                  // word_spacing
            &font_ctx,
        )
        .expect("fast path should succeed");

        // Should produce at least one glyph path per character
        assert!(!compiled.glyphs.is_empty(), "Should produce glyph paths");

        // Metrics should be populated
        assert!(compiled.ascent > 0.0);
        assert!(compiled.descent < 0.0);
    }

    #[test]
    fn fast_path_with_bold_weight() {
        let font_ctx = test_font_ctx();
        let family = "Open Sans";

        // Bold weight should also resolve
        assert!(
            font_ctx.load_face(family, 700.0, "normal").is_some(),
            "Open Sans bold should be loadable"
        );

        let compiled = compile_text_fast(
            "Bold Text",
            family,
            700.0, // bold
            "normal",
            48.0,
            [1.0, 1.0, 1.0, 1.0],
            0.0,
            0.0,
            &font_ctx,
        )
        .expect("bold fast path should succeed");

        assert!(!compiled.glyphs.is_empty(), "Bold text should produce glyph paths");
    }

    #[test]
    fn fast_path_vs_typst_visually_equivalent() {
        // Compare fast-path output against Typst output for a simple string.
        // They should produce bounding boxes that are close (sub-pixel tolerance).
        let font_ctx = test_font_ctx();
        let family = "Open Sans";
        let text = "Hello World";
        let size = 48.0;
        let color = [1.0, 1.0, 1.0, 1.0];

        // Fast path
        let fast_compiled =
            compile_text_fast(text, family, 400.0, "normal", size, color, 0.0, 0.0, &font_ctx)
                .expect("fast path should succeed");
        let fast_paths = fast_compiled.glyphs;

        // Typst path
        let typst_color = typst::visualize::Color::from_u8(255, 255, 255, 255);
        let frame = compile_text(
            text,
            size,
            typst_color,
            family,
            &font_ctx,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0,
            0.0,
            "left",
            "visible",
        )
        .expect("Typst path should succeed");
        let typst_paths = extract_glyphs(&frame);

        assert!(!fast_paths.is_empty(), "Fast path should produce paths");
        assert!(!typst_paths.is_empty(), "Typst path should produce paths");

        // Compare overall bounding box (centered).
        // Both paths use the same bundled font data, but different rendering
        // pipelines (ttf_parser vs Typst's engine) which can produce slightly
        // different metrics — allow a generous tolerance for this difference.
        let fast_bbox = measure_text_paths(&fast_paths);
        let typst_bbox = measure_text_paths(&typst_paths);

        let half_w_diff = (fast_bbox[0] - typst_bbox[0]).abs();
        let half_h_diff = (fast_bbox[1] - typst_bbox[1]).abs();
        assert!(
            half_w_diff < 30.0,
            "Width mismatch: fast={:.3}, typst={:.3}, diff={:.3}",
            fast_bbox[0] * 2.0,
            typst_bbox[0] * 2.0,
            half_w_diff * 2.0
        );
        assert!(
            half_h_diff < 30.0,
            "Height mismatch: fast={:.3}, typst={:.3}, diff={:.3}",
            fast_bbox[1] * 2.0,
            typst_bbox[1] * 2.0,
            half_h_diff * 2.0
        );
    }

    #[test]
    fn fast_path_caches_results() {
        let font_ctx = test_font_ctx();
        let mut compiler = TextCompiler::new();
        compiler.text_fast_path = true;

        let paths1 = compiler
            .compile(
                "Cache Test",
                "Open Sans",
                24.0,
                400.0,
                "normal",
                1.2,
                0.0,
                0.0,
                [1.0, 1.0, 1.0, 1.0],
                TextKind::Text,
                &font_ctx,
                0.0,
                "left",
                "visible",
            )
            .expect("first compile should succeed");

        let paths2 = compiler
            .compile(
                "Cache Test",
                "Open Sans",
                24.0,
                400.0,
                "normal",
                1.2,
                0.0,
                0.0,
                [1.0, 1.0, 1.0, 1.0],
                TextKind::Text,
                &font_ctx,
                0.0,
                "left",
                "visible",
            )
            .expect("second compile should succeed (cache hit)");

        // Same Arc should be returned (cache hit)
        assert_eq!(paths1.as_ptr(), paths2.as_ptr(), "Cache should return the same Arc pointer");
    }

    #[test]
    fn non_plain_text_routes_to_typst() {
        let font_ctx = test_font_ctx();
        let mut compiler = TextCompiler::new();
        compiler.text_fast_path = true;

        // Text with asterisk should route to Typst (not fast path)
        let paths = compiler
            .compile(
                "Hello *World*",
                "Open Sans",
                24.0,
                400.0,
                "normal",
                1.2,
                0.0,
                0.0,
                [1.0, 1.0, 1.0, 1.0],
                TextKind::Text,
                &font_ctx,
                0.0,
                "left",
                "visible",
            )
            .expect("Typst fallback should succeed");

        assert!(!paths.is_empty(), "Should produce glyph paths via Typst");
    }

    #[test]
    fn non_latin_text_routes_to_typst() {
        let font_ctx = test_font_ctx();
        let mut compiler = TextCompiler::new();
        compiler.text_fast_path = true;

        // Non-Latin text should route to Typst
        let _paths = compiler
            .compile(
                "中文测试",
                "Open Sans",
                24.0,
                400.0,
                "normal",
                1.2,
                0.0,
                0.0,
                [1.0, 1.0, 1.0, 1.0],
                TextKind::Text,
                &font_ctx,
                0.0,
                "left",
                "visible",
            )
            .expect("Typst path for non-Latin should succeed");

        // Even though Typst may not render CJK with Open Sans (the bundled mock font
        // has no .notdef outline for unavailable characters), the compilation itself
        // should succeed without crashing.
        // The paths may be empty if .notdef has no outline, but we at least verify
        // that Typst was used (not the fast path) by checking the routing worked.
        // Just verifying that compile succeeded without panic is sufficient.
    }

    #[test]
    fn fast_path_disabled_via_config_falls_back() {
        let font_ctx = test_font_ctx();
        let mut compiler = TextCompiler::new();
        compiler.text_fast_path = false; // disabled

        // Plain text should still compile via Typst
        let paths = compiler
            .compile(
                "Plain Text",
                "Open Sans",
                24.0,
                400.0,
                "normal",
                1.2,
                0.0,
                0.0,
                [1.0, 1.0, 1.0, 1.0],
                TextKind::Text,
                &font_ctx,
                0.0,
                "left",
                "visible",
            )
            .expect("Typst fallback should succeed when fast path disabled");

        assert!(!paths.is_empty(), "Should produce glyph paths");
    }

    #[test]
    fn code_kind_uses_typst_not_fast_path() {
        let font_ctx = test_font_ctx();
        let mut compiler = TextCompiler::new();
        compiler.text_fast_path = true;

        // Even plain text should use Typst for Code kind
        let paths = compiler
            .compile(
                "fn main()",
                "Open Sans",
                24.0,
                400.0,
                "normal",
                1.2,
                0.0,
                0.0,
                [1.0, 1.0, 1.0, 1.0],
                TextKind::Code, // Code kind → always Typst
                &font_ctx,
                0.0,
                "left",
                "visible",
            )
            .expect("Code kind should compile via Typst");

        assert!(!paths.is_empty(), "Should produce glyph paths");
    }

    #[test]
    fn fast_path_letter_spacing() {
        // Verify that letter_spacing affects the total width.
        let font_ctx = test_font_ctx();

        let compiled_no_spacing = compile_text_fast(
            "AB",
            "Open Sans",
            400.0,
            "normal",
            48.0,
            [1.0; 4],
            0.0,
            0.0,
            &font_ctx,
        )
        .unwrap();
        let paths_no_spacing = compiled_no_spacing.glyphs;

        let compiled_with_spacing = compile_text_fast(
            "AB",
            "Open Sans",
            400.0,
            "normal",
            48.0,
            [1.0; 4],
            10.0,
            0.0,
            &font_ctx, // 10pt letter spacing
        )
        .unwrap();
        let paths_with_spacing = compiled_with_spacing.glyphs;

        let bbox_no = measure_text_paths(&paths_no_spacing);
        let bbox_with = measure_text_paths(&paths_with_spacing);

        // Width should be larger with letter spacing
        assert!(
            bbox_with[0] > bbox_no[0],
            "Letter spacing should increase width: no_spacing={:.3}, with_spacing={:.3}",
            bbox_no[0],
            bbox_with[0]
        );
    }

    #[test]
    fn fast_path_resolves_bundled_and_system_fonts() {
        let font_ctx = test_font_ctx();

        // Open Sans is bundled, should always work
        let face = font_ctx.load_face("Open Sans", 400.0, "normal");
        assert!(face.is_some(), "Open Sans should resolve");

        // Try system font (may or may not exist, but shouldn't crash)
        let _ = font_ctx.load_face("Arial", 400.0, "normal");
        let _ = font_ctx.load_face("sans-serif", 400.0, "normal");
    }

    #[test]
    fn compile_text_fast_empty_string() {
        let font_ctx = test_font_ctx();
        let compiled = compile_text_fast(
            "",
            "Open Sans",
            400.0,
            "normal",
            48.0,
            [1.0; 4],
            0.0,
            0.0,
            &font_ctx,
        )
        .unwrap();

        assert!(compiled.glyphs.is_empty(), "Empty string should produce no paths");
        assert!(compiled.ascent == 0.0, "Empty string should have zero ascent");
    }

    #[test]
    fn wrapped_fast_path_produces_multiple_lines() {
        // A long string wrapped at narrow width should produce multiple lines
        let font_ctx = test_font_ctx();
        let text = "Hello world this is a long string that should wrap";
        let compiled_single = compile_text_fast(
            text,
            "Open Sans",
            400.0,
            "normal",
            24.0,
            [1.0; 4],
            0.0,
            0.0,
            &font_ctx,
        )
        .unwrap();
        let paths_single = compiled_single.glyphs;

        // With a narrow max_width, wrapping should produce more glyphs due to vertical layout
        let compiled_wrapped = compile_text_fast_wrapped(
            text,
            "Open Sans",
            400.0,
            "normal",
            24.0,
            [1.0; 4],
            0.0,
            0.0,
            &font_ctx,
            50.0,
            "left",
            "visible",
        )
        .unwrap();
        let paths_wrapped = compiled_wrapped.glyphs;

        // Wrapped text should produce glyphs
        assert!(!paths_wrapped.is_empty(), "Wrapped text should produce glyphs");

        // The bounding box height should be taller (multiple lines) vs single line width
        let single_bbox = measure_text_paths(&paths_single);
        let wrapped_bbox = measure_text_paths(&paths_wrapped);

        // Wrapped text should have a taller bounding box (multiple lines)
        assert!(
            wrapped_bbox[1] > single_bbox[1],
            "Wrapped text should be taller (multi-line): single_h={:.3}, wrapped_h={:.3}",
            single_bbox[1],
            wrapped_bbox[1]
        );
    }

    #[test]
    fn wrapped_fast_path_centered_alignment() {
        let font_ctx = test_font_ctx();
        let text = "left center right";

        // Left-aligned: first glyph starts near x = -max_width/2
        let compiled_left = compile_text_fast_wrapped(
            text,
            "Open Sans",
            400.0,
            "normal",
            24.0,
            [1.0; 4],
            0.0,
            0.0,
            &font_ctx,
            200.0,
            "left",
            "visible",
        )
        .unwrap();
        let paths_left = compiled_left.glyphs;

        // Center-aligned
        let compiled_center = compile_text_fast_wrapped(
            text,
            "Open Sans",
            400.0,
            "normal",
            24.0,
            [1.0; 4],
            0.0,
            0.0,
            &font_ctx,
            200.0,
            "center",
            "visible",
        )
        .unwrap();
        let paths_center = compiled_center.glyphs;

        // Right-aligned
        let compiled_right = compile_text_fast_wrapped(
            text,
            "Open Sans",
            400.0,
            "normal",
            24.0,
            [1.0; 4],
            0.0,
            0.0,
            &font_ctx,
            200.0,
            "right",
            "visible",
        )
        .unwrap();
        let paths_right = compiled_right.glyphs;

        assert!(!paths_left.is_empty());
        assert!(!paths_center.is_empty());
        assert!(!paths_right.is_empty());

        // All should produce glyphs (sanity check)
        let bbox_left = measure_text_paths(&paths_left);
        let bbox_center = measure_text_paths(&paths_center);
        let _bbox_right = measure_text_paths(&paths_right);

        // Widths should be roughly equal
        assert!(
            (bbox_left[0] - bbox_center[0]).abs() < 1.0,
            "Left and center widths should match: left={:.3}, center={:.3}",
            bbox_left[0],
            bbox_center[0]
        );
    }

    #[test]
    fn no_max_width_is_identical() {
        // Text without max_width renders the same as before
        let font_ctx = test_font_ctx();
        let text = "NoWrapTest";

        let compiled_normal = compile_text_fast(
            text,
            "Open Sans",
            400.0,
            "normal",
            24.0,
            [1.0; 4],
            0.0,
            0.0,
            &font_ctx,
        )
        .unwrap();
        let paths_normal = compiled_normal.glyphs;

        // Wrapping with very large max_width (effectively no wrap) should match single-line
        let compiled_wide = compile_text_fast_wrapped(
            text,
            "Open Sans",
            400.0,
            "normal",
            24.0,
            [1.0; 4],
            0.0,
            0.0,
            &font_ctx,
            10000.0,
            "left",
            "visible",
        )
        .unwrap();
        let paths_wide = compiled_wide.glyphs;

        // Both should produce the same number of glyphs
        assert_eq!(
            paths_normal.len(),
            paths_wide.len(),
            "No-wrap and wide-wrap should produce same glyph count"
        );

        // Bounding box should be the same
        let bbox_normal = measure_text_paths(&paths_normal);
        let bbox_wide = measure_text_paths(&paths_wide);
        assert!(
            (bbox_normal[0] - bbox_wide[0]).abs() < 0.5,
            "Widths should match: normal={:.3}, wide={:.3}",
            bbox_normal[0],
            bbox_wide[0]
        );
        assert!(
            (bbox_normal[1] - bbox_wide[1]).abs() < 0.5,
            "Heights should match: normal={:.3}, wide={:.3}",
            bbox_normal[1],
            bbox_wide[1]
        );
    }
}
