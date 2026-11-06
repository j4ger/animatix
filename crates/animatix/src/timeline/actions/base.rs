//! Common action execution context and target processing utilities.

use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::entity::EntityId;
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

/// Common timing parameters parsed from an action's modifiers.
#[derive(Clone, Copy, Debug)]
pub struct ActionTiming {
    /// Duration of the action in milliseconds.
    pub duration_ms: f64,
    /// Delay before the action starts in milliseconds.
    pub delay_ms: f64,
    /// Interpolation easing curve.
    pub easing: Easing,
    /// Absolute start timestamp in milliseconds.
    pub t_start_ms: u64,
    /// Absolute end timestamp in milliseconds.
    pub t_end_ms: u64,
}

impl ActionTiming {
    /// Parse timing modifiers for an action.
    pub fn parse(action: &Action, time_ms: f64, diagnostics: &mut Vec<Diagnostic>) -> Self {
        let parsed = parse_timing_modifiers(
            &action.modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;
        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        Self {
            duration_ms,
            delay_ms,
            easing,
            t_start_ms,
            t_end_ms,
        }
    }
}

/// Helper that parses timing, iterates over each target's [`EntityId`], and invokes `apply`.
pub fn for_each_action_target<F>(
    action: &Action,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    mut apply: F,
) where
    F: FnMut(&mut Timeline, &str, EntityId, &ActionTiming, &mut Vec<Diagnostic>),
{
    let timing = ActionTiming::parse(action, time_ms, diagnostics);
    for target in &action.targets {
        if let Some(id) = timeline.world.get_id(target) {
            apply(timeline, target, id, &timing, diagnostics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_timing_default() {
        let action = Action {
            verb: "fade-in".to_string(),
            targets: vec!["a".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
        };
        let mut diagnostics = Vec::new();
        let timing = ActionTiming::parse(&action, 1000.0, &mut diagnostics);
        assert_eq!(timing.duration_ms, 0.0);
        assert_eq!(timing.delay_ms, 0.0);
        assert_eq!(timing.t_start_ms, 1000);
        assert_eq!(timing.t_end_ms, 1000);
        assert!(diagnostics.is_empty());
    }
}
