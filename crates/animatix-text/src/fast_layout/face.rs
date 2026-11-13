//! Construction of [`FastPathFace`] from parsed font bytes.
//!
//! The type itself stays in `mod.rs`; only its inherent implementation lives
//! here so the module root stays declaration-only.

use crate::font_context::FontFaceData;

use super::FastPathFace;

impl<'a> FastPathFace<'a> {
    pub fn from_data(source: &'a FontFaceData, weight: f32, style: &str) -> Option<Self> {
        let mut ttf = ttf_parser::Face::parse(&source.bytes, source.face_index).ok()?;
        let mut rb = rustybuzz::Face::from_slice(&source.bytes, source.face_index)?;
        let wght = ttf_parser::Tag::from_bytes(b"wght");
        let ital = ttf_parser::Tag::from_bytes(b"ital");
        let slnt = ttf_parser::Tag::from_bytes(b"slnt");
        let want_italic = matches!(style, "italic" | "oblique");
        let axes: Vec<_> = ttf
            .variation_axes()
            .into_iter()
            .map(|axis| (axis.tag, axis.min_value, axis.max_value))
            .collect();
        let mut variations = Vec::new();
        for (tag, min_value, max_value) in axes {
            let value = if tag == wght {
                Some(weight.clamp(min_value, max_value))
            } else if tag == ital {
                Some(if want_italic { 1.0 } else { 0.0 })
            } else if tag == slnt && want_italic {
                Some(-12.0)
            } else {
                None
            };
            if let Some(value) = value {
                let value = value.clamp(min_value, max_value);
                ttf.set_variation(tag, value)?;
                variations.push(rustybuzz::Variation { tag, value });
            }
        }
        rb.set_variations(&variations);
        let normalized_coords =
            ttf.variation_coordinates().iter().map(|coord| coord.get()).collect();
        Some(Self {
            ttf,
            rb,
            family: source.family.clone(),
            source,
            normalized_coords,
        })
    }
}
