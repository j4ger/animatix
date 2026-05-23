//!
//! Top-level parsers for the Animatix DSL.
//!
//! These parsers handle constructs that appear at the top level of a `.amx` file:
//! scene declarations, keyframes, config blocks, and `play` transitions.
//! The actual combinator construction lives in [`super::parser()`] due to
//! shared dependencies. Utility functions extracted here are called from
//! within the main parser function.

use crate::ast::*;

/// Convert play statement modifiers into a `Transition` descriptor.
///
/// Modifiers format: `[fade, 300ms]` or `[wipe-left, 200ms]`.
/// The first bare identifier (not a time) is the transition type.
/// The first time literal is the duration.
pub(crate) fn parse_transition_from_modifiers(
    modifiers: &[Modifier],
) -> Option<crate::ast::Transition> {
    let mut transition_id: Option<String> = None;
    let mut duration_ms: u64 = 0;

    for m in modifiers {
        match (&m.name, &m.value) {
            (None, Expr::Ident(name)) if transition_id.is_none() => {
                if crate::transition_registry::find(name).is_some() {
                    transition_id = Some(name.clone());
                }
            }
            (None, Expr::Ident(name)) if name.ends_with("ms") => {
                if let Ok(ms) = name.trim_end_matches("ms").parse::<u64>() {
                    if duration_ms == 0 {
                        duration_ms = ms;
                    }
                }
            }
            (None, Expr::Ident(name))
                if name.ends_with('s') && !name.starts_with(|c: char| c.is_alphabetic()) =>
            {
                if let Ok(s) = name.trim_end_matches('s').parse::<f64>() {
                    if duration_ms == 0 {
                        duration_ms = (s * 1000.0) as u64;
                    }
                }
            }
            _ => {}
        }
    }

    transition_id.map(|id| crate::ast::Transition {
        id,
        duration_ms,
        easing: crate::easing::Easing::Linear,
    })
}