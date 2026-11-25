use super::*;

impl LayoutPreset {
    pub const ALL: [Self; 2] = [Self::Code, Self::Animate];

    pub(crate) fn icon(self) -> &'static str {
        use egui_phosphor::regular as icons;
        match self {
            Self::Code => icons::CODE,
            Self::Animate => icons::FILM_STRIP,
        }
    }

    pub(crate) fn message_id(self) -> &'static str {
        match self {
            Self::Code => "gui-layout-write",
            Self::Animate => "gui-layout-review",
        }
    }

    pub(crate) fn description_id(self) -> &'static str {
        match self {
            Self::Code => "gui-layout-write.description",
            Self::Animate => "gui-layout-review.description",
        }
    }
}
