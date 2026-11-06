//! Kind-specific text declaration behavior and the public declaration entry point.

use super::*;

impl TextDeclarationKind {
    pub(super) fn unnamed_label(self) -> &'static str {
        match self {
            Self::Text => "unnamed_text",
            Self::Code => "unnamed_code",
            Self::Typst => "unnamed_typst",
            Self::Math => "unnamed_math",
            Self::Counter => "unnamed_counter",
        }
    }

    /// Registry type name for this text-like actor.
    pub(super) fn type_name(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Code => "Code",
            Self::Typst => "Typst",
            Self::Math => "Math",
            Self::Counter => "Counter",
        }
    }

    pub(super) fn modifier_host(self) -> ModifierHost {
        match self {
            Self::Text | Self::Counter => ModifierHost::Text,
            Self::Code => ModifierHost::Code,
            Self::Typst | Self::Math => ModifierHost::Typst,
        }
    }

    pub(super) fn default_font_size(self) -> f32 {
        match self {
            Self::Code => 24.0,
            Self::Text | Self::Typst | Self::Math | Self::Counter => 48.0,
        }
    }

    pub(super) fn content_matches(self, property_name: &str) -> bool {
        // Uniform content property across text-like kinds: `text`, which the
        // schema lists for every text-like actor. `Code` also accepts its own
        // spelling. The `content`/`latex`/`math` aliases are absent from the
        // property table — each was a second name for the same storage field,
        // so the analyzer warned about the uniform spelling while the runtime
        // honoured it.
        match self {
            Self::Text | Self::Typst | Self::Math => property_name == "text",
            Self::Code => matches!(property_name, "text" | "code"),
            Self::Counter => false,
        }
    }

    pub(super) fn morph_warning(self) -> &'static str {
        match self {
            Self::Text => {
                "Morph-specific modifiers on text declaration require a re-declaration with non-zero duration; ignoring them for now."
            },
            Self::Code => {
                "Morph-specific modifiers on code declaration require a re-declaration with non-zero duration; ignoring them for now."
            },
            Self::Typst => {
                "Morph-specific modifiers on typst declaration require a re-declaration with non-zero duration; ignoring them for now."
            },
            Self::Math => {
                "Morph-specific modifiers on math declaration require a re-declaration with non-zero duration; ignoring them for now."
            },
            Self::Counter => {
                "Morph-specific modifiers on counter declaration require a re-declaration with non-zero duration; ignoring them for now."
            },
        }
    }
}

impl Timeline {
    /// Process a text actor declaration (Text, Math, Code, Typst) and add it to the timeline.
    pub fn process_text_actor_decl(
        &mut self,
        actor_type: &str,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<(), RenderError> {
        let kind = match animatix_std::caps_for_type(actor_type).and_then(|c| c.text) {
            Some(animatix_core::caps::TextKind::Counter) => TextDeclarationKind::Counter,
            Some(animatix_core::caps::TextKind::Text) => TextDeclarationKind::Text,
            Some(animatix_core::caps::TextKind::Math) => TextDeclarationKind::Math,
            Some(animatix_core::caps::TextKind::Code) => TextDeclarationKind::Code,
            Some(animatix_core::caps::TextKind::Typst) => TextDeclarationKind::Typst,
            None => return Ok(()),
        };
        self.process_text_declaration(
            kind,
            Some(label),
            props,
            modifiers,
            time_ms,
            parent_label,
            diagnostics,
        )
    }
}
