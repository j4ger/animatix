use super::morph::{MorphOptions, MorphStrategy};
use super::plot::FuncBlendMode;
use crate::ast::{Expr, Modifier};
use crate::diagnostics::{Diagnostic, DiagnosticCode};
use crate::easing::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ParsedTimingModifiers {
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub easing: Easing,
    /// True when the author passed `ease:` explicitly. Preset entrances carry
    /// their own arrival curve and only fall back to it when this is false.
    pub ease_authored: bool,
    /// Length of the counter-move a motion verb should insert before its
    /// action (`[anticipate: 80ms]`); 0 = no anticipation.
    pub anticipate_ms: f64,
    pub morph_options: MorphOptions,
    pub func_blend_mode: FuncBlendMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModifierHost {
    Action,
    Assignment,
    Text,
    Typst,
    Code,
    ActorDeclaration,
}

/// Easing names have one source of truth: `animatix_syntax::easing`, which
/// the engine re-exports as `crate::easing`. Duplicating the table risks the
/// parser and the evaluator disagreeing on a name.
pub use crate::easing::parse_easing_name;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StaggerOrigin {
    Center,
    TopLeft,
    Point([f64; 2]),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StaggerMetric {
    #[default]
    Euclidean,
    Manhattan,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedStaggerModifiers {
    pub interval_ms: f64,
    pub from: Option<StaggerOrigin>,
    pub metric: StaggerMetric,
}

mod modifiers;
mod parsing;
mod stagger;
#[cfg(test)]
mod tests;

pub(crate) use modifiers::{
    config_string_value, has_non_default_morph_options, parse_duration_literal,
    push_conflicting_modifier_diagnostic, push_modifier_diagnostic,
    push_unknown_target_path_diagnostic, push_unsupported_stagger_statement_diagnostic,
    sequence_stmt_kind,
};
pub(crate) use parsing::parse_timing_modifiers;
pub(crate) use stagger::parse_stagger_modifiers;

pub(in crate::timeline::timing) use modifiers::{action_declares_modifier, ease_value_diagnostic};
