use super::property_engine::parse_property_value;
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

mod declaration_impl;
mod text_impl;
