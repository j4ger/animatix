//! Strict verify policy built on the shared review facts.
//!
//! These rules are intentionally kept separate from the heuristic review
//! policy: review tolerates more authoring styles, while verify is the gate an
//! LLM-generated scene must satisfy.

use std::collections::BTreeMap;

use crate::review::facts::{FrameFact, ReviewFacts};
use crate::review::rules::{
    RuleRun, rule_beat_static, rule_build_never_revealed, rule_clipped, rule_frame_blank,
    rule_never_on_frame, rule_opacity_never_positive, rule_site_rest, rule_text_overflow,
    rule_transition_endpoint, source_line, time_key,
};
use crate::review::types::{Confidence, RenderStatus, RuleIssue, Severity};

/// Run the strict verify policy. This reuses the review fact set and the
/// existing rule functions, then adds the checks a generated scene needs:
/// draw presence, per-actor ink, reveal coverage, dead beats, and scene
/// duration. `review` keeps its own tolerant policy.
pub(crate) fn evaluate_verify(facts: &ReviewFacts) -> RuleRun {
    let mut issues = Vec::new();
    let mut skipped = 0usize;
    rule_build_never_revealed(facts, &mut issues);
    rule_opacity_never_positive(facts, &mut issues);
    rule_never_on_frame(facts, &mut issues);
    rule_clipped(facts, &mut issues);
    rule_text_overflow(facts, &mut issues);
    rule_scene_duration(facts, &mut issues);
    rule_actor_draw_missing(facts, &mut issues);

    let render_ready = facts
        .render
        .as_ref()
        .is_some_and(|render| render.status == RenderStatus::Rendered);
    if render_ready {
        rule_actor_ink_missing(facts, &mut issues);
        rule_actor_reveal_missing(facts, &mut issues);
        rule_frame_blank(facts, &mut issues);
        rule_beat_static(facts, &mut issues, &mut skipped);
        rule_transition_endpoint(facts, &mut issues);
        if facts.site_rest.is_some() {
            rule_site_rest(facts, &mut issues);
        } else {
            skipped += 2;
        }
    } else {
        // actor.ink-missing, actor.reveal-missing, frame.blank,
        // beat.static(render), transition.endpoint, rest.ink-floor,
        // rest.cast-erased
        skipped += 6;
        rule_beat_static_structural(facts, &mut issues, &mut skipped);
    }

    // `build.never-revealed` already explains the same actor; do not report a
    // second opacity/never-on-frame issue for it.
    let never_revealed: std::collections::BTreeSet<Option<String>> = issues
        .iter()
        .filter(|issue| issue.rule_id == "build.never-revealed")
        .map(|issue| issue.actor.clone())
        .collect();
    issues.retain(|issue| {
        !matches!(issue.rule_id, "actor.opacity-never-positive" | "actor.never-on-frame")
            || !never_revealed.contains(&issue.actor)
    });

    for issue in &mut issues {
        if matches!(
            issue.rule_id,
            "build.never-revealed" | "actor.opacity-never-positive" | "rest.ink-floor"
        ) {
            issue.severity = Severity::Error;
        }
    }

    issues.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| time_key(a).total_cmp(&time_key(b)))
            .then_with(|| a.rule_id.cmp(b.rule_id))
            .then_with(|| a.actor.cmp(&b.actor))
    });
    RuleRun {
        issues,
        skipped,
        checked: 7 + if render_ready { 6 } else { 0 },
    }
}

fn rule_scene_duration(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    // A static poster with actors and no keyframes is valid; only an empty
    // file is a hard error here.
    if !facts.actors.is_empty() {
        return;
    }
    out.push(RuleIssue {
        rule_id: "scene.duration",
        severity: Severity::Error,
        confidence: Confidence::High,
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
    });
}

fn rule_actor_draw_missing(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
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
            confidence: Confidence::High,
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
        });
    }
}

fn rule_actor_ink_missing(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let Some(render) = &facts.render else {
        return;
    };
    let mut reported: BTreeMap<(String, String), (f64, usize)> = BTreeMap::new();
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
                || actor.is_group_like
                || actor.is_effect_scope
                || actor_frame.opacity.unwrap_or(0.0) <= 0.01
                || actor_frame.padded_on_frame_fraction <= 0.0
                || actor_frame.draw_commands == Some(0)
            {
                continue;
            }
            let ink = sample
                .actor_ink
                .get(&(actor_frame.scene.clone(), actor_frame.label.clone()))
                .copied()
                .unwrap_or(0.0);
            if ink > 0.001 {
                continue;
            }
            let entry = reported
                .entry((actor_frame.scene.clone(), actor_frame.label.clone()))
                .or_insert((sample.time_s, 0));
            entry.1 += 1;
        }
    }
    for ((scene, label), (time_s, occurrences)) in reported {
        out.push(RuleIssue {
            rule_id: "actor.ink-missing",
            severity: Severity::Error,
            confidence: Confidence::Medium,
            scene: Some(scene.clone()),
            time_s: Some(time_s),
            actor: Some(label.clone()),
            source_line: source_line(facts, &scene, &label),
            message: format!(
                "`{label}` has positive opacity and on-frame bounds but no ink at {time_s:.2}s."
            ),
            evidence: serde_json::json!({
                "time_s": time_s,
                "scene": scene,
                "occurrences": occurrences,
            }),
            suggestion: "Add an entrance or verify the actor's draw data; the actor is in the \
                         frame graph but paints nothing."
                .to_string(),
        });
    }
}

fn rule_actor_reveal_missing(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let Some(render) = &facts.render else {
        return;
    };
    let mut reported: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new();
    for actor in &facts.actors {
        if !actor.visual || actor.is_group_like || actor.is_effect_scope {
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
            confidence: Confidence::Medium,
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
        });
    }
}

fn rule_beat_static_structural(facts: &ReviewFacts, out: &mut Vec<RuleIssue>, skipped: &mut usize) {
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
        // A flat interval immediately followed by a change is a setup/hold
        // before the next beat, not a dead beat.
        if frames
            .get(index + 2)
            .is_some_and(|next| next.state_digest != after.state_digest)
        {
            continue;
        }
        out.push(RuleIssue {
            rule_id: "beat.static",
            severity: Severity::Warning,
            confidence: Confidence::Medium,
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
        });
        reported += 1;
        if reported >= 8 {
            break;
        }
    }
}
