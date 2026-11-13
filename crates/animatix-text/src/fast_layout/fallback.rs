//! Fallback face list for fast-path shaping.
//!
//! Native builds contribute their compiled-in faces; web builds contribute the
//! faces registered at runtime from the external font pack. Keeping this list
//! out of `mod.rs` keeps the fast-layout entry point within the structure
//! budget while preserving one shared order for both sources.

use crate::font_context::{
    BUNDLED_FONTS, FontContext, FontFaceData, bundled_face, bundled_face_data,
    bundled_variable_face_data,
};

pub(super) fn fallback_faces(
    font_ctx: &FontContext,
    primary_family: &str,
    weight: f32,
    style: &str,
) -> Vec<FontFaceData> {
    let registered = font_ctx
        .loaded_faces()
        .into_iter()
        .filter(|face| face.family != primary_family)
        .collect::<Vec<_>>();
    let mut faces = ordered_registered_faces(font_ctx, &registered, weight, style);
    if font_ctx.include_bundled {
        faces.extend(ordered_bundled_faces(primary_family, weight, style));
    }
    faces
}

fn ordered_registered_faces(
    font_ctx: &FontContext,
    faces: &[FontFaceData],
    weight: f32,
    style: &str,
) -> Vec<FontFaceData> {
    let mut ordered = Vec::with_capacity(faces.len());
    for family in family_order(faces) {
        let preferred = font_ctx.registered_face_data(&family, weight, style);
        append_family_faces(&mut ordered, faces, &family, preferred.as_ref());
    }
    ordered
}

fn ordered_bundled_faces(primary_family: &str, weight: f32, style: &str) -> Vec<FontFaceData> {
    let bundled = BUNDLED_FONTS
        .iter()
        .filter(|face| face.family != primary_family)
        .map(bundled_face_data)
        .collect::<Vec<_>>();
    let mut ordered = Vec::with_capacity(bundled.len());
    for family in family_order(&bundled) {
        let preferred = bundled_variable_face_data(&family, style, weight)
            .or_else(|| bundled_face(&family, weight, style).map(bundled_face_data));
        append_family_faces(&mut ordered, &bundled, &family, preferred.as_ref());
    }
    ordered
}

fn family_order(faces: &[FontFaceData]) -> Vec<String> {
    let mut families = Vec::new();
    for face in faces {
        if !families.iter().any(|family: &String| family == &face.family) {
            families.push(face.family.clone());
        }
    }
    families
}

fn append_family_faces(
    output: &mut Vec<FontFaceData>,
    faces: &[FontFaceData],
    family: &str,
    preferred: Option<&FontFaceData>,
) {
    if let Some(preferred) = preferred {
        output.push(preferred.clone());
    }
    for face in faces.iter().filter(|face| face.family == family) {
        if !preferred.is_some_and(|preferred| same_face(face, preferred)) {
            output.push(face.clone());
        }
    }
}

fn same_face(left: &FontFaceData, right: &FontFaceData) -> bool {
    match (left.id, right.id) {
        (Some(left), Some(right)) => left == right,
        _ => {
            left.face_index == right.face_index && std::sync::Arc::ptr_eq(&left.bytes, &right.bytes)
        },
    }
}
