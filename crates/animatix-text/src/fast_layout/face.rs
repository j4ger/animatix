//! Construction of [`FastPathFace`] from parsed font bytes.
//!
//! The type itself stays in `mod.rs`; only its inherent implementation lives
//! here so the module root stays declaration-only.

use super::FastPathFace;

impl FastPathFace {
    pub fn from_data(data: &'static [u8], index: u32, family: String) -> Option<Self> {
        let ttf = ttf_parser::Face::parse(data, index).ok()?;
        let rb = rustybuzz::Face::from_slice(data, index)?;
        Some(Self {
            ttf,
            rb,
            family,
            font_data: data,
            font_index: index,
        })
    }
}
