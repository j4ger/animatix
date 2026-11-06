use super::*;

impl LayoutPreset {
    pub const ALL: [Self; 4] = [Self::Animate, Self::Code, Self::Inspect, Self::Focus];

    pub fn label(self) -> &'static str {
        match self {
            Self::Animate => "Animate",
            Self::Code => "Code",
            Self::Inspect => "Inspect",
            Self::Focus => "Focus",
        }
    }
}
