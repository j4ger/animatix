//! Interface-only messages. Keep authored source and stable UI identities outside this catalog.
use egui::{Context, Id};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Language {
    #[default]
    English,
    SimplifiedChinese,
}

impl Language {
    pub(crate) const ALL: [Self; 2] = [Self::English, Self::SimplifiedChinese];
    pub(crate) const NAMES: [&str; 2] = ["English", "简体中文"];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
        }
    }

    pub(crate) fn from_key(key: &str) -> Self {
        if key == "zh-CN" {
            Self::SimplifiedChinese
        } else {
            Self::English
        }
    }
}

fn language_id() -> Id {
    Id::unique("animatix.interface_language")
}

pub(crate) fn set_language(ctx: &Context, language: Language) {
    ctx.data_mut(|data| data.insert_temp(language_id(), language));
}

pub(crate) fn current_language(ctx: &Context) -> Language {
    ctx.data(|data| data.get_temp(language_id())).unwrap_or_default()
}
