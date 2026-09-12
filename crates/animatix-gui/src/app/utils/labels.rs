//! Label generation utilities for actors and other named entities.

use std::collections::HashSet;

/// Generate a unique label for a new actor of the given type.
///
/// Scans the timeline track labels and returns the first unused
/// candidate of the form `{lowercase_type}{n}` (e.g. `rect1`, `text2`).
pub fn unique_label(timeline: Option<&animatix::timeline::Timeline>, ty: &str) -> String {
    let base = ty.to_lowercase();
    let existing: HashSet<String> =
        timeline.map(|t| t.tracks().keys().cloned().collect()).unwrap_or_default();
    for i in 1.. {
        let candidate = format!("{}{}", base, i);
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    format!("{}{}", base, existing.len() + 1)
}

/// Generate a unique effect-stage label within a compositing scope.
///
/// Effect stages are not timeline tracks, so [`unique_label`] cannot see them;
/// a stage label only has to be unique among the scope's own stages (it is
/// addressed as `scope.stage.param`).
pub fn unique_effect_label(
    timeline: Option<&animatix::timeline::Timeline>,
    scope: &str,
    ty: &str,
) -> String {
    let base = ty.to_lowercase();
    let existing: HashSet<&str> = timeline
        .and_then(|t| t.tracks().get(scope))
        .map(|track| track.effects.stages.iter().map(|stage| stage.label.as_str()).collect())
        .unwrap_or_default();
    for i in 1.. {
        let candidate = format!("{}{}", base, i);
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
    }
    format!("{}{}", base, existing.len() + 1)
}

#[cfg(test)]
mod tests {
    use animatix::timeline::{AnimationTrack, Timeline};

    use super::*;

    #[test]
    fn unique_label_finds_first_available() {
        let mut timeline = Timeline::new();
        timeline
            .tracks_mut()
            .insert("rect1".into(), AnimationTrack::new("rect1".into(), "Rect"));

        assert_eq!(unique_label(Some(&timeline), "Rect"), "rect2");
    }

    #[test]
    fn unique_label_starts_at_one() {
        let timeline = Timeline::new();
        assert_eq!(unique_label(Some(&timeline), "Text"), "text1");
    }

    #[test]
    fn unique_label_skips_occupied() {
        let mut timeline = Timeline::new();
        timeline
            .tracks_mut()
            .insert("rect1".into(), AnimationTrack::new("rect1".into(), "Rect"));
        timeline
            .tracks_mut()
            .insert("rect2".into(), AnimationTrack::new("rect2".into(), "Rect"));

        assert_eq!(unique_label(Some(&timeline), "Rect"), "rect3");
    }
}
