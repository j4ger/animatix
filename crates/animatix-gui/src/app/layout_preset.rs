use super::*;

impl LayoutPreset {
    pub const ALL: [Self; 4] = [Self::Code, Self::Animate, Self::Inspect, Self::Focus];

    pub fn label(self) -> &'static str {
        match self {
            Self::Animate => "Review",
            Self::Code => "Write",
            Self::Inspect => "Inspect",
            Self::Focus => "Focus",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        use egui_phosphor::regular as icons;
        match self {
            Self::Code => icons::CODE,
            Self::Animate => icons::FILM_STRIP,
            Self::Inspect => icons::MAGNIFYING_GLASS,
            Self::Focus => icons::CORNERS_OUT,
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Code => "Write · give the code editor more room",
            Self::Animate => "Review · balance preview and timeline",
            Self::Inspect => "Inspect · give sampled properties more room",
            Self::Focus => "Focus · show only the preview",
        }
    }
}
