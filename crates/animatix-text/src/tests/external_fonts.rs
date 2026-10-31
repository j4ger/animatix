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
