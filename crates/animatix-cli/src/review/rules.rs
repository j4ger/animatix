//! Deterministic rule engine for `animatix review`.
//!
//! Rules only turn measured [`ReviewFacts`] into
//! issues. They never touch the renderer, so every rule is unit-testable
//! against synthetic facts and the same rule set runs with or without a GPU.

use std::collections::BTreeMap;

use crate::frame_source::Box2;

use super::facts::{ActorFact, ReviewFacts};
use super::types::{Confidence, Policy, RenderStatus, RuleIssue, Severity};

/// Below this whole-frame ink fraction a rendered frame is treated as blank
/// (0.1% — well below any deliberate content and above antialiasing noise).
const BLANK_INK_FRACTION: f32 = 0.001;
/// Below this changed-pixel fraction two consecutive samples are one beat.
const STATIC_BEAT_FRACTION: f32 = 0.0005;
/// Site policy: the held frame must keep at least this share of the plateau.
const REST_FLOOR: f32 = 0.35;
/// Overlap shorter than this much of the smaller actor is ignored.
const OVERLAP_RATIO_FLOOR: f64 = 0.05;

/// Result of one rule pass.
pub(crate) struct RuleRun {
    /// All findings, sorted for stable output.
    pub issues: Vec<RuleIssue>,
    /// Number of rule checks that could not run (e.g. render tier skipped).
    pub skipped: usize,
}

/// Run every rule enabled by `policy`.
pub(crate) fn evaluate(facts: &ReviewFacts, policy: Policy) -> RuleRun {
    let mut issues = Vec::new();
    let mut skipped = 0usize;
    rule_build_never_revealed(facts, &mut issues);
    rule_opacity_never_positive(facts, &mut issues);
    rule_never_on_frame(facts, &mut issues);
    rule_clipped(facts, &mut issues);
    rule_overlap_candidates(facts, &mut issues);
    rule_text_overflow(facts, &mut issues);
    rule_entrance_not_visible(facts, &mut issues);
    rule_keyframe_redundant(facts, &mut issues);

    let render_ready = facts
        .render
        .as_ref()
        .is_some_and(|render| render.status == RenderStatus::Rendered);
    if render_ready {
        rule_frame_blank(facts, &mut issues);
        rule_beat_static(facts, &mut issues, &mut skipped);
        rule_transition_endpoint(facts, &mut issues);
    } else {
        // frame.blank, beat.static, transition.endpoint
        skipped += 3;
    }
    if policy == Policy::Site {
        if render_ready && facts.site_rest.is_some() {
            rule_site_rest(facts, &mut issues);
        } else {
            // rest.ink-floor, rest.cast-erased
            skipped += 2;
        }
    }

    issues.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| time_key(a).total_cmp(&time_key(b)))
            .then_with(|| a.rule_id.cmp(b.rule_id))
            .then_with(|| a.actor.cmp(&b.actor))
    });
    RuleRun { issues, skipped }
}

fn time_key(issue: &RuleIssue) -> f64 {
    issue.time_s.unwrap_or(-1.0)
}

fn source_line(facts: &ReviewFacts, scene: &str, label: &str) -> Option<usize> {
    facts.actor(scene, label).and_then(|actor| actor.source_line)
}

fn rule_build_never_revealed(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    for diagnostic in &facts.build_diagnostics {
        if diagnostic.code != "never-revealed" {
            continue;
        }
        let line = diagnostic.source_line.or_else(|| {
            diagnostic
                .actor
                .as_deref()
                .and_then(|label| facts.actors.iter().find(|actor| actor.label == label))
                .and_then(|actor| actor.source_line)
        });
        out.push(RuleIssue {
            rule_id: "build.never-revealed",
            severity: diagnostic.severity,
            confidence: Confidence::High,
            time_s: None,
            actor: diagnostic.actor.clone(),
            source_line: line,
            message: diagnostic.message.clone(),
            evidence: serde_json::json!({
                "diagnostic_code": diagnostic.code,
                "diagnostic_severity": diagnostic.severity.as_str(),
            }),
            suggestion: "Add an entrance action (fade-in, wipe-in, draw-in, reveal-in) or an \
                         explicit opacity assignment so the actor is revealed."
                .to_string(),
        });
    }
}

fn rule_opacity_never_positive(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let mut maxima: BTreeMap<(String, String), f32> = facts
        .actors
        .iter()
        .filter(|actor| actor.visual)
        .map(|actor| ((actor.scene.clone(), actor.label.clone()), actor.max_effective_opacity))
        .collect();
    for frame in facts.all_frames() {
        for actor_frame in &frame.actors {
            let visual = facts
                .actor(&actor_frame.scene, &actor_frame.label)
                .is_some_and(|actor| actor.visual);
            if !visual {
                continue;
            }
            let entry = maxima
                .entry((actor_frame.scene.clone(), actor_frame.label.clone()))
                .or_insert(0.0);
            *entry = entry.max(actor_frame.opacity.unwrap_or(0.0));
        }
    }
    for ((scene, label), max_opacity) in maxima {
        if max_opacity > 0.001 {
            continue;
        }
        let line = source_line(facts, &scene, &label);
        out.push(RuleIssue {
            rule_id: "actor.opacity-never-positive",
            severity: Severity::Warning,
            confidence: Confidence::High,
            time_s: None,
            actor: Some(label.clone()),
            source_line: line,
            message: format!(
                "`{label}` never reaches positive rendered opacity across the sampled frames."
            ),
            evidence: serde_json::json!({
                "max_effective_opacity": max_opacity,
                "scene": scene,
            }),
            suggestion: "Check the actor's opacity and its ancestors' opacity; an actor seeded \
                         hidden needs an entrance or an explicit opacity."
                .to_string(),
        });
    }
}

#[derive(Default)]
struct NeverOnFrameMeasurement {
    max_opacity: f32,
    first_bounds: Option<Box2>,
    first_time: Option<f64>,
}

fn rule_never_on_frame(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let mut measurements: BTreeMap<(String, String), NeverOnFrameMeasurement> = BTreeMap::new();
    for frame in facts.all_frames() {
        for actor_frame in &frame.actors {
            let visual = facts
                .actor(&actor_frame.scene, &actor_frame.label)
                .is_some_and(|actor| actor.visual);
            if !visual {
                continue;
            }
            let entry = measurements
                .entry((actor_frame.scene.clone(), actor_frame.label.clone()))
                .or_default();
            entry.max_opacity = entry.max_opacity.max(actor_frame.opacity.unwrap_or(0.0));
            if actor_frame.bounds.is_some() && entry.first_bounds.is_none() {
                entry.first_bounds = actor_frame.bounds;
                entry.first_time = Some(frame.time_s);
            }
        }
    }
    for ((scene, label), measurement) in measurements {
        let Some(bounds) = measurement.first_bounds else {
            continue;
        };
        if measurement.max_opacity <= 0.01 {
            continue;
        }
        let first_time = measurement.first_time;
        let ever_on_frame = facts.all_frames().any(|frame| {
            facts.actor_frame(frame, &scene, &label).is_some_and(|actor_frame| {
                actor_frame.opacity.unwrap_or(0.0) > 0.01 && actor_frame.on_frame_fraction > 0.0
            })
        });
        if ever_on_frame {
            continue;
        }
        let line = source_line(facts, &scene, &label);
        out.push(RuleIssue {
            rule_id: "actor.never-on-frame",
            severity: Severity::Warning,
            confidence: Confidence::High,
            time_s: first_time,
            actor: Some(label.clone()),
            source_line: line,
            message: format!("`{label}` is evaluated but its bounds never intersect the canvas."),
            evidence: serde_json::json!({
                "bounds": bounds_json(bounds),
                "canvas": [facts.target.width, facts.target.height],
                "scene": scene,
            }),
            suggestion: "Move the actor inside the canvas bounds, or remove it if it is no longer \
                         part of the scene."
                .to_string(),
        });
    }
}

fn rule_clipped(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let mut worst: BTreeMap<(String, String), (f64, Box2, f64, bool)> = BTreeMap::new();
    for frame in facts.all_frames() {
        for actor_frame in &frame.actors {
            let Some(actor) = facts.actor(&actor_frame.scene, &actor_frame.label) else {
                continue;
            };
            if !actor.visual || actor_frame.opacity.unwrap_or(0.0) <= 0.01 {
                continue;
            }
            let Some(bounds) = actor_frame.bounds else {
                continue;
            };
            if actor_frame.on_frame_fraction <= 0.0 || actor_frame.on_frame_fraction >= 0.98 {
                continue;
            }
            let entry = worst
                .entry((actor_frame.scene.clone(), actor_frame.label.clone()))
                .or_insert((1.0, bounds, frame.time_s, actor.is_text || actor.is_plot));
            if actor_frame.on_frame_fraction < entry.0 {
                *entry = (
                    actor_frame.on_frame_fraction,
                    bounds,
                    frame.time_s,
                    actor.is_text || actor.is_plot,
                );
            }
        }
    }
    for ((scene, label), (fraction, bounds, time_s, emphasis)) in worst {
        out.push(RuleIssue {
            rule_id: "actor.clipped",
            severity: if emphasis {
                Severity::Warning
            } else {
                Severity::Info
            },
            confidence: Confidence::High,
            time_s: Some(time_s),
            actor: Some(label.clone()),
            source_line: source_line(facts, &scene, &label),
            message: format!(
                "`{label}` is {:.1}% on frame at {time_s:.2}s ({:.1}% of its bounds is clipped).",
                fraction * 100.0,
                (1.0 - fraction) * 100.0
            ),
            evidence: serde_json::json!({
                "bounds": bounds_json(bounds),
                "canvas": [facts.target.width, facts.target.height],
                "on_frame_fraction": fraction,
            }),
            suggestion: "Move or resize the actor so its bounds stay inside the canvas."
                .to_string(),
        });
    }
}

fn rule_overlap_candidates(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    struct Candidate {
        time_s: f64,
        bounds_a: Box2,
        bounds_b: Box2,
        area: f64,
        ratio_a: f64,
        ratio_b: f64,
    }
    let mut best: BTreeMap<(String, String, String), Candidate> = BTreeMap::new();
    for frame in facts.all_frames() {
        let visible: Vec<&super::facts::ActorFrameFact> = frame
            .actors
            .iter()
            .filter(|actor_frame| {
                let Some(actor) = facts.actor(&actor_frame.scene, &actor_frame.label) else {
                    return false;
                };
                actor.visual
                    && actor_frame.opacity.unwrap_or(0.0) > 0.05
                    && actor_frame.on_frame_fraction > 0.0
                    && actor_frame.bounds.is_some()
            })
            .collect();
        for (index, a) in visible.iter().enumerate() {
            for b in visible.iter().skip(index + 1) {
                if a.scene != b.scene {
                    continue;
                }
                let Some(actor_a) = facts.actor(&a.scene, &a.label) else {
                    continue;
                };
                let Some(actor_b) = facts.actor(&b.scene, &b.label) else {
                    continue;
                };
                if overlap_excluded(actor_a, actor_b) {
                    continue;
                }
                let (Some(bounds_a), Some(bounds_b)) = (a.bounds, b.bounds) else {
                    continue;
                };
                let area_a = box_area(bounds_a);
                let area_b = box_area(bounds_b);
                if area_a <= f64::EPSILON || area_b <= f64::EPSILON {
                    continue;
                }
                let overlap = box_intersection_area(bounds_a, bounds_b);
                if overlap <= 1.0 {
                    continue;
                }
                let ratio_a = overlap / area_a;
                let ratio_b = overlap / area_b;
                if ratio_a.max(ratio_b) <= OVERLAP_RATIO_FLOOR {
                    continue;
                }
                let (label_a, label_b) = if a.label <= b.label {
                    (&a.label, &b.label)
                } else {
                    (&b.label, &a.label)
                };
                let key = (a.scene.clone(), label_a.clone(), label_b.clone());
                let replace = best.get(&key).is_none_or(|candidate| overlap > candidate.area);
                if replace {
                    best.insert(
                        key,
                        Candidate {
                            time_s: frame.time_s,
                            bounds_a,
                            bounds_b,
                            area: overlap,
                            ratio_a,
                            ratio_b,
                        },
                    );
                }
            }
        }
    }
    for ((scene, label_a, label_b), candidate) in best {
        out.push(RuleIssue {
            rule_id: "overlap.candidates",
            severity: Severity::Info,
            confidence: Confidence::Medium,
            time_s: Some(candidate.time_s),
            actor: Some(format!("{label_a}, {label_b}")),
            source_line: source_line(facts, &scene, &label_a),
            message: format!(
                "`{label_a}` and `{label_b}` overlap by {:.1}px² at {:.2}s; independent content \
                 may be colliding.",
                candidate.area, candidate.time_s
            ),
            evidence: serde_json::json!({
                "pair": [label_a, label_b],
                "overlap_area": candidate.area,
                "ratio_a": candidate.ratio_a,
                "ratio_b": candidate.ratio_b,
                "bounds_a": bounds_json(candidate.bounds_a),
                "bounds_b": bounds_json(candidate.bounds_b),
                "relation": "independent",
            }),
            suggestion: "Check whether the overlap is intentional (annotation, decoration, \
                         transition); if not, move one actor or add a container."
                .to_string(),
        });
    }
}

fn overlap_excluded(a: &ActorFact, b: &ActorFact) -> bool {
    a.ancestors.contains(&b.label)
        || b.ancestors.contains(&a.label)
        || a.stack_ancestors.iter().any(|stack| b.stack_ancestors.contains(stack))
        || (a.parent.is_some() && a.parent == b.parent && (a.parent_is_stack || b.parent_is_stack))
        || a.effect_scope_ancestors
            .iter()
            .any(|scope| b.effect_scope_ancestors.contains(scope))
        || a.group_ancestors.iter().any(|group| b.group_ancestors.contains(group))
        || a.is_effect_scope
        || b.is_effect_scope
        || a.is_callout
        || b.is_callout
        || a.is_legend
        || b.is_legend
}

fn rule_text_overflow(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    struct Candidate {
        time_s: f64,
        measured: f32,
        max_width: f32,
        overflow: String,
    }
    let mut worst: BTreeMap<(String, String), Candidate> = BTreeMap::new();
    for frame in facts.all_frames() {
        for actor_frame in &frame.actors {
            let Some(actor) = facts.actor(&actor_frame.scene, &actor_frame.label) else {
                continue;
            };
            if !actor.is_text {
                continue;
            }
            let (Some(max_width), Some(measured)) =
                (actor_frame.text_max_width, actor_frame.measured_text_width)
            else {
                continue;
            };
            if max_width <= 0.0 || measured <= max_width * 1.02 + 1.0 {
                continue;
            }
            let overflow = actor_frame.text_overflow.clone().unwrap_or_else(|| "visible".into());
            let key = (actor_frame.scene.clone(), actor_frame.label.clone());
            let replace = worst.get(&key).is_none_or(|candidate| {
                measured / max_width > candidate.measured / candidate.max_width
            });
            if replace {
                worst.insert(
                    key,
                    Candidate {
                        time_s: frame.time_s,
                        measured,
                        max_width,
                        overflow,
                    },
                );
            }
        }
    }
    for ((scene, label), candidate) in worst {
        out.push(RuleIssue {
            rule_id: "text.overflow",
            severity: if candidate.overflow == "visible" {
                Severity::Warning
            } else {
                Severity::Info
            },
            confidence: Confidence::High,
            time_s: Some(candidate.time_s),
            actor: Some(label.clone()),
            source_line: source_line(facts, &scene, &label),
            message: format!(
                "`{label}` measures {:.1}px wide against text_max_width {:.1}px \
                 (overflow: {}).",
                candidate.measured, candidate.max_width, candidate.overflow
            ),
            evidence: serde_json::json!({
                "measured_width": candidate.measured,
                "text_max_width": candidate.max_width,
                "overflow": candidate.overflow,
            }),
            suggestion: "Raise `text_max_width`, shorten the text, or use `overflow: ellipsis`."
                .to_string(),
        });
    }
}

fn rule_entrance_not_visible(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    for action in &facts.actions {
        if action.category != animatix::timeline::ActionCategory::Entrance {
            continue;
        }
        let after_s = action.start_s + action.duration_s;
        let Some(frame) = facts
            .frames
            .iter()
            .chain(facts.probe_frames.iter())
            .filter(|frame| frame.time_s + 0.005 >= after_s)
            .min_by(|a, b| a.time_s.total_cmp(&b.time_s))
        else {
            continue;
        };
        for target in &action.targets {
            let Some(actor_frame) = facts.actor_frame(frame, &action.scene, target) else {
                continue;
            };
            let opacity = actor_frame.opacity.unwrap_or(0.0);
            let visible = opacity > 0.01
                && actor_frame.bounds.is_some()
                && actor_frame.on_frame_fraction > 0.0;
            if visible {
                continue;
            }
            out.push(RuleIssue {
                rule_id: "entrance.not-visible",
                severity: Severity::Warning,
                confidence: Confidence::High,
                time_s: Some(frame.time_s),
                actor: Some(target.clone()),
                source_line: source_line(facts, &action.scene, target),
                message: format!(
                    "`{target}` is still not visible at {:.2}s after `{}` (opacity {:.2}).",
                    frame.time_s, action.verb, opacity
                ),
                evidence: serde_json::json!({
                    "verb": action.verb,
                    "start_s": action.start_s,
                    "duration_s": action.duration_s,
                    "probe_time_s": frame.time_s,
                    "opacity": opacity,
                    "bounds": actor_frame.bounds.map(bounds_json),
                    "on_frame_fraction": actor_frame.on_frame_fraction,
                }),
                suggestion: "Extend the entrance, raise its end opacity, or move the actor on \
                             frame before the entrance ends."
                    .to_string(),
            });
        }
    }
}

fn rule_keyframe_redundant(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    for fact in facts.redundant_keyframes.iter().take(40) {
        out.push(RuleIssue {
            rule_id: "keyframe.redundant",
            severity: Severity::Info,
            confidence: Confidence::High,
            time_s: Some(fact.time_s),
            actor: Some(fact.label.clone()),
            source_line: source_line(facts, &fact.scene, &fact.label),
            message: format!(
                "`{}` keyframe at {:.2}s repeats the value from {:.2}s (property `{}`).",
                fact.label, fact.time_s, fact.previous_time_s, fact.property
            ),
            evidence: serde_json::json!({
                "property": fact.property,
                "previous_time_s": fact.previous_time_s,
                "time_s": fact.time_s,
            }),
            suggestion: "Remove the redundant keyframe or change its value.".to_string(),
        });
    }
}

fn rule_frame_blank(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
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

fn rule_beat_static(facts: &ReviewFacts, out: &mut Vec<RuleIssue>, skipped: &mut usize) {
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

fn rule_transition_endpoint(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
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

fn rule_site_rest(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let Some(rest) = &facts.site_rest else {
        return;
    };
    if rest.held_ink_fraction < BLANK_INK_FRACTION {
        out.push(RuleIssue {
            rule_id: "rest.ink-floor",
            severity: Severity::Warning,
            confidence: Confidence::High,
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
        && rest.held_ink_fraction < REST_FLOOR * rest.peak_ink_fraction
    {
        out.push(RuleIssue {
            rule_id: "rest.cast-erased",
            severity: Severity::Warning,
            confidence: Confidence::High,
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
                "rest_floor": REST_FLOOR,
            }),
            suggestion: "Stop the closing fade before the hold, or make the final beat animate \
                         parameters instead of erasing the cast."
                .to_string(),
        });
    }
}

fn box_area(bounds: Box2) -> f64 {
    (bounds.x1 - bounds.x0).max(0.0) * (bounds.y1 - bounds.y0).max(0.0)
}

fn box_intersection_area(a: Box2, b: Box2) -> f64 {
    let width = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let height = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    width * height
}

fn bounds_json(bounds: Box2) -> serde_json::Value {
    serde_json::json!([bounds.x0, bounds.y0, bounds.x1, bounds.y1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::facts::{
        ActorFact, ActorFrameFact, BuildDiagnosticFact, FrameFact, ReviewFacts, TargetFacts,
    };
    use crate::review::types::{RenderFacts, RenderSampleFacts, TargetKind};

    fn base_facts() -> ReviewFacts {
        ReviewFacts {
            target: TargetFacts {
                kind: TargetKind::SingleScene,
                duration_s: 2.0,
                width: 100,
                height: 100,
            },
            frames: Vec::new(),
            actors: Vec::new(),
            actions: Vec::new(),
            probe_frames: Vec::new(),
            build_diagnostics: Vec::new(),
            redundant_keyframes: Vec::new(),
            has_modifier_programs: false,
            render: None,
            site_rest: None,
        }
    }

    fn actor(label: &str) -> ActorFact {
        ActorFact {
            scene: "main".into(),
            label: label.into(),
            source_line: Some(3),
            parent: None,
            ancestors: Vec::new(),
            group_ancestors: Vec::new(),
            stack_ancestors: Vec::new(),
            effect_scope_ancestors: Vec::new(),
            parent_is_stack: false,
            max_effective_opacity: 0.0,
            visual: true,
            is_text: false,
            is_plot: false,
            is_effect_scope: false,
            is_callout: false,
            is_legend: false,
        }
    }

    fn actor_frame(label: &str, opacity: f32, bounds: Option<Box2>) -> ActorFrameFact {
        ActorFrameFact {
            scene: "main".into(),
            label: label.into(),
            opacity: Some(opacity),
            bounds,
            on_frame_fraction: if bounds.is_some() { 1.0 } else { 0.0 },
            text_max_width: None,
            text_overflow: None,
            measured_text_width: None,
        }
    }

    fn frame(time_s: f64, actors: Vec<ActorFrameFact>) -> FrameFact {
        FrameFact {
            time_s,
            scene: "main".into(),
            ink_fraction: None,
            cast: actors.len(),
            actors,
        }
    }

    #[test]
    fn opacity_rule_flags_actors_never_positive() {
        let mut facts = base_facts();
        facts.actors = vec![actor("ghost"), actor("solid")];
        facts.frames = vec![frame(
            0.0,
            vec![
                actor_frame("ghost", 0.0, None),
                actor_frame("solid", 1.0, None),
            ],
        )];
        let run = evaluate(&facts, Policy::Default);
        assert!(run.issues.iter().any(|issue| issue.rule_id == "actor.opacity-never-positive"
            && issue.actor.as_deref() == Some("ghost")));
        assert!(!run.issues.iter().any(|issue| issue.actor.as_deref() == Some("solid")));
    }

    #[test]
    fn never_on_frame_flags_fully_off_canvas_bounds() {
        let mut facts = base_facts();
        facts.actors = vec![actor("offscreen")];
        let mut offscreen =
            actor_frame("offscreen", 1.0, Some(Box2::new(200.0, 200.0, 250.0, 250.0)));
        offscreen.on_frame_fraction = 0.0;
        facts.frames = vec![frame(0.0, vec![offscreen])];
        let run = evaluate(&facts, Policy::Default);
        assert!(run.issues.iter().any(|issue| issue.rule_id == "actor.never-on-frame"));
    }

    #[test]
    fn clipped_rule_prefers_warning_for_text() {
        let mut facts = base_facts();
        let mut text = actor("caption");
        text.is_text = true;
        facts.actors = vec![text];
        let mut frame_actor = actor_frame("caption", 1.0, Some(Box2::new(-10.0, 10.0, 50.0, 50.0)));
        frame_actor.on_frame_fraction = 0.8;
        facts.frames = vec![frame(0.0, vec![frame_actor])];
        let run = evaluate(&facts, Policy::Default);
        let issue = run
            .issues
            .iter()
            .find(|issue| issue.rule_id == "actor.clipped")
            .expect("clipped issue");
        assert_eq!(issue.severity, Severity::Warning);
    }

    #[test]
    fn overlap_rule_skips_ancestor_and_same_stack_pairs() {
        let mut facts = base_facts();
        let mut parent = actor("card");
        parent.visual = false;
        let mut child = actor("label");
        child.ancestors = vec!["card".into()];
        let mut sibling = actor("badge");
        sibling.parent = Some("card".into());
        sibling.parent_is_stack = true;
        child.parent = Some("card".into());
        facts.actors = vec![parent, child, sibling];
        facts.frames = vec![frame(
            0.0,
            vec![
                actor_frame("label", 1.0, Some(Box2::new(0.0, 0.0, 50.0, 50.0))),
                actor_frame("badge", 1.0, Some(Box2::new(10.0, 10.0, 40.0, 40.0))),
            ],
        )];
        let run = evaluate(&facts, Policy::Default);
        assert!(!run.issues.iter().any(|issue| issue.rule_id == "overlap.candidates"));
    }

    #[test]
    fn overlap_rule_flags_independent_content() {
        let mut facts = base_facts();
        facts.actors = vec![actor("a"), actor("b")];
        facts.frames = vec![frame(
            0.0,
            vec![
                actor_frame("a", 1.0, Some(Box2::new(0.0, 0.0, 50.0, 50.0))),
                actor_frame("b", 1.0, Some(Box2::new(10.0, 10.0, 40.0, 40.0))),
            ],
        )];
        let run = evaluate(&facts, Policy::Default);
        assert!(run.issues.iter().any(|issue| issue.rule_id == "overlap.candidates"));
    }

    #[test]
    fn text_overflow_rule_uses_max_width_and_overflow_mode() {
        let mut facts = base_facts();
        let mut text = actor("title");
        text.is_text = true;
        facts.actors = vec![text];
        let mut frame_actor = actor_frame("title", 1.0, Some(Box2::new(0.0, 0.0, 200.0, 20.0)));
        frame_actor.text_max_width = Some(100.0);
        frame_actor.text_overflow = Some("visible".into());
        frame_actor.measured_text_width = Some(200.0);
        facts.frames = vec![frame(0.0, vec![frame_actor])];
        let run = evaluate(&facts, Policy::Default);
        let issue = run
            .issues
            .iter()
            .find(|issue| issue.rule_id == "text.overflow")
            .expect("text overflow");
        assert_eq!(issue.severity, Severity::Warning);
    }

    #[test]
    fn entrance_rule_flags_actor_not_visible_after_action() {
        let mut facts = base_facts();
        facts.actors = vec![actor("title")];
        facts.actions = vec![crate::review::facts::ActionFact {
            scene: "main".into(),
            verb: "fade-in".into(),
            targets: vec!["title".into()],
            start_s: 0.0,
            duration_s: 1.0,
            category: animatix::timeline::ActionCategory::Entrance,
        }];
        facts.frames = vec![
            frame(0.0, vec![actor_frame("title", 0.0, None)]),
            frame(1.05, vec![actor_frame("title", 0.0, None)]),
        ];
        let run = evaluate(&facts, Policy::Default);
        assert!(run.issues.iter().any(|issue| issue.rule_id == "entrance.not-visible"));
    }

    #[test]
    fn never_revealed_rule_maps_build_diagnostic() {
        let mut facts = base_facts();
        facts.build_diagnostics = vec![BuildDiagnosticFact {
            code: "never-revealed".into(),
            severity: Severity::Warning,
            message: "Actor `ghost` is declared before any keyframe".into(),
            actor: Some("ghost".into()),
            source_line: None,
        }];
        let run = evaluate(&facts, Policy::Default);
        assert!(run.issues.iter().any(|issue| issue.rule_id == "build.never-revealed"));
    }

    #[test]
    fn render_rules_skip_without_render_facts() {
        let facts = base_facts();
        let run = evaluate(&facts, Policy::Default);
        assert_eq!(run.skipped, 3);
        assert!(run.issues.iter().all(|issue| !issue.rule_id.starts_with("frame.")));
    }

    #[test]
    fn frame_blank_rule_uses_render_ink() {
        let mut facts = base_facts();
        facts.render = Some(RenderFacts {
            status: RenderStatus::Rendered,
            reason: "test".into(),
            samples: vec![RenderSampleFacts {
                time_s: 1.0,
                ink_fraction: 0.0,
                changed_from_prev: None,
                actor_ink: Default::default(),
                transition: None,
            }],
            site_rest: None,
        });
        let run = evaluate(&facts, Policy::Default);
        assert!(run.issues.iter().any(|issue| issue.rule_id == "frame.blank"));
    }

    #[test]
    fn site_rest_rule_ports_the_035_plateau() {
        let mut facts = base_facts();
        facts.render = Some(RenderFacts {
            status: RenderStatus::Rendered,
            reason: "test".into(),
            samples: vec![],
            site_rest: Some(crate::review::types::SiteRestFacts {
                held_time_s: 2.0,
                held_ink_fraction: 0.01,
                peak_ink_fraction: 0.10,
                peak_time_s: 1.0,
            }),
        });
        facts.site_rest = facts.render.as_ref().and_then(|render| render.site_rest.clone());
        let run = evaluate(&facts, Policy::Site);
        assert!(run.issues.iter().any(|issue| issue.rule_id == "rest.cast-erased"));
    }
}
