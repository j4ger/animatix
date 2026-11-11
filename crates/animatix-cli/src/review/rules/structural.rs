use super::{OVERLAP_RATIO_FLOOR, source_line};
use crate::frame_source::Box2;
use crate::review::facts::{ActorFact, FrameFact, ReviewFacts};
use crate::review::types::{Persistence, RuleIssue, Severity};
use std::collections::BTreeMap;

fn bounds_json(bounds: Box2) -> serde_json::Value {
    serde_json::json!([bounds.x0, bounds.y0, bounds.x1, bounds.y1])
}

pub(crate) fn rule_build_never_revealed(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
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
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
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

pub(crate) fn rule_opacity_never_positive(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
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
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
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

pub(crate) fn rule_never_on_frame(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    #[derive(Default)]
    struct NeverOnFrameMeasurement {
        max_opacity: f32,
        first_bounds: Option<Box2>,
        first_time: Option<f64>,
    }
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
                actor_frame.opacity.unwrap_or(0.0) > 0.01
                    && actor_frame.padded_on_frame_fraction > 0.0
            })
        });
        if ever_on_frame {
            continue;
        }
        let line = source_line(facts, &scene, &label);
        out.push(RuleIssue {
            rule_id: "actor.never-on-frame",
            severity: Severity::Warning,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
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

pub(crate) fn rule_clipped(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    let mut worst: BTreeMap<(String, String), (f64, Box2, f64, bool)> = BTreeMap::new();
    for frame in facts.all_frames() {
        for actor_frame in &frame.actors {
            let Some(actor) = facts.actor(&actor_frame.scene, &actor_frame.label) else {
                continue;
            };
            // Match the engine's build-time sampler: explicitly non-content
            // actors (`legend: false` / hidden) and full-viewport backgrounds
            // are decorative layers whose out-of-frame extents are intentional.
            if !actor.visual
                || actor.is_non_content
                || actor.is_background
                || actor_frame.opacity.unwrap_or(0.0) <= 0.01
            {
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
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
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

struct OverlapCandidate {
    time_s: f64,
    bounds_a: Box2,
    bounds_b: Box2,
    area: f64,
    ratio_a: f64,
    ratio_b: f64,
    relation: &'static str,
}

struct PairCandidate {
    label_a: String,
    label_b: String,
    candidate: OverlapCandidate,
}

/// Largest overlap per unordered actor pair, grouped per scene into
/// `(independent, explained)` buckets.
fn collect_overlap_candidates(
    facts: &ReviewFacts,
) -> BTreeMap<String, (Vec<PairCandidate>, Vec<PairCandidate>)> {
    let mut best: BTreeMap<(String, String, String), OverlapCandidate> = BTreeMap::new();
    for frame in facts.all_frames() {
        let visible: Vec<&crate::review::facts::ActorFrameFact> = frame
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
                let relation = overlap_relation(actor_a, actor_b, frame);
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
                        OverlapCandidate {
                            time_s: frame.time_s,
                            bounds_a,
                            bounds_b,
                            area: overlap,
                            ratio_a,
                            ratio_b,
                            relation,
                        },
                    );
                }
            }
        }
    }
    let mut by_scene: BTreeMap<String, (Vec<PairCandidate>, Vec<PairCandidate>)> = BTreeMap::new();
    for ((scene, label_a, label_b), candidate) in best {
        let is_independent = candidate.relation == "independent";
        let pair = PairCandidate {
            label_a,
            label_b,
            candidate,
        };
        let entry = by_scene.entry(scene).or_default();
        if is_independent {
            entry.0.push(pair);
        } else {
            entry.1.push(pair);
        }
    }
    by_scene
}

/// Sort by descending overlap area with a stable label tie-break.
fn sort_by_area(pairs: &mut [PairCandidate]) {
    pairs.sort_by(|a, b| {
        b.candidate
            .area
            .total_cmp(&a.candidate.area)
            .then_with(|| a.label_a.cmp(&b.label_a))
            .then_with(|| a.label_b.cmp(&b.label_b))
    });
}

fn push_overlap_pair(
    facts: &ReviewFacts,
    scene: &str,
    pair: &PairCandidate,
    rule_id: &'static str,
    severity: Severity,
    out: &mut Vec<RuleIssue>,
) {
    let relation = pair.candidate.relation;
    out.push(RuleIssue {
        rule_id,
        severity,
        persistence: Persistence::Held,
        fix_hint: None,
        waived_by: None,
        scene: Some(scene.to_string()),
        time_s: Some(pair.candidate.time_s),
        actor: Some(format!("{}, {}", pair.label_a, pair.label_b)),
        source_line: source_line(facts, scene, &pair.label_a),
        message: format!(
            "`{}` and `{}` overlap by {:.1}px² at {:.2}s ({relation}).",
            pair.label_a, pair.label_b, pair.candidate.area, pair.candidate.time_s
        ),
        evidence: serde_json::json!({
            "pair": [&pair.label_a, &pair.label_b],
            "overlap_area": pair.candidate.area,
            "ratio_a": pair.candidate.ratio_a,
            "ratio_b": pair.candidate.ratio_b,
            "bounds_a": bounds_json(pair.candidate.bounds_a),
            "bounds_b": bounds_json(pair.candidate.bounds_b),
            "relation": pair.candidate.relation,
        }),
        suggestion: "Check whether the overlap is intentional (annotation, decoration, \
                     transition); if not, move one actor or add a container."
            .to_string(),
    });
}

fn push_overlap_cluster(
    scene: &str,
    independent_total: usize,
    sample_pairs: Vec<[String; 2]>,
    max_time_s: f64,
    out: &mut Vec<RuleIssue>,
) {
    out.push(RuleIssue {
        rule_id: "overlap.cluster",
        severity: Severity::Warning,
        persistence: Persistence::Held,
        fix_hint: None,
        waived_by: None,
        scene: Some(scene.to_string()),
        time_s: Some(max_time_s),
        actor: None,
        source_line: None,
        message: format!(
            "{independent_total} independent overlapping actor pairs in scene `{scene}`; capped to top 24 largest overlaps."
        ),
        evidence: serde_json::json!({
            "pair_count": independent_total,
            "sample_pairs": sample_pairs,
            "relation": "independent",
            "kept": 24,
        }),
        suggestion: "Organize overlapping actors with a layout container (Row/Col/Stack/Grid) or add local layout waivers (`// layout: allow-overlap`).".to_string(),
    });
}

pub(crate) fn rule_overlap_candidates(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    for (scene, (mut independent, mut explained)) in collect_overlap_candidates(facts) {
        let independent_total = independent.len();
        if independent_total > 24 {
            sort_by_area(&mut independent);
            let sample_pairs: Vec<[String; 2]> = independent
                .iter()
                .take(5)
                .map(|p| [p.label_a.clone(), p.label_b.clone()])
                .collect();
            independent.truncate(24);
            let max_time_s = independent.iter().map(|p| p.candidate.time_s).fold(0.0_f64, f64::max);
            for pair in &independent {
                push_overlap_pair(
                    facts,
                    &scene,
                    pair,
                    "overlap.undeclared",
                    Severity::Warning,
                    out,
                );
            }
            push_overlap_cluster(&scene, independent_total, sample_pairs, max_time_s, out);
        } else {
            for pair in &independent {
                push_overlap_pair(
                    facts,
                    &scene,
                    pair,
                    "overlap.undeclared",
                    Severity::Warning,
                    out,
                );
            }
        }

        if explained.len() > 24 {
            sort_by_area(&mut explained);
            explained.truncate(24);
        }
        for pair in &explained {
            push_overlap_pair(facts, &scene, pair, "overlap.explained", Severity::Info, out);
        }
    }
}

pub(crate) fn overlap_relation(a: &ActorFact, b: &ActorFact, frame: &FrameFact) -> &'static str {
    if a.ancestors.contains(&b.label) || b.ancestors.contains(&a.label) {
        return "ancestor";
    }
    if a.stack_ancestors.iter().any(|stack| b.stack_ancestors.contains(stack)) {
        return "same_stack";
    }
    if a.parent.is_some() && a.parent == b.parent && (a.parent_is_stack || b.parent_is_stack) {
        return "same_stack";
    }
    if a.effect_scope_ancestors
        .iter()
        .any(|scope| b.effect_scope_ancestors.contains(scope))
    {
        // One Filter/Glass scope composites its children as a unit.
        return "same_stack";
    }
    if a.group_ancestors.iter().any(|group| b.group_ancestors.contains(group)) {
        // Same Row/Col/Grid/Group: the layout engine placed these siblings, so
        // overlapping bounds are structural, not an undeclared collision.
        return "same_stack";
    }
    if a.is_callout || b.is_callout {
        return "callout_target";
    }
    if a.is_legend || b.is_legend {
        return "legend_host";
    }
    if a.in_stack && (b.is_non_content || b.is_background) {
        return "stack_layer";
    }
    if b.in_stack && (a.is_non_content || a.is_background) {
        return "stack_layer";
    }
    if a.is_non_content || a.is_background || b.is_non_content || b.is_background {
        return "decoration_layer";
    }
    if frame.scene.contains(" -> ") {
        return "during_transition";
    }
    if a.label.contains('.')
        || b.label.contains('.')
        || (a.source_line.is_none() && b.source_line.is_none())
    {
        return "generated_batch";
    }
    "independent"
}

pub(crate) fn box_area(bounds: Box2) -> f64 {
    (bounds.x1 - bounds.x0).max(0.0) * (bounds.y1 - bounds.y0).max(0.0)
}

pub(crate) fn box_intersection_area(a: Box2, b: Box2) -> f64 {
    let width = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let height = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    width * height
}

pub(crate) fn rule_text_overflow(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
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
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
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

pub(crate) fn rule_entrance_not_visible(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
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
                && actor_frame.padded_on_frame_fraction > 0.0;
            if visible {
                continue;
            }
            out.push(RuleIssue {
                rule_id: "entrance.not-visible",
                severity: Severity::Warning,
                persistence: Persistence::Held,
                fix_hint: None,
                waived_by: None,
                scene: None,
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

pub(crate) fn rule_keyframe_redundant(facts: &ReviewFacts, out: &mut Vec<RuleIssue>) {
    for fact in facts.redundant_keyframes.iter().take(40) {
        out.push(RuleIssue {
            rule_id: "keyframe.redundant",
            severity: Severity::Info,
            persistence: Persistence::Held,
            fix_hint: None,
            waived_by: None,
            scene: None,
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
