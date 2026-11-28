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

pub(crate) fn rule_text_size_at_display(
    facts: &ReviewFacts,
    min_height: Option<f64>,
    out: &mut Vec<RuleIssue>,
) {
    let canvas = animatix::timeline::SceneDimensions {
        width: facts.target.width,
        height: facts.target.height,
    };
    for actor in facts.actors.iter().filter(|actor| !actor.is_non_content) {
        let smallest = facts
            .frames
            .iter()
            .filter_map(|frame| {
                let sample = facts.actor_frame(frame, &actor.scene, &actor.label)?;
                let text = sample.text_at_display.as_ref()?;
                let default_height = if text.role.as_deref() == Some("note") {
                    6.0
                } else {
                    8.0
                };
                let threshold = min_height.unwrap_or(default_height);
                (text.bounds_height_px < threshold
                    && crate::review::facts::on_frame_fraction(text.text_bounds, canvas) > 0.0)
                    .then_some((text.bounds_height_px, text, threshold, frame.time_s))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0));
        let Some((height, text, threshold, time_s)) = smallest else {
            continue;
        };
        let bounds = text.text_bounds;
        out.push(RuleIssue {
            rule_id: "text.small-at-display",
            severity: Severity::Info,
            persistence: Persistence::Transient,
            scene: Some(actor.scene.clone()),
            time_s: Some(time_s),
            actor: Some(actor.label.clone()),
            source_line: actor.source_line,
            message: format!(
                "`{}` has {:.1}px-high text bounds at a {}px-wide display.",
                actor.label, height, text.width_px
            ),
            evidence: serde_json::json!({
                "display_width_px": text.width_px,
                "bounds_height_px": height,
                "min_text_height_px": threshold,
                "role": text.role,
                "bounds": [bounds.x0, bounds.y0, bounds.x1, bounds.y1],
            }),
            fix_hint: None,
            waived_by: None,
            suggestion: "Check the text at the intended display size. Enlarge important labels or give them more space.".to_string(),
            baseline_state: None,
        });
    }
}

pub(crate) fn rule_plot_empty_path(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    for actor in facts.actors.iter().filter(|actor| actor.is_plot && !actor.is_non_content) {
        let mut had_segments = false;
        let mut first_empty = None;
        for frame in facts.frames.iter().filter(|frame| frame.scene == actor.scene) {
            match facts
                .actor_frame(frame, &actor.scene, &actor.label)
                .filter(|sample| sample.opacity.is_some_and(|value| value > 0.01))
                .and_then(|sample| sample.path_segments)
            {
                None => {
                    had_segments = false;
                    first_empty = None;
                },
                Some(0) => {
                    if had_segments {
                        first_empty.get_or_insert(frame.time_s);
                    }
                },
                Some(_) => {
                    if let Some(time_s) = first_empty {
                        out.push(RuleIssue {
                            rule_id: "plot.empty-path",
                            severity: Severity::Info,
                            persistence: Persistence::Transient,
                            scene: Some(actor.scene.clone()),
                            time_s: Some(time_s),
                            actor: Some(actor.label.clone()),
                            source_line: actor.source_line,
                            message: format!(
                                "`{}` emitted a path with no segments between samples where it drew geometry.",
                                actor.label
                            ),
                            evidence: serde_json::json!({ "path_segments": 0 }),
                            fix_hint: None,
                            waived_by: None,
                            suggestion: "Check whether the plot intentionally leaves the viewport or has an empty domain at this time.".to_string(),
                            baseline_state: None,
                        });
                        break;
                    }
                    had_segments = true;
                },
            }
        }
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
        let Some(frame) = facts.all_frames().find(|frame| frame.time_s == sample.time_s) else {
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
    for missed in render.samples.iter().flat_map(|sample| &sample.missed_reveals) {
        let scene = &missed.scene;
        let label = &missed.label;
        let hidden_time = missed.hidden_s;
        let visible_time = missed.visible_s;
        out.push(RuleIssue {
            rule_id: "actor.reveal-missing",
            severity: Severity::Warning,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: Some(scene.to_string()),
            time_s: Some(visible_time),
            actor: Some(label.to_string()),
            source_line: source_line(facts, scene, label),
            message: format!(
                "`{label}` is hidden at {hidden_time:.2}s and visible at {visible_time:.2}s, \
                 but it contributes no detectable pixels to the composed frame."
            ),
            evidence: serde_json::json!({
                "hidden_time_s": hidden_time,
                "visible_time_s": visible_time,
                "same_time_paint_comparison": true,
            }),
            suggestion: "Check its paint, clipping and whether another layer covers it."
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
