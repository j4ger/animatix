#[cfg(feature = "rich-text")]
use animatix_core::error::RenderError;
#[cfg(feature = "rich-text")]
use kurbo::Shape;
use kurbo::{Affine, BezPath, Point};

#[cfg(feature = "rich-text")]
use crate::CompiledText;
use crate::font_context::{BUNDLED_FONTS, FontContext, bundled_face, resolve_font_family};
#[cfg(feature = "rich-text")]
use crate::font_context::{
    DEFAULT_FONT_WEIGHT, DEFAULT_MATH_FONT_FAMILY, VARIABLE_FONT_FACES, collect_fallback_fonts,
};
#[cfg(feature = "rich-text")]
use crate::paint_to_rgba;
use crate::{TextKind, TextPath};

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

pub(crate) struct PathBuilder(pub BezPath);

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
/// Build a TypstWorld with bundled fonts + any requested system fonts loaded.
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
    pub(crate) fn cache_bits(&self) -> Vec<u32> {
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
