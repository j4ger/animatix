use super::property_engine::{parse_property_value, write_property_field};
use super::property_registry::lookup_property;
use super::{
    DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, Diagnostic, DiagnosticCode, DiagnosticPhase, Easing,
    Expr, Modifier, ModifierHost, MorphOptions, ParsedTimingModifiers, Timeline, TrackAccessor,
    Value, apply_explicit_position_binding, evaluate_expr_with_lookup_diagnostic,
    has_non_default_morph_options, parse_color_in_env_with_lookup_diagnostic,
    parse_timing_modifiers, preserve_discrete_position_state_before,
    preserve_instant_delayed_value, push_modifier_diagnostic,
    resolve_position_binding_with_lookup_diagnostic,
};
use crate::ast::Property;
use crate::renderer::error::RenderError;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextDeclarationKind {
    Text,
    Code,
    Typst,
    Math,
    Counter,
}

impl TextDeclarationKind {
    fn unnamed_label(self) -> &'static str {
        match self {
            Self::Text => "unnamed_text",
            Self::Code => "unnamed_code",
            Self::Typst => "unnamed_typst",
            Self::Math => "unnamed_math",
            Self::Counter => "unnamed_counter",
        }
    }

    /// Registry type name for this text-like actor.
    fn type_name(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Code => "Code",
            Self::Typst => "Typst",
            Self::Math => "Math",
            Self::Counter => "Counter",
        }
    }

    fn modifier_host(self) -> ModifierHost {
        match self {
            Self::Text | Self::Counter => ModifierHost::Text,
            Self::Code => ModifierHost::Code,
            Self::Typst | Self::Math => ModifierHost::Typst,
        }
    }

    fn default_font_size(self) -> f32 {
        match self {
            Self::Code => 24.0,
            Self::Text | Self::Typst | Self::Math | Self::Counter => 48.0,
        }
    }

    fn content_matches(self, property_name: &str) -> bool {
        // Uniform content property across text-like kinds: `text`, which the
        // schema now lists for every text-like actor. `Code` also accepts its
        // own spelling. The `content`/`latex`/`math` aliases were removed from
        // the property table on 2026-09-14 — each was a second name for the
        // same storage field, so the analyzer warned about the uniform spelling
        // while the runtime honoured it.
        match self {
            Self::Text | Self::Typst | Self::Math => property_name == "text",
            Self::Code => matches!(property_name, "text" | "code"),
            Self::Counter => false,
        }
    }

    fn morph_warning(self) -> &'static str {
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

mod text_impl;
