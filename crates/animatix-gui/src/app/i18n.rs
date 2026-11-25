//! Interface copy resolved using the language of the current display.
use animatix_i18n::FluentArgs;
pub(crate) use animatix_i18n::Language;
use egui::{Context, Id, Ui};

fn language_id() -> Id {
    Id::unique("animatix.interface_language")
}

pub(crate) fn set_language(ctx: &Context, language: Language) {
    ctx.data_mut(|data| data.insert_temp(language_id(), language));
}

pub(crate) fn current_language(ctx: &Context) -> Language {
    ctx.data(|data| data.get_temp(language_id())).unwrap_or_default()
}

pub(crate) fn text(ui: &Ui, id: &str) -> String {
    text_for(current_language(ui.ctx()), id)
}

pub(crate) fn text_for(language: Language, id: &str) -> String {
    language.format(id, None).unwrap_or_else(|| id.to_owned())
}

pub(crate) fn format(language: Language, id: &str, args: FluentArgs<'_>) -> String {
    language.format(id, Some(&args)).unwrap_or_else(|| id.to_owned())
}

pub(crate) fn with_arg<'a>(
    language: Language,
    id: &str,
    name: &'a str,
    value: impl Into<animatix_i18n::FluentValue<'a>>,
) -> String {
    let mut args = FluentArgs::new();
    args.set(name, value);
    format(language, id, args)
}

pub(crate) fn template_title(
    language: Language,
    template: animatix_render::delivery::ProjectTemplate,
) -> String {
    use animatix_render::delivery::ProjectTemplate;
    let id = match template {
        ProjectTemplate::Narrated => "gui-template-narrated",
        ProjectTemplate::MathExplainer => "gui-template-math-explainer",
        ProjectTemplate::MetricBrief => "gui-template-metric-brief",
    };
    text_for(language, id)
}

pub(crate) fn empty_state(ui: &mut Ui, icon: &str, title: &str, subtitle: &str) {
    let title = text(ui, title);
    let subtitle = text(ui, subtitle);
    eparts::widget::layout::empty_state(ui, icon, &title, &subtitle);
}

/// A status can outlive the action that produced it. Retain the arguments so
/// the footer can render it in whichever language is currently displayed.
#[derive(Debug, Default, Clone)]
pub(crate) struct StatusText {
    canonical: String,
    base_len: Option<usize>,
    notes: Vec<(&'static str, StatusText)>,
    translation: Option<(&'static str, Vec<(&'static str, String)>)>,
}

impl StatusText {
    pub(crate) fn message(
        id: &'static str,
        args: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Self {
        let args: Vec<_> = args.into_iter().collect();
        let canonical =
            format_status(Language::English, id, &args).unwrap_or_else(|| id.to_owned());
        Self {
            canonical,
            base_len: None,
            notes: Vec::new(),
            translation: Some((id, args)),
        }
    }

    pub(crate) fn append(&mut self, separator: &'static str, note: Self) {
        self.base_len.get_or_insert(self.canonical.len());
        self.canonical.push_str(separator);
        self.canonical.push_str(&note.canonical);
        self.notes.push((separator, note));
    }

    pub(crate) fn display(&self, language: Language) -> std::borrow::Cow<'_, str> {
        use std::borrow::Cow;

        let base = &self.canonical[..self.base_len.unwrap_or(self.canonical.len())];
        let mut text = self
            .translation
            .as_ref()
            .and_then(|(id, args)| format_status(language, id, args))
            .map_or(Cow::Borrowed(base), Cow::Owned);
        for (separator, note) in &self.notes {
            text.to_mut().push_str(separator);
            text.to_mut().push_str(&note.display(language));
        }
        text
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn canonical(&self) -> &str {
        &self.canonical
    }
}

impl From<String> for StatusText {
    fn from(canonical: String) -> Self {
        Self {
            canonical,
            base_len: None,
            notes: Vec::new(),
            translation: None,
        }
    }
}

impl From<&str> for StatusText {
    fn from(canonical: &str) -> Self {
        Self::from(canonical.to_owned())
    }
}

pub(crate) fn render(ui: &Ui, id: &str, args: &[(&str, String)]) -> String {
    format_status(current_language(ui.ctx()), id, args).unwrap_or_else(|| id.to_owned())
}

fn format_status(language: Language, id: &str, args: &[(&str, String)]) -> Option<String> {
    let mut fluent = FluentArgs::new();
    for (name, value) in args {
        fluent.set(*name, value.as_str());
    }
    language.format(id, Some(&fluent))
}
