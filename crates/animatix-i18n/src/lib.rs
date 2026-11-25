//! Embedded interface translations for Rust frontends.

use std::sync::OnceLock;

pub use fluent_bundle::{FluentArgs, FluentValue};
use fluent_bundle::{FluentResource, concurrent::FluentBundle};
use unic_langid::LanguageIdentifier;

const EN: &str = include_str!("../locales/en/gui.ftl");
const ZH_HANS: &str = include_str!("../locales/zh-Hans/gui.ftl");

type Bundle = FluentBundle<FluentResource>;

/// The language of a display context.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// The canonical English UI.
    #[default]
    English,
    /// The Simplified Chinese UI.
    SimplifiedChinese,
}

impl Language {
    /// Languages offered by the interface, in display order.
    pub const ALL: [Self; 2] = [Self::English, Self::SimplifiedChinese];

    /// Autonyms for the language switcher.
    pub const NAMES: [&str; 2] = ["English", "简体中文"];

    /// Stable value used by existing GUI settings.
    pub const fn key(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
        }
    }

    /// Resolve a saved language key, accepting the Fluent locale identifier too.
    pub fn from_key(key: &str) -> Self {
        match key {
            "zh-CN" | "zh-Hans" => Self::SimplifiedChinese,
            _ => Self::English,
        }
    }

    /// Format a message or a message attribute (using "message.attribute").
    ///
    /// English is the fallback for a missing value or failed format.
    pub fn format(self, key: &str, args: Option<&FluentArgs<'_>>) -> Option<String> {
        let primary = self.bundle();
        let english = Self::English.bundle();
        format_with_fallback(primary, english, key, args)
    }

    fn bundle(self) -> &'static Bundle {
        static ENGLISH: OnceLock<Bundle> = OnceLock::new();
        static CHINESE: OnceLock<Bundle> = OnceLock::new();
        match self {
            Self::English => ENGLISH.get_or_init(|| load("en", EN)),
            Self::SimplifiedChinese => CHINESE.get_or_init(|| load("zh-Hans", ZH_HANS)),
        }
    }
}

fn format_with_fallback(
    primary: &Bundle,
    english: &Bundle,
    key: &str,
    args: Option<&FluentArgs<'_>>,
) -> Option<String> {
    let (id, attribute) = match key.split_once('.') {
        Some((id, attribute)) => (id, Some(attribute)),
        None => (key, None),
    };
    let format = |bundle: &Bundle| {
        let message = bundle.get_message(id)?;
        let pattern = match attribute {
            Some(attribute) => message.get_attribute(attribute)?.value(),
            None => message.value()?,
        };
        let mut errors = Vec::new();
        let result = bundle.format_pattern(pattern, args, &mut errors);
        errors.is_empty().then(|| result.into_owned())
    };
    format(primary).or_else(|| {
        if std::ptr::eq(primary, english) {
            None
        } else {
            format(english)
        }
    })
}

fn load(locale: &str, source: &str) -> Bundle {
    let locale: LanguageIdentifier = locale.parse().expect("valid embedded locale");
    let mut bundle = Bundle::new_concurrent(vec![locale]);
    bundle.set_use_isolating(false);
    let resource =
        FluentResource::try_new(source.to_owned()).expect("valid embedded Fluent catalog");
    bundle.add_resource(resource).expect("unique Fluent keys");
    bundle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_by_value_or_attribute_and_keeps_authored_parameters() {
        let english =
            load("en", "only-en = English\ncontrol = Control\n    .hint = Open { $target }");
        let chinese = load("zh-Hans", "control = 控件");
        let mut args = FluentArgs::new();
        args.set("target", "镜头.amx");
        let translate = |key| format_with_fallback(&chinese, &english, key, Some(&args));
        assert_eq!(translate("control").as_deref(), Some("控件"));
        assert_eq!(translate("control.hint").as_deref(), Some("Open 镜头.amx"));
        assert_eq!(translate("only-en").as_deref(), Some("English"));
    }

    #[test]
    fn both_embedded_catalogs_are_valid_and_use_existing_language_keys() {
        let chosen = Language::from_key("zh-CN");
        assert_eq!(chosen.key(), "zh-CN");
        assert_eq!(chosen, Language::from_key("zh-Hans"));
        assert_eq!(chosen.format("gui-settings-title", None).as_deref(), Some("设置"));
        assert_eq!(
            Language::default().format("gui-settings-title", None).as_deref(),
            Some("Settings")
        );
    }
}
