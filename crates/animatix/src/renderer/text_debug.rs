use super::text::{ExtractedGlyph, ExtractedShape};
use std::fmt;

impl fmt::Debug for ExtractedGlyph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ExtractedGlyph")
    }
}
impl fmt::Debug for ExtractedShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ExtractedShape")
    }
}
