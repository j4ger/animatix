use crate::review::facts::{FrameFact, ReviewFacts};
use crate::review::rules::source_line;
use crate::review::types::{Persistence, RenderStatus, RuleIssue, Severity};
use std::collections::BTreeMap;

pub(crate) fn rule_scene_duration(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    if !facts.actors.is_empty() {
        return;
    }
    out.push(RuleIssue {
        rule_id: "scene.duration",
        severity: Severity::Error,
        persistence: Persistence::Held,
        fix_hint: None,
        waived_by: None,
        scene: None,
        time_s: None,
        actor: None,
        source_line: None,
        message: "The file contains no visual actors.".to_string(),
        evidence: serde_json::json!({
            "duration_s": facts.target.duration_s,
            "actors": facts.actors.len(),
        }),
        suggestion: "Add visible content and a non-zero timeline length.".to_string(),
        baseline_state: None,
    });
}

pub(crate) fn rule_actor_draw_missing(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let mut reported: BTreeMap<(String, String), f64> = BTreeMap::new();
    for frame in facts.all_frames() {
        for actor_frame in &frame.actors {
            let Some(actor) = facts.actor(&actor_frame.scene, &actor_frame.label) else {
                continue;
            };
            if !actor.visual
                || actor.is_group_like
                || actor.category != animatix::timeline::ActorCategory::Shape
                || actor.is_effect_scope
                || actor_frame.opacity.unwrap_or(0.0) <= 0.01
                || actor_frame.padded_on_frame_fraction <= 0.0
                || actor_frame.draw_commands != Some(0)
            {
                continue;
            }
            reported
                .entry((actor_frame.scene.clone(), actor_frame.label.clone()))
                .or_insert(frame.time_s);
        }
    }
    for ((scene, label), time_s) in reported {
        out.push(RuleIssue {
            rule_id: "actor.draw-missing",
            severity: Severity::Error,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: Some(scene.clone()),
            time_s: Some(time_s),
            actor: Some(label.clone()),
            source_line: source_line(facts, &scene, &label),
            message: format!(
                "`{label}` is visible in the evaluated program but emitted no draw commands."
            ),
            evidence: serde_json::json!({"time_s": time_s, "scene": scene}),
            suggestion: "Check the primitive's data and draw path; an empty path or empty \
                         text is not visible."
                .to_string(),
            baseline_state: None,
        });
    }
}

pub(crate) fn rule_actor_ink_missing(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    if facts.render.as_ref().is_none_or(|r| r.status != RenderStatus::Rendered) {
        return;
    }
    let Some(render) = &facts.render else {
        return;
    };
    struct ActorInkMeasurement {
        max_ink: f32,
        first_time: f64,
        occurrences: usize,
        /// The engine emitted draw commands for this actor at some sample, so
        /// the pixel probe's inability to resolve thin strokes is not a defect.
        engine_drew: bool,
    }
    let mut measurements: BTreeMap<(String, String), ActorInkMeasurement> = BTreeMap::new();
    for sample in &render.samples {
        if sample.transition.is_some() {
            continue;
        }
        let Some(frame) =
            facts.all_frames().find(|frame| (frame.time_s - sample.time_s).abs() < 0.001)
        else {
            continue;
        };
        for actor_frame in &frame.actors {
            let Some(actor) = facts.actor(&actor_frame.scene, &actor_frame.label) else {
                continue;
            };
            if !actor.visual
                || actor.is_non_content
                || actor.is_background
                || actor.is_group_like
                || actor.is_effect_scope
                || actor_frame.label.contains("__")
            {
                continue;
            }
            // Only samples where the actor is expected to paint can prove
            // "never emits ink"; an invisible actor is judged by other rules.
            if actor_frame.opacity.unwrap_or(0.0) <= 0.01
                || actor_frame.padded_on_frame_fraction <= 0.0
            {
                continue;
            }
            let ink = sample
                .actor_ink
                .get(&(actor_frame.scene.clone(), actor_frame.label.clone()))
                .copied()
                .unwrap_or(0.0);
            let entry = measurements
                .entry((actor_frame.scene.clone(), actor_frame.label.clone()))
                .or_insert(ActorInkMeasurement {
                    max_ink: ink,
                    first_time: sample.time_s,
                    occurrences: 0,
                    engine_drew: false,
                });
            entry.max_ink = entry.max_ink.max(ink);
            entry.engine_drew |= actor_frame.draw_commands.unwrap_or(0) > 0;
            if ink <= 0.001 {
                entry.occurrences += 1;
            }
        }
    }
    for ((scene, label), measurement) in measurements {
        if measurement.engine_drew || measurement.max_ink > 0.001 {
            continue;
        }
        out.push(RuleIssue {
            rule_id: "actor.ink-missing",
            severity: Severity::Error,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: Some(scene.clone()),
            time_s: Some(measurement.first_time),
            actor: Some(label.clone()),
            source_line: source_line(facts, &scene, &label),
            message: format!("`{label}` has never emitted any ink across all sampled frames."),
            evidence: serde_json::json!({
                "time_s": measurement.first_time,
                "scene": scene,
                "occurrences": measurement.occurrences,
            }),
            suggestion: "Add an entrance or verify the actor's draw data; the actor is in the \
                         frame graph but paints nothing."
                .to_string(),
            baseline_state: None,
        });
    }
}

pub(crate) fn rule_actor_reveal_missing(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    if facts.render.as_ref().is_none_or(|r| r.status != RenderStatus::Rendered) {
        return;
    }
    let Some(render) = &facts.render else {
        return;
    };
    let mut reported: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new();
    for actor in &facts.actors {
        if !actor.visual
            || actor.is_group_like
            || actor.is_effect_scope
            || actor.is_non_content
            || actor.is_background
            || actor.label.contains("__")
        {
            continue;
        }
        let mut hidden: Option<(f64, f32)> = None;
        let mut visible: Option<(f64, f32)> = None;
        for frame in facts.all_frames() {
            let Some(actor_frame) = facts.actor_frame(frame, &actor.scene, &actor.label) else {
                continue;
            };
            if actor_frame.padded_on_frame_fraction <= 0.0 {
                continue;
            }
            let ink = render
                .samples
                .iter()
                .find(|sample| (sample.time_s - frame.time_s).abs() < 0.001)
                .and_then(|sample| {
                    sample.actor_ink.get(&(actor.scene.clone(), actor.label.clone())).copied()
                })
                .unwrap_or(0.0);
            if actor_frame.opacity.unwrap_or(0.0) <= 0.01 {
                hidden.get_or_insert((frame.time_s, ink));
            } else if actor_frame.opacity.unwrap_or(0.0) > 0.01 {
                visible.get_or_insert((frame.time_s, ink));
            }
        }
        let (Some((hidden_time, hidden_ink)), Some((visible_time, visible_ink))) =
            (hidden, visible)
        else {
            continue;
        };
        // Both ends unmeasured: the ink probe could not resolve this actor
        // (thin strokes, tiny glyphs), so this is not evidence of a defect.
        if hidden_ink.max(visible_ink) <= 0.01 {
            continue;
        }
        if visible_ink - hidden_ink > 0.01 {
            continue;
        }
        reported
            .entry((actor.scene.clone(), actor.label.clone()))
            .or_insert((visible_time, hidden_time));
    }
    for ((scene, label), (visible_time, hidden_time)) in reported {
        out.push(RuleIssue {
            rule_id: "actor.reveal-missing",
            severity: Severity::Warning,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: Some(scene.clone()),
            time_s: Some(visible_time),
            actor: Some(label.clone()),
            source_line: source_line(facts, &scene, &label),
            message: format!(
                "`{label}` is hidden at {hidden_time:.2}s and visible at {visible_time:.2}s, \
                 but its region barely changes."
            ),
            evidence: serde_json::json!({
                "hidden_time_s": hidden_time,
                "visible_time_s": visible_time,
            }),
            suggestion: "Check that the reveal action changes the actor's own pixels, not only \
                         its parent opacity."
                .to_string(),
            baseline_state: None,
        });
    }
}

pub(crate) fn rule_beat_static_structural(
    facts: &ReviewFacts,
    out: &mut Vec<RuleIssue>,
    skipped: &mut usize,
) {
    if facts.has_modifier_programs {
        *skipped += 1;
        return;
    }
    let frames: Vec<&FrameFact> = facts
        .frames
        .iter()
        .filter(|frame| frame.state_digest.is_some() && !frame.scene.contains(" -> "))
        .collect();
    let mut reported = 0usize;
    for (index, pair) in frames.windows(2).enumerate() {
        let [before, after] = pair else { continue };
        if after.time_s - before.time_s <= 0.05
            || before.cast == 0
            || after.cast == 0
            || before.state_digest != after.state_digest
        {
            continue;
        }
        if frames
            .get(index + 2)
            .is_some_and(|next| next.state_digest != after.state_digest)
        {
            continue;
        }
        out.push(RuleIssue {
            rule_id: "beat.static",
            severity: Severity::Warning,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: Some(after.scene.clone()),
            time_s: Some(after.time_s),
            actor: None,
            source_line: None,
            message: format!(
                "{:.2}s -> {:.2}s changes no evaluated actor state; this beat is static.",
                before.time_s, after.time_s
            ),
            evidence: serde_json::json!({
                "from_s": before.time_s,
                "to_s": after.time_s,
            }),
            suggestion: "Add a value change, motion, or content change at this beat.".to_string(),
            baseline_state: None,
        });
        reported += 1;
        if reported >= 8 {
            break;
        }
    }
}
