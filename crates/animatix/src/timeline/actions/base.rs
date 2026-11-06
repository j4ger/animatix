//! Common action execution context and target processing utilities.

use crate::ast::{Action, Modifier};
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
    /// Whether the author spelled out an `ease`/`easing` modifier (presets use
    /// their own arrival curve when the author did not).
    pub ease_authored: bool,
    /// Anticipation lead-in in milliseconds for motion actions.
    pub anticipate_ms: f64,
    /// Absolute start timestamp in milliseconds.
    pub t_start_ms: u64,
    /// Absolute end timestamp in milliseconds.
    pub t_end_ms: u64,
}

impl ActionTiming {
    /// Parse timing modifiers for an action.
    pub fn parse(action: &Action, time_ms: f64, diagnostics: &mut Vec<Diagnostic>) -> Self {
        Self::from_parsed(
            &parse_timing_modifiers(
                &action.modifiers,
                ModifierHost::Action,
                Some(&action.verb),
                diagnostics,
            ),
            time_ms,
        )
    }

    /// Parse timing modifiers for an action while excluding action-specific
    /// modifier keys (for example `to`, `by`, `along`) from the timing
    /// vocabulary, so a parameter passed on the same action is not misread as a
    /// timing modifier.
    ///
    /// This is the single implementation of the hand-rolled
    /// "filter the modifiers, then parse" blocks action files used to repeat.
    pub fn parse_excluding(
        action: &Action,
        excluded_keys: &[&str],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self {
        if excluded_keys.is_empty() {
            return Self::parse(action, time_ms, diagnostics);
        }
        let modifiers: Vec<Modifier> = action
            .modifiers
            .iter()
            .filter(|modifier| {
                modifier.name.as_deref().is_none_or(|name| !excluded_keys.contains(&name))
            })
            .cloned()
            .collect();
        Self::from_parsed(
            &parse_timing_modifiers(
                &modifiers,
                ModifierHost::Action,
                Some(&action.verb),
                diagnostics,
            ),
            time_ms,
        )
    }

    fn from_parsed(parsed: &crate::timeline::timing::ParsedTimingModifiers, time_ms: f64) -> Self {
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        Self {
            duration_ms,
            delay_ms,
            easing: parsed.easing,
            ease_authored: parsed.ease_authored,
            anticipate_ms: parsed.anticipate_ms,
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
