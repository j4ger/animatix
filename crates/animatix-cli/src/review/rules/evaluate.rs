use crate::review::facts::ReviewFacts;
use crate::review::markers::{MarkerSet, waiver_for};
use crate::review::rules::{basic::*, render_dependent::*, structural::*, time_key};
use crate::review::types::{Persistence, Policy, RenderStatus, RuleIssue, Severity};
use std::collections::BTreeMap;

pub(crate) struct RuleRun {
    pub issues: Vec<RuleIssue>,
    pub skipped: usize,
    pub checked: usize,
    pub skipped_rules: Vec<String>,
}

pub(crate) fn evaluate(facts: &ReviewFacts, markers: &MarkerSet, policy: Policy) -> RuleRun {
    let mut issues = Vec::new();
    let mut skipped = 0usize;
    let mut skipped_rules = Vec::new();
    rule_build_never_revealed(facts, &mut issues);
    rule_opacity_never_positive(facts, &mut issues);
    rule_never_on_frame(facts, &mut issues);
    rule_clipped(facts, &mut issues);
    rule_overlap_candidates(facts, &mut issues);
    rule_text_overflow(facts, &mut issues);
    rule_entrance_not_visible(facts, &mut issues);
    rule_keyframe_redundant(facts, &mut issues);
    rule_scene_duration(facts, &mut issues);
    rule_actor_draw_missing(facts, &mut issues);

    for invalid in &markers.invalid {
        issues.push(RuleIssue {
            rule_id: "layout.marker-invalid",
            severity: Severity::Warning,
            persistence: Persistence::Held,
            scene: None,
            time_s: None,
            actor: None,
            source_line: Some(invalid.line),
            message: invalid.message.clone(),
            evidence: serde_json::json!({ "line": invalid.line }),
            fix_hint: None,
            waived_by: None,
            suggestion: "Fix or remove the marker; see llms.txt for the syntax.".to_string(),
        });
    }

    let visual_count = facts.actors.iter().filter(|a| a.visual).count();
    let waived_labels: std::collections::BTreeSet<&str> = markers
        .waivers
        .iter()
        .filter(|w| !w.label.is_empty())
        .map(|w| w.label.as_str())
        .collect();
    if visual_count > 0 && waived_labels.len() * 2 > visual_count {
        issues.push(RuleIssue {
            rule_id: "layout.marker-abuse",
            severity: Severity::Warning,
            persistence: Persistence::Held,
            scene: None,
            time_s: None,
            actor: None,
            source_line: None,
            message: format!(
                "{} of {} visual actors carry layout waivers; blanket opt-outs hide real defects.",
                waived_labels.len(),
                visual_count
            ),
            evidence: serde_json::json!({
                "waived_actors": waived_labels.len(),
                "visual_actors": visual_count,
            }),
            fix_hint: None,
            waived_by: None,
            suggestion:
                "Scope each waiver to the smallest wrapper that owns the intentional layout."
                    .to_string(),
        });
    }

    let render_ready = facts
        .render
        .as_ref()
        .is_some_and(|render| render.status == RenderStatus::Rendered);
    if render_ready {
        rule_frame_blank(facts, &mut issues);
        rule_beat_static(facts, &mut issues, &mut skipped);
        rule_transition_endpoint(facts, &mut issues);
        rule_text_occluded(facts, &mut issues);
        rule_actor_ink_missing(facts, &mut issues);
        rule_actor_reveal_missing(facts, &mut issues);
        rule_beat_static_structural(facts, &mut issues, &mut skipped);
    } else {
        skipped += 6;
        skipped_rules.extend([
            "actor.ink-missing".to_string(),
            "actor.reveal-missing".to_string(),
            "frame.blank".to_string(),
            "beat.static".to_string(),
            "transition.endpoint".to_string(),
            "text.occluded".to_string(),
        ]);
    }
    if policy == Policy::Site {
        if render_ready && facts.site_rest.is_some() {
            rule_site_rest(facts, &mut issues);
        } else {
            skipped += 2;
            skipped_rules.extend(["rest.ink-floor".to_string(), "rest.cast-erased".to_string()]);
        }
    }

    let mut history: BTreeMap<(String, Option<String>, Option<String>), Vec<f64>> = BTreeMap::new();
    for issue in &issues {
        if let Some(time_s) = issue.time_s {
            history
                .entry((issue.rule_id.to_string(), issue.actor.clone(), issue.scene.clone()))
                .or_default()
                .push(time_s);
        }
    }

    let mut final_issues = Vec::new();
    for mut issue in issues {
        let waiver = if let Some(actor_str) = &issue.actor {
            if actor_str.contains(", ") {
                actor_str.split(", ").find_map(|single_actor| {
                    let ancestors = issue
                        .scene
                        .as_ref()
                        .and_then(|s| facts.actor(s, single_actor))
                        .map(|a| a.ancestors.clone())
                        .unwrap_or_default();
                    waiver_for(markers, single_actor, &ancestors, issue.rule_id)
                })
            } else {
                let ancestors = issue
                    .scene
                    .as_ref()
                    .and_then(|s| facts.actor(s, actor_str))
                    .map(|a| a.ancestors.clone())
                    .unwrap_or_default();
                waiver_for(markers, actor_str, &ancestors, issue.rule_id)
            }
        } else {
            waiver_for(markers, "", &[], issue.rule_id)
        };

        if let Some(w) = waiver {
            issue.waived_by = Some(w.raw.clone());
            issue.severity = Severity::Info;
        }

        if let Some(times) =
            history.get(&(issue.rule_id.to_string(), issue.actor.clone(), issue.scene.clone()))
        {
            // Rule functions aggregate across sampled frames, so a single
            // issue usually represents a persistent condition record. Never
            // downgrade severity here: doing so turned nearly every finding
            // into Info and made the gate a no-op.
            issue.persistence = if times.len() >= 2 {
                Persistence::Held
            } else {
                Persistence::Transient
            };
        }

        final_issues.push(issue);
    }

    final_issues.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| time_key(a).total_cmp(&time_key(b)))
            .then_with(|| a.rule_id.cmp(b.rule_id))
            .then_with(|| a.actor.cmp(&b.actor))
    });
    RuleRun {
        issues: final_issues,
        skipped,
        checked: 15 + if render_ready { 7 } else { 0 },
        skipped_rules,
    }
}
