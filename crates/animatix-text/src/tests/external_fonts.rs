//! Coverage for builds that do not compile font bytes into the library.
//!
//! Web packages use this mode: the player registers the external font pack
//! before a scene compiles. The test keeps a regular and a CJK face in the
//! same runtime database so it exercises both the primary fast path and the
//! loaded-face fallback list.

use std::fs;
use std::path::Path;

use crate::{FontContext, compile_text_fast};

fn load_face(ctx: &mut FontContext, name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts").join(name);
    ctx.load_font_bytes(fs::read(path).expect("vendored font is readable"));
}

#[test]
fn loaded_fonts_cover_fast_path_and_fallback() {
    let mut ctx = FontContext::new();
    load_face(&mut ctx, "OpenSans-Regular.ttf");
    load_face(&mut ctx, "OpenSans-Bold.ttf");
    load_face(&mut ctx, "NotoSansSC-Common.ttf");

    let paths = compile_text_fast(
        "Hello 中文",
        "Open Sans",
        400.0,
        "normal",
        24.0,
        [1.0; 4],
        0.0,
        0.0,
        &ctx,
    )
    .expect("external font set compiles");
    assert!(!paths.glyphs.is_empty(), "both Latin and CJK glyphs must render");

    let bold =
        compile_text_fast("Bold", "Open Sans", 700.0, "normal", 24.0, [1.0; 4], 0.0, 0.0, &ctx)
            .expect("external bold face resolves");
    assert!(!bold.glyphs.is_empty());
}

#[test]
fn runtime_variable_font_matches_continuous_native_coordinates() {
    let mut ctx = FontContext::with_database(std::sync::Arc::new(fontdb::Database::new()));
    for name in [
        "OpenSans-Regular.ttf",
        "OpenSans-Bold.ttf",
        "OpenSans-Variable.ttf",
    ] {
        load_face(&mut ctx, name);
    }
    let shape = |weight| {
        compile_text_fast(
            "Variable width",
            "Open Sans",
            weight,
            "normal",
            48.0,
            [1.0; 4],
            0.0,
            0.0,
            &ctx,
        )
        .unwrap()
    };
    let regular = shape(400.0);
    let medium = shape(550.0);
    let bold = shape(700.0);
    let width = |text: &crate::CompiledText| crate::measure_text_paths(&text.glyphs)[0];
    assert!(width(&regular) < width(&medium));
    assert!(width(&medium) < width(&bold));
    let coords = |text: &crate::CompiledText| {
        text.hardware_layout.as_ref().unwrap().runs[0].normalized_coords.clone()
    };
    assert!(!coords(&medium).is_empty());
    assert_ne!(coords(&regular), coords(&medium));
    assert_ne!(coords(&medium), coords(&bold));
}

#[test]
fn fractional_weight_prefers_exact_variable_face_over_nearby_static_overlay() {
    let mut ctx = FontContext::with_database(std::sync::Arc::new(fontdb::Database::new()));
    load_face(&mut ctx, "OpenSans-Variable.ttf");
    // A later registration wins exact ties, but must not round a fractional
    // weight distance into a tie with the variable face's exact coordinates.
    load_face(&mut ctx, "OpenSans-Regular.ttf");
    let regular = ctx.registered_face_data("Open Sans", 400.0, "normal").unwrap();
    let fractional = ctx.registered_face_data("Open Sans", 400.5, "normal").unwrap();
    let variable = |data: &crate::FontFaceData| {
        !ttf_parser::Face::parse(&data.bytes, data.face_index)
            .unwrap()
            .variation_axes()
            .is_empty()
    };
    assert!(!variable(&regular));
    assert!(variable(&fractional));
}
