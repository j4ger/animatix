use std::fs;
use std::path::Path;

use crate::{
    FontContext, TextCompileRequest, TextKind, TextStyle, compile_text_cached,
    compile_typst_grouped_cached,
};

fn font_bytes(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts").join(name);
    fs::read(path).expect("font fixture is readable")
}

fn scale_advance_widths(mut bytes: Vec<u8>, scale: f32) -> Vec<u8> {
    let table_count = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    let mut hhea = None;
    let mut hmtx = None;
    for index in 0..table_count {
        let entry = 12 + index * 16;
        let tag = &bytes[entry..entry + 4];
        let offset = u32::from_be_bytes(bytes[entry + 8..entry + 12].try_into().unwrap()) as usize;
        match tag {
            b"hhea" => hhea = Some(offset),
            b"hmtx" => hmtx = Some(offset),
            _ => {},
        }
    }
    let hhea = hhea.expect("fixture has hhea table");
    let hmtx = hmtx.expect("fixture has hmtx table");
    let metrics = u16::from_be_bytes([bytes[hhea + 34], bytes[hhea + 35]]) as usize;
    for index in 0..metrics {
        let offset = hmtx + index * 4;
        let advance = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        let scaled = (f32::from(advance) * scale).round().clamp(1.0, f32::from(u16::MAX)) as u16;
        bytes[offset..offset + 2].copy_from_slice(&scaled.to_be_bytes());
    }
    bytes
}

fn rich_width(font_ctx: &FontContext, weight: f32) -> f32 {
    let mut style = TextStyle::new("Open Sans", 48.0);
    style.font_weight = weight;
    let request =
        TextCompileRequest::new(TextKind::Typst, "Overlay isolation measures advances", style);
    let compiled = compile_text_cached(&request, font_ctx).expect("rich text compiles");
    compiled.half_size[0] * 2.0
}

#[test]
fn rich_font_overlays_replace_bundled_family_and_remain_context_local() {
    let baseline = FontContext::new();
    let mut first = FontContext::new();
    first.load_font_bytes(scale_advance_widths(font_bytes("OpenSans-Regular.ttf"), 1.5));
    let mut second = FontContext::new();
    second.load_font_bytes(scale_advance_widths(font_bytes("OpenSans-Regular.ttf"), 2.0));
    let mut stacked = FontContext::new();
    stacked.load_font_bytes(scale_advance_widths(font_bytes("OpenSans-Regular.ttf"), 1.5));
    stacked.load_font_bytes(scale_advance_widths(font_bytes("OpenSans-Regular.ttf"), 2.0));

    let baseline_width = rich_width(&baseline, 400.0);
    let first_width = rich_width(&first, 400.0);
    let second_width = rich_width(&second, 400.0);
    let stacked_width = rich_width(&stacked, 400.0);
    assert!(
        first_width > baseline_width * 1.35,
        "overlay should replace bundled metrics: {baseline_width} vs {first_width}"
    );
    assert!(
        second_width > first_width * 1.25,
        "contexts should retain their own overlay bytes: {first_width} vs {second_width}"
    );
    assert!(
        (stacked_width - second_width).abs() < 0.01,
        "the most recently registered exact-tie face should win: {stacked_width} vs {second_width}"
    );
    assert_eq!(
        rich_width(&first, 400.0),
        first_width,
        "another overlay must not replace a cached document's font"
    );
}

#[test]
fn rich_registered_family_keeps_variable_face_alongside_static_faces() {
    let mut font_ctx = FontContext::new();
    for name in [
        "OpenSans-Regular.ttf",
        "OpenSans-Bold.ttf",
        "OpenSans-Variable.ttf",
    ] {
        font_ctx.load_font_bytes(font_bytes(name));
    }

    let faces = font_ctx.load_font_emphasis_faces("Open Sans");
    assert!(
        faces.iter().any(|font| {
            ttf_parser::Face::parse(font.data().as_slice(), font.index())
                .is_ok_and(|face| !face.variation_axes().is_empty())
        }),
        "registered variable face must be included with static emphasis faces"
    );

    let regular = rich_width(&font_ctx, 400.0);
    let medium = rich_width(&font_ctx, 550.0);
    let bold = rich_width(&font_ctx, 700.0);
    assert!(
        regular < medium && medium < bold,
        "variable weight should remain continuous: {regular}, {medium}, {bold}"
    );
}

#[test]
fn rich_generated_unicode_can_use_any_registered_typst_fallback_face() {
    use crate::{compile_typst, extract_glyphs_with_metrics};

    let mut font_ctx = FontContext::empty();
    for name in [
        "OpenSans-Regular.ttf",
        "OpenSans-Bold.ttf",
        "FiraMath-Regular.otf",
        "NotoSansSC-Common.ttf",
    ] {
        font_ctx.load_font_bytes(font_bytes(name));
    }

    let compile = |markup: &str| {
        let frame = compile_typst(
            markup,
            48.0,
            typst::visualize::Color::from_u8(255, 255, 255, 255),
            "Open Sans",
            &font_ctx,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0,
            400.0,
            "left",
            "visible",
        )
        .expect("rich text compiles");
        extract_glyphs_with_metrics(&frame)
    };

    let generated = compile("#str.from-unicode(20013)");
    let literal = compile("中");
    assert_eq!(generated.glyphs.len(), 1, "generated CJK should emit one glyph");
    assert_eq!(literal.glyphs.len(), 1, "literal CJK should emit one glyph");
    assert_eq!(
        generated.glyphs[0].path.elements(),
        literal.glyphs[0].path.elements(),
        "generated Unicode with the primary Open Sans font must resolve to the same outline as literal text"
    );

    let content_selected = compile("#set text(font: \"Noto Sans SC\"); #str.from-unicode(20013)");
    assert_eq!(
        content_selected.glyphs[0].path.elements(),
        literal.glyphs[0].path.elements(),
        "a family selected inside Typst content must be available to generated Unicode"
    );
}

#[test]
fn rich_registered_typst_faces_keep_family_order_and_latest_face_first() {
    let mut font_ctx = FontContext::empty();
    font_ctx.load_font_bytes(font_bytes("OpenSans-Regular.ttf"));
    font_ctx.load_font_bytes(font_bytes("NotoSansSC-Common.ttf"));
    font_ctx.load_font_bytes(font_bytes("OpenSans-Bold.ttf"));

    let faces = font_ctx.registered_typst_fonts();
    let families: Vec<_> = faces.iter().map(|font| font.info().family.as_str()).collect();
    assert_eq!(
        families,
        ["Open Sans", "Open Sans", "Noto Sans SC"],
        "family order follows first registration, while faces within a family prefer the latest registration"
    );
    assert_eq!(faces[0].info().variant.weight.to_number(), 700);
    assert_eq!(faces[1].info().variant.weight.to_number(), 400);
}

#[test]
fn rich_generated_missing_glyphs_error_but_empty_and_ignorable_text_succeed() {
    let mut font_ctx = FontContext::empty();
    font_ctx.load_font_bytes(font_bytes("OpenSans-Regular.ttf"));
    let compile = |content: &str| {
        let request =
            TextCompileRequest::new(TextKind::Typst, content, TextStyle::new("Open Sans", 24.0));
        compile_text_cached(&request, &font_ctx)
    };

    let error = compile("#str.from-unicode(20013)").err().expect("unsupported scalar must fail");
    let message = error.to_string();
    assert!(message.contains("U+4E2D"), "missing codepoint should be identified: {message}");
    assert!(message.contains("prepared font"), "error should explain the remedy: {message}");

    let nested = compile("#box()[#str.from-unicode(20013)]")
        .err()
        .expect("missing glyphs inside a nested frame must fail");
    assert!(nested.to_string().contains("U+4E2D"));

    for content in ["", " \n", "#str.from-unicode(8203)"] {
        assert!(compile(content).is_ok(), "valid empty/ignorable content failed: {content:?}");
    }

    let grouped = compile_typst_grouped_cached(
        "#box()[#str.from-unicode(20013)]",
        24.0,
        [1.0; 4],
        "Open Sans",
        &font_ctx,
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
    )
    .err()
    .expect("grouped Typst compilation must report missing generated glyphs");
    assert!(grouped.to_string().contains("U+4E2D"));
}

#[test]
fn fast_text_fallback_uses_registered_regular_and_bold_faces_for_missing_primary_glyphs() {
    let regular = font_bytes("OpenSans-Regular.ttf");
    let bold = font_bytes("OpenSans-Bold.ttf");
    let math = font_bytes("FiraMath-Regular.otf");
    fn face(bytes: &[u8]) -> ttf_parser::Face<'_> {
        ttf_parser::Face::parse(bytes, 0).expect("font fixture parses")
    }
    let regular_face = face(&regular);
    let bold_face = face(&bold);
    let math_face = face(&math);
    let missing = (0x00A0..=0x2FFF)
        .filter_map(char::from_u32)
        .find(|ch| {
            ch.is_alphabetic()
                && regular_face.glyph_index(*ch).is_some()
                && bold_face.glyph_index(*ch).is_some()
                && math_face.glyph_index(*ch).is_none()
        })
        .expect("fixture faces share a letter that Fira Math does not cover");

    let mut font_ctx = FontContext::empty();
    font_ctx.load_font_bytes(regular.clone());
    font_ctx.load_font_bytes(bold.clone());
    font_ctx.load_font_bytes(math);
    let content = missing.to_string();
    let ordered_faces = crate::fast_layout::resolve_faces(
        &content,
        "Fira Math",
        700.0,
        "normal",
        &font_ctx,
        "fallback regression test",
    )
    .expect("primary and fallback faces resolve");
    let regular_index = ordered_faces
        .iter()
        .position(|face| face.bytes.as_ref() == regular.as_slice())
        .expect("regular face remains available as a coverage fallback");
    let bold_index = ordered_faces
        .iter()
        .position(|face| face.bytes.as_ref() == bold.as_slice())
        .expect("bold face remains available as the requested fallback");
    assert!(bold_index < regular_index, "requested bold face should be tried first");

    let compile_fallback = |weight| {
        let mut style = TextStyle::new("Fira Math", 48.0);
        style.font_weight = weight;
        let request = TextCompileRequest::new(TextKind::Text, &content, style);
        let compiled = compile_text_cached(&request, &font_ctx).expect("fallback letter compiles");
        let layout = compiled.hardware_layout.as_ref().expect("fast path hardware layout");
        assert!(!compiled.paths.is_empty(), "fallback letter must have an outline");
        assert!(compiled.paths[0].bounds.width() > 0.0);
        let fallback = layout
            .runs
            .iter()
            .find(|run| {
                run.font_data.as_ref() == regular.as_slice()
                    || run.font_data.as_ref() == bold.as_slice()
            })
            .expect("glyph run should use a registered Open Sans face");
        assert_eq!(fallback.glyphs.len(), 1, "expected one fallback glyph");
        (fallback.font_data.to_vec(), compiled.paths[0].bounds)
    };

    let (regular_used, regular_bounds) = compile_fallback(400.0);
    let (bold_used, bold_bounds) = compile_fallback(700.0);
    let selected_weight = |bytes: &[u8]| face(bytes).weight().to_number();
    assert_eq!(selected_weight(&regular_used), 400, "weight 400 must use regular fallback");
    assert_eq!(
        selected_weight(&bold_used),
        700,
        "weight 700 selected face weight {} for missing-primary letter U+{:04X}; expected the registered bold face; bounds={bold_bounds:?}",
        selected_weight(&bold_used),
        missing as u32,
    );
    assert_ne!(
        regular_bounds, bold_bounds,
        "fallback outlines should reflect the selected face"
    );
}
