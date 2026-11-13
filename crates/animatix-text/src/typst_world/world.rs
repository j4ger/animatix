#[cfg(feature = "rich-text")]
use crate::font_context::{BUNDLED_FONT_FACES, BUNDLED_FONTS, FontContext, VARIABLE_FONT_FACES};
#[cfg(feature = "rich-text")]
use animatix_core::error::RenderError;
#[cfg(feature = "rich-text")]
use typst::foundations::{Bytes, Datetime};
#[cfg(feature = "rich-text")]
use typst::syntax::{FileId, Source};
#[cfg(feature = "rich-text")]
use typst::text::{Font, FontBook};
#[cfg(feature = "rich-text")]
use typst::utils::LazyHash;
#[cfg(feature = "rich-text")]
use typst::{Library, LibraryExt, World};

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
/// Build a TypstWorld with bundled fonts + any requested system fonts loaded.
fn build_world(
    source: Source,
    extra_fonts: &[&str],
    fallback_fonts: &[Font],
    font_ctx: &FontContext,
) -> Result<TypstWorld, RenderError> {
    let mut fonts = Vec::with_capacity(BUNDLED_FONTS.len() + extra_fonts.len());
    let mut book = FontBook::new();
    let overlay_families = font_ctx.registered_family_names();
    let is_overlay_family =
        |family: &str| overlay_families.iter().any(|overlay| overlay.eq_ignore_ascii_case(family));

    // Load bundled fonts. The parsed faces are process-wide; only the
    // refcounted handles are cloned into this world. An explicitly registered
    // family replaces its bundled counterpart for this compilation.
    for (bf, cached) in BUNDLED_FONTS
        .iter()
        .zip(BUNDLED_FONT_FACES.iter())
        .filter(|(bf, _)| font_ctx.include_bundled && !is_overlay_family(bf.family))
    {
        let font = cached.clone().ok_or_else(|| {
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
    for font in VARIABLE_FONT_FACES
        .iter()
        .filter(|font| font_ctx.include_bundled && !is_overlay_family(&font.info().family))
    {
        book.push(font.info().clone());
        fonts.push(font.clone());
    }

    // Every explicitly registered face is part of this prepared font set,
    // even when generated Typst source (for example str.from-unicode) hides
    // its glyphs from the source-text fallback scan. Keep registration order
    // reversed so exact metadata ties prefer the latest registered face.
    for font in font_ctx.registered_typst_fonts() {
        book.push(font.info().clone());
        fonts.push(font);
    }

    // Load requested extra fonts via persistent FontContext. Existing bundled
    // families keep their default metrics unless the caller registered an
    // explicit overlay. The context returns all registered overlay faces, while
    // unregistered system families use four queried emphasis faces.
    let mut handled_families: Vec<String> = Vec::new();
    for family in extra_fonts {
        if handled_families.iter().any(|loaded| loaded.eq_ignore_ascii_case(family)) {
            continue;
        }
        handled_families.push((*family).to_string());
        let is_overlay = is_overlay_family(family);
        if is_overlay {
            continue;
        }
        if !is_overlay && fonts.iter().any(|f| f.info().family.eq_ignore_ascii_case(family)) {
            continue;
        }
        for font in font_ctx.load_font_emphasis_faces(family) {
            if !is_overlay
                && fonts.iter().any(|f| {
                    f.info().family == font.info().family
                        && f.info().variant.weight == font.info().variant.weight
                        && f.info().variant.style == font.info().variant.style
                })
            {
                continue;
            }
            book.push(font.info().clone());
            fonts.push(font);
        }
    }

    // Append glyph-fallback faces covering characters the requested families
    // cannot render (CJK, Cyrillic, ...). One representative face per family.
    for font in fallback_fonts {
        if fonts.iter().any(|f| f.info().family.eq_ignore_ascii_case(&font.info().family)) {
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

#[cfg(feature = "rich-text")]
impl TypstWorld {
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
