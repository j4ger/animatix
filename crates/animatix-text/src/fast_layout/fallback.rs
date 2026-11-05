//! Fallback face list for fast-path shaping.
//!
//! Native builds contribute their compiled-in faces; web builds contribute the
//! faces registered at runtime from the external font pack. Keeping this list
//! out of `mod.rs` keeps the fast-layout entry point within the structure
//! budget while preserving one shared order for both sources.

use crate::font_context::{BUNDLED_FONTS, FontContext};

use super::FastPathFace;

pub(super) fn fallback_faces(font_ctx: &FontContext, primary_family: &str) -> Vec<FastPathFace> {
    let mut faces = Vec::new();
    for bf in BUNDLED_FONTS {
        if bf.family != primary_family
            && let Some(face) = FastPathFace::from_data(bf.data, 0, bf.family.to_string())
        {
            faces.push(face);
        }
    }
    for loaded in font_ctx.loaded_faces() {
        if loaded.family != primary_family
            && let Some(face) = FastPathFace::from_data(loaded.data, loaded.index, loaded.family)
        {
            faces.push(face);
        }
    }
    faces
}
