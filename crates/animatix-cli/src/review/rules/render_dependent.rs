use crate::review::facts::ReviewFacts;
use crate::review::rules::{BLANK_INK_FRACTION, STATIC_BEAT_FRACTION, source_line};
use crate::review::types::{Confidence, Persistence, RuleIssue, Severity};

pub(crate) fn rule_frame_blank(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let Some(render) = &facts.render else {
        return;
    };
    for sample in render.samples.iter().take(12) {
        if sample.ink_fraction >= BLANK_INK_FRACTION {
            continue;
        }
        out.push(RuleIssue {
            rule_id: "frame.blank",
            severity: Severity::Warning,
            confidence: Confidence::High,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
            time_s: Some(sample.time_s),
            actor: None,
            source_line: None,
            message: format!(
                "Rendered frame at {:.2}s is blank ({:.3}% ink).",
                sample.time_s,
                sample.ink_fraction * 100.0
            ),
            evidence: serde_json::json!({
                "ink_fraction": sample.ink_fraction,
                "threshold": BLANK_INK_FRACTION,
            }),
            suggestion: "Check that content is visible at this time (opacity, off-frame \
                         placement, or an empty scene)."
                .to_string(),
        });
    }
}

pub(crate) fn rule_beat_static(facts: &ReviewFacts, out: &mut Vec<RuleIssue>, skipped: &mut usize) {
    if facts.has_modifier_programs {
        *skipped += 1;
        return;
    }
    let Some(render) = &facts.render else {
        return;
    };
    let mut reported = 0usize;
    for pair in render.samples.windows(2) {
        let [before, after] = pair else { continue };
        let Some(changed) = after.changed_from_prev else {
            continue;
        };
        if changed >= STATIC_BEAT_FRACTION || after.time_s - before.time_s <= 0.05 {
            continue;
        }
        out.push(RuleIssue {
            rule_id: "beat.static",
            severity: Severity::Info,
            confidence: Confidence::Medium,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
            time_s: Some(after.time_s),
            actor: None,
            source_line: None,
            message: format!(
                "{:.2}s -> {:.2}s changes only {:.3}% of pixels; this beat is static.",
                before.time_s,
                after.time_s,
                changed * 100.0
            ),
            evidence: serde_json::json!({
                "changed_fraction": changed,
                "from_s": before.time_s,
                "to_s": after.time_s,
            }),
            suggestion: "Remove the dead beat or add motion/state change if the pause is \
                         unintentional."
                .to_string(),
        });
        reported += 1;
        if reported >= 12 {
            break;
        }
    }
}

pub(crate) fn rule_transition_endpoint(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let Some(render) = &facts.render else {
        return;
    };
    for sample in &render.samples {
        let Some(transition) = &sample.transition else {
            continue;
        };
        if sample.ink_fraction >= BLANK_INK_FRACTION {
            continue;
        }
        out.push(RuleIssue {
            rule_id: "transition.endpoint",
            severity: Severity::Warning,
            confidence: Confidence::Medium,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
            time_s: Some(sample.time_s),
            actor: None,
            source_line: None,
            message: format!(
                "Transition `{}` ({} -> {}) reaches a blank frame at {:.2}s.",
                transition.id, transition.from_scene, transition.to_scene, sample.time_s
            ),
            evidence: serde_json::json!({
                "transition_id": transition.id,
                "from_scene": transition.from_scene,
                "to_scene": transition.to_scene,
                "progress": transition.progress,
                "ink_fraction": sample.ink_fraction,
            }),
            suggestion: "Keep ink on screen through the transition or shorten it so neither \
                         endpoint is empty."
                .to_string(),
        });
    }
}

pub(crate) fn rule_site_rest(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let Some(rest) = &facts.site_rest else {
        return;
    };
    if rest.held_ink_fraction < BLANK_INK_FRACTION {
        out.push(RuleIssue {
            rule_id: "rest.ink-floor",
            severity: Severity::Warning,
            confidence: Confidence::High,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
            time_s: Some(rest.held_time_s),
            actor: None,
            source_line: None,
            message: format!(
                "Held frame at {:.2}s is essentially blank ({:.3}% ink).",
                rest.held_time_s,
                rest.held_ink_fraction * 100.0
            ),
            evidence: serde_json::json!({
                "held_time_s": rest.held_time_s,
                "held_ink_fraction": rest.held_ink_fraction,
                "ink_floor": BLANK_INK_FRACTION,
            }),
            suggestion: "Keep a resting composition on screen through the hold; do not fade the \
                         cast out before the loop rest."
                .to_string(),
        });
    }
    if rest.peak_ink_fraction > BLANK_INK_FRACTION
        && rest.held_ink_fraction < crate::review::rules::REST_FLOOR * rest.peak_ink_fraction
    {
        out.push(RuleIssue {
            rule_id: "rest.cast-erased",
            severity: Severity::Warning,
            confidence: Confidence::High,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
            time_s: Some(rest.held_time_s),
            actor: None,
            source_line: None,
            message: format!(
                "Held frame is {:.1}% ink against a {:.1}% plateau at {:.2}s — a closing beat \
                 erases the cast.",
                rest.held_ink_fraction * 100.0,
                rest.peak_ink_fraction * 100.0,
                rest.peak_time_s
            ),
            evidence: serde_json::json!({
                "held_time_s": rest.held_time_s,
                "held_ink_fraction": rest.held_ink_fraction,
                "peak_time_s": rest.peak_time_s,
                "peak_ink_fraction": rest.peak_ink_fraction,
                "rest_floor": crate::review::rules::REST_FLOOR,
            }),
            suggestion: "Stop the closing fade before the hold, or make the final beat animate \
                         parameters instead of erasing the cast."
                .to_string(),
        });
    }
}

pub(crate) fn rule_text_occluded(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let Some(render) = &facts.render else {
        return;
    };
    for sample in &render.samples {
        for occlusion in &sample.occlusions {
            let rule_id = if occlusion.probe_coverage >= 0.95 {
                "text.occluded-full"
            } else {
                "text.occluded"
            };
            out.push(RuleIssue {
                rule_id,
                severity: Severity::Warning,
                confidence: Confidence::High,
                persistence: Persistence::Held,
                fix_hint: None,
                waived_by: None,
                scene: Some(occlusion.scene.clone()),
                time_s: Some(sample.time_s),
                actor: Some(occlusion.label.clone()),
                source_line: source_line(facts, &occlusion.scene, &occlusion.label),
                message: format!(
                    "`{}` is occluded by {:?} (coverage {:.1}%).",
                    occlusion.label,
                    occlusion.occluder,
                    occlusion.probe_coverage * 100.0
                ),
                evidence: serde_json::json!({
                    "occluder": occlusion.occluder,
                    "probe_coverage": occlusion.probe_coverage,
                    "probed_points": occlusion.probed_points,
                    "covered_points": occlusion.covered_points,
                }),
                suggestion: "Move the occluder, or move the text so it is fully visible."
                    .to_string(),
            });
        }
    }
}
