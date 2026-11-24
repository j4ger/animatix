use super::*;

impl LayoutPreset {
    pub const ALL: [Self; 2] = [Self::Code, Self::Animate];

    pub fn label(self) -> &'static str {
        match self {
            Self::Animate => "Review",
            Self::Code => "Write",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        use egui_phosphor::regular as icons;
        match self {
            Self::Code => icons::CODE,
            Self::Animate => icons::FILM_STRIP,
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Code => "Write · give the code editor more room",
            Self::Animate => "Review · balance preview and timeline",
        }
    }
}
