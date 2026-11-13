//! Compile-time feature requirements declared by built-in primitives.
//!
//! This is deliberately a small bit vocabulary rather than a scene manifest:
//! the builder and the browser loader consume the same bits, and the catalog is
//! the single place that assigns them to primitives. New optional engine
//! packages append a bit; existing values never change.

/// Features a primitive can require from an engine package.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FeatureMask(u32);

impl FeatureMask {
    /// No optional engine feature required.
    pub const NONE: Self = Self(0);
    /// Typst-backed rich text (`Code`, `Math`, `Typst`).
    pub const RICH_TEXT: Self = Self(1 << 0);
    /// Raster image decoding (`Image`).
    pub const IMAGE_DECODE: Self = Self(1 << 1);
    /// SVG asset loading (`Svg`).
    pub const SVG: Self = Self(1 << 2);
    /// Raster export encoding, explicitly requested by an export operation.
    pub const RASTER_ENCODE: Self = Self(1 << 3);
    /// Unknown / extension primitive: preparation must diagnose this bit.
    pub const UNKNOWN: Self = Self(1 << 31);

    /// The raw bit pattern crossing the wasm boundary.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Build a mask from a raw boundary value.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// True when every bit in `required` is present in `self`.
    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    /// Union of two masks.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// True when no feature is required.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for FeatureMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for FeatureMask {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::FeatureMask;

    #[test]
    fn contains_is_bitwise() {
        let both = FeatureMask::RICH_TEXT | FeatureMask::SVG;
        assert!(both.contains(FeatureMask::RICH_TEXT));
        assert!(both.contains(FeatureMask::SVG));
        assert!(!both.contains(FeatureMask::IMAGE_DECODE));
        assert!(both.contains(FeatureMask::NONE));
    }

    #[test]
    fn unknown_is_distinct() {
        assert!(!FeatureMask::RICH_TEXT.contains(FeatureMask::UNKNOWN));
        assert_eq!(FeatureMask::from_bits(FeatureMask::UNKNOWN.bits()), FeatureMask::UNKNOWN);
    }
}
