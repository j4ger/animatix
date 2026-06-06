pub use super::types::TextPath;
use super::error::RenderError;
use kurbo::{Affine, BezPath, Point, Shape};

use typst::foundations::{Bytes, Datetime};
use typst::layout::{Frame, FrameItem, Transform};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::World;
use typst::{Library, LibraryExt};

// ─────────────────────────────────────────────────────────────
// Persistent font database (FontContext)
// ─────────────────────────────────────────────────────────────

/// Owns a persistent `fontdb::Database` to avoid redundant font scanning (~45-60ms per call).
///
/// Create one `FontContext` early and share it throughout the build pipeline.
#[derive(Clone, Debug)]
pub struct FontContext {
    /// The underlying font database.
    db: fontdb::Database,
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new()
    }
}

impl FontContext {
    /// Create a new font context with system fonts loaded.
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self { db }
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

    /// Return a sorted list of all available font family names from the system.
    pub fn families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .db
            .faces()
            .filter_map(|face| {
                self.db
                    .face(face.id)?
                    .families
                    .first()
                    .map(|(name, _)| name.clone())
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
fn build_world(source: Source, extra_fonts: &[&str], font_ctx: &FontContext) -> Result<TypstWorld, RenderError> {
    let mut fonts = Vec::with_capacity(BUNDLED_FONTS.len() + extra_fonts.len());
    let mut book = FontBook::new();

    // Load bundled fonts
    for bf in BUNDLED_FONTS {
        let font = Font::new(Bytes::new(bf.data), 0)
            .ok_or_else(|| RenderError::TextCompilation(format!("Failed to load bundled font: {}", bf.family)))?;
        book.push(font.info().clone());
        fonts.push(font);
    }

    // Load requested system fonts via persistent FontContext
    for family in extra_fonts {
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
        self.0.quad_to(
            Point::new(x1 as f64, y1 as f64),
            Point::new(x as f64, y as f64),
        );
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
    pub fn with_fonts(source: Source, fonts: &[&str], font_ctx: &FontContext) -> Result<Self, RenderError> {
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
            Err(typst::diag::FileError::NotFound(
                id.vpath().as_rootless_path().into(),
            ))
        }
    }

    fn file(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        Err(typst::diag::FileError::NotFound(
            id.vpath().as_rootless_path().into(),
        ))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

/// Compile Typst math markup into a frame.
pub fn compile_math(math: &str, font_size: f32, color: typst::visualize::Color, font_family: &str, font_ctx: &FontContext) -> Result<Frame, RenderError> {
    let text_font = resolve_font_family(font_family, font_ctx);
    let markup = format!(
        "#set text(size: {}pt, fill: rgb(\"{}\"), font: (\"{}\", \"Fira Math\")); #show math.equation: set text(font: \"Fira Math\"); $ {} $",
        font_size,
        color.to_hex(),
        text_font,
        math
    );

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::with_fonts(source, &[&text_font, DEFAULT_MATH_FONT_FAMILY], font_ctx)?;
    let document: typst::layout::PagedDocument = typst::compile(&world).output
        .map_err(|_| RenderError::TextCompilation("failed to compile Typst math document".to_string()))?;

    Ok(document.pages[0].frame.clone())
}

/// Compile Typst markup into a frame.
pub fn compile_typst(typst_markup: &str, font_size: f32, color: typst::visualize::Color, font_family: &str, font_ctx: &FontContext) -> Result<Frame, RenderError> {
    let font = resolve_font_family(font_family, font_ctx);
    let markup = format!(
        "#set text(size: {}pt, fill: rgb(\"{}\"), font: \"{}\")\n{}",
        font_size,
        color.to_hex(),
        font,
        typst_markup
    );

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::with_fonts(source, &[&font], font_ctx)?;
    let document: typst::layout::PagedDocument = typst::compile(&world).output
        .map_err(|_| RenderError::TextCompilation("failed to compile Typst document".to_string()))?;

    Ok(document.pages[0].frame.clone())
}

/// Compile plain text into a Typst frame.
pub fn compile_text(text: &str, font_size: f32, color: typst::visualize::Color, font_family: &str, font_ctx: &FontContext) -> Result<Frame, RenderError> {
    let font = resolve_font_family(font_family, font_ctx);
    // Use Typst raw block (4 backticks) to avoid markup interpretation of user text.
    // 4-backtick delimiter handles text containing up to 3 consecutive backticks.
    // Block raw also handles newlines, which inline raw cannot.
    let escaped = text
        .replace('\', "\\\\");
    let markup = format!(
        "#set text(size: {}pt, fill: rgb(\"{}\"), font: \"{}\")\n````\n{}````",
        font_size,
        color.to_hex(),
        font,
        escaped
    );

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::with_fonts(source, &[&font], font_ctx)?;
    let document: typst::layout::PagedDocument = typst::compile(&world).output
        .map_err(|_| RenderError::TextCompilation("failed to compile Typst text document".to_string()))?;

    Ok(document.pages[0].frame.clone())
}

/// Compile code text into a Typst frame.
pub fn compile_code(code: &str, font_size: f32, color: typst::visualize::Color, font_family: &str, font_ctx: &FontContext) -> Result<Frame, RenderError> {
    let font = resolve_font_family(font_family, font_ctx);
    // Use Typst raw block (4 backticks) to avoid markup interpretation of code text.
    let escaped = code
        .replace('\\', "\\\\");
    let markup = format!(
        "#set text(size: {}pt, fill: rgb(\"{}\"), font: \"{}\")\n````\n{}````",
        font_size,
        color.to_hex(),
        font,
        escaped
    );

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::with_fonts(source, &[&font], font_ctx)?;
    let document: typst::layout::PagedDocument = typst::compile(&world).output
        .map_err(|_| RenderError::TextCompilation("failed to compile Typst code document".to_string()))?;

    Ok(document.pages[0].frame.clone())
}

/// Extract glyph paths from a Typst frame.
pub fn extract_glyphs(frame: &Frame) -> Vec<TextPath> {
    let mut glyphs = Vec::new();
    walk_frame_for_glyphs(frame, Transform::identity(), &mut glyphs);
    center_text_paths(&mut glyphs);
    glyphs
}

/// Centers text paths around the origin so that layout positioning works correctly.
/// The layout system positions children by their center point, so text needs to be
/// centered around (0, 0) for layout alignment to work properly.
pub fn center_text_paths(paths: &mut [TextPath]) {
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
            }
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
            }
            _ => {}
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
            }
            FrameItem::Shape(shape, _span) => {
                if let typst::visualize::Geometry::Curve(curve) = &shape.geometry {
                    shapes.push(ExtractedShape {
                        curve: (*curve).clone(),
                        transform,
                    });
                }
            }
            _ => {}
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Runtime text recompilation (Phase 2)
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

/// Cache key for compiled text paths.
/// Since `f32` is not `Hash`, we bit-cast it to `u32` for the key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextCacheKey {
    content: String,
    font_family: String,
    font_size_bits: u32,
    color: [u8; 4],
    kind: TextKind,
}

/// Runtime text compiler with LRU-style caching.
///
/// When text properties change at runtime (e.g. via `always` reactive blocks),
/// this service recompiles glyph paths on-demand and caches them so that
/// identical `(content, font_family, font_size, color, kind)` tuples only
/// pay compilation cost once.
#[derive(Clone, Default)]
pub struct TextCompiler {
    cache: std::collections::HashMap<TextCacheKey, std::sync::Arc<[TextPath]>>,
}

impl TextCompiler {
    /// Create a new text compiler with an empty cache.
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
        color: [f32; 4],
        kind: TextKind,
        font_ctx: &FontContext,
    ) -> Result<std::sync::Arc<[TextPath]>, RenderError> {
        let key = TextCacheKey {
            content: content.to_string(),
            font_family: font_family.to_string(),
            font_size_bits: font_size.to_bits(),
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
            let to_remove: Vec<_> = self.cache.keys()
                .take(self.cache.len() / 2)
                .cloned()
                .collect();
            for k in to_remove {
                self.cache.remove(&k);
            }
        }

        let typst_color = typst::visualize::Color::from_u8(key.color[0], key.color[1], key.color[2], key.color[3]);
        let frame = match kind {
            TextKind::Text => compile_text(content, font_size, typst_color, font_family, font_ctx)?,
            TextKind::Math => compile_math(content, font_size, typst_color, font_family, font_ctx)?,
            TextKind::Code => compile_code(content, font_size, typst_color, font_family, font_ctx)?,
            TextKind::Typst => compile_typst(content, font_size, typst_color, font_family, font_ctx)?,
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
