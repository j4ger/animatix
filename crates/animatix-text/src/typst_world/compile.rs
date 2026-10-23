#[cfg(feature = "rich-text")]
use crate::font_context::{
    DEFAULT_FONT_WEIGHT, DEFAULT_MATH_FONT_FAMILY, FontContext, collect_fallback_fonts,
    resolve_font_family,
};
#[cfg(feature = "rich-text")]
use animatix_core::error::RenderError;
#[cfg(feature = "rich-text")]
use typst::layout::Frame;
#[cfg(feature = "rich-text")]
use typst::syntax::{RootedPath, Source, VirtualPath, VirtualRoot};

#[cfg(feature = "rich-text")]
use super::world::TypstWorld;

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
