use crate::review::facts::ReviewFacts;
use crate::review::rules::{BLANK_INK_FRACTION, STATIC_BEAT_FRACTION, source_line};
use crate::review::types::{Persistence, RenderStatus, RuleIssue, Severity};
use std::collections::BTreeMap;

type OcclusionKey = (String, String, Option<String>);
type OcclusionWorst = (f64, f64, usize, usize);

pub(crate) fn rule_frame_blank(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    if facts.render.as_ref().is_none_or(|r| r.status != RenderStatus::Rendered) {
        return;
    }
    let Some(render) = &facts.render else {
        return;
    };
    let samples: Vec<_> = render.samples.iter().take(12).collect();
    if samples.is_empty() {
        return;
    }
    // A scene that paints anything at any sampled frame is not blank; a quiet
    // opening frame is a composition choice, not a defect. Only an entirely
    // ink-free pass over the sampled frames is reported.
    let max_ink = samples.iter().map(|sample| sample.ink_fraction).fold(0.0_f32, f32::max);
    if max_ink >= BLANK_INK_FRACTION {
        return;
    }
    let blankest = samples
        .iter()
        .min_by(|a, b| a.ink_fraction.total_cmp(&b.ink_fraction))
        .copied()
        .expect("samples is non-empty");
    out.push(RuleIssue {
        rule_id: "frame.blank",
        severity: Severity::Warning,
        persistence: Persistence::Held,
        fix_hint: None,
        waived_by: None,
        scene: None,
        time_s: Some(blankest.time_s),
        actor: None,
        source_line: None,
        message: format!(
            "Every sampled frame is blank (max {:.3}% ink across {} samples).",
            max_ink * 100.0,
            samples.len()
        ),
        evidence: serde_json::json!({
            "ink_fraction": blankest.ink_fraction,
            "max_ink_fraction": max_ink,
            "samples_checked": samples.len(),
            "threshold": BLANK_INK_FRACTION,
        }),
        suggestion: "Check that content is visible (opacity, off-frame placement, or an \
                     empty scene)."
            .to_string(),
        baseline_state: None,
    });
}

pub(crate) fn rule_beat_static(facts: &ReviewFacts, out: &mut Vec<RuleIssue>, skipped: &mut usize) {
    if facts.render.as_ref().is_none_or(|r| r.status != RenderStatus::Rendered) {
        return;
    }
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
            baseline_state: None,
        });
        reported += 1;
        if reported >= 12 {
            break;
        }
    }
}

pub(crate) fn rule_transition_endpoint(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    if facts.render.as_ref().is_none_or(|r| r.status != RenderStatus::Rendered) {
        return;
    }
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
            baseline_state: None,
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
            baseline_state: None,
        });
    }
    if rest.peak_ink_fraction > BLANK_INK_FRACTION
        && rest.held_ink_fraction < crate::review::rules::REST_FLOOR * rest.peak_ink_fraction
    {
        out.push(RuleIssue {
            rule_id: "rest.cast-erased",
            severity: Severity::Warning,
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
            baseline_state: None,
        });
    }
}

pub(crate) fn rule_text_occluded(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    if facts.render.as_ref().is_none_or(|r| r.status != RenderStatus::Rendered) {
        return;
    }
    let Some(render) = &facts.render else {
        return;
    };
    // One finding per (text, occluder): keep the worst sample rather than
    // repeating the same collision at every sampled frame.
    let mut worst: BTreeMap<OcclusionKey, OcclusionWorst> = BTreeMap::new();
    for sample in &render.samples {
        for occlusion in &sample.occlusions {
            let key =
                (occlusion.scene.clone(), occlusion.label.clone(), occlusion.occluder.clone());
            let entry = worst.entry(key).or_insert((
                0.0,
                sample.time_s,
                occlusion.probed_points,
                occlusion.covered_points,
            ));
            if occlusion.probe_coverage > entry.0 {
                *entry = (
                    occlusion.probe_coverage,
                    sample.time_s,
                    occlusion.probed_points,
                    occlusion.covered_points,
                );
            }
        }
    }
    for ((scene, label, occluder), (coverage, time_s, probed, covered)) in worst {
        // A single probe point touching another glyph is not "occluded"; the
        // grid is 27 points, so require at least a quarter of them.
        if coverage < 0.25 {
            continue;
        }
        let rule_id = if coverage >= 0.95 {
            "text.occluded-full"
        } else {
            "text.occluded"
        };
        out.push(RuleIssue {
            rule_id,
            severity: Severity::Warning,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: Some(scene.clone()),
            time_s: Some(time_s),
            actor: Some(label.clone()),
            source_line: source_line(facts, &scene, &label),
            message: format!(
                "`{label}` is occluded by {occluder:?} (coverage {:.1}%).",
                coverage * 100.0
            ),
            evidence: serde_json::json!({
                "occluder": occluder,
                "probe_coverage": coverage,
                "probed_points": probed,
                "covered_points": covered,
            }),
            suggestion: "Move the occluder, or move the text so it is fully visible.".to_string(),
            baseline_state: None,
        });
    }
}
