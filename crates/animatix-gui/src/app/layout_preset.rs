use super::*;

impl LayoutPreset {
    pub const ALL: [Self; 4] = [Self::Animate, Self::Code, Self::Inspect, Self::Focus];

    pub fn label(self) -> &'static str {
        match self {
            Self::Animate => "Review",
            Self::Code => "Write",
            Self::Inspect => "Inspect",
            Self::Focus => "Focus",
        }
    }
}
