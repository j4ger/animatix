//! Structural fact collection for `animatix review`.
//!
//! The structural tier evaluates `SceneProgram`s on the CPU with no filter
//! backend. Facts separate what the engine measured (bounds, inherited
//! opacity, text metrics, action events, keyframes) from what rules judge, so
//! every finding can quote the measurements it came from.

use std::collections::BTreeMap;

use animatix::composition::BuildTarget;
use animatix::timeline::{
    ActionCategory, ActorCategory, PROPERTY_REGISTRY, PropertyValue, SceneDimensions, TrackAccessor,
};
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticSeverity};

use crate::diagnostics::backticked_label;
use crate::frame_source::{BOUNDS_PAD, Box2, evaluate_program_frame};

use super::types::{RenderFacts, Severity, SiteRestFacts, TargetKind};

/// Static identity and structure of one actor.
#[derive(Clone, Debug)]
pub(crate) struct ActorFact {
    /// Scene owning the actor (`main` for single-scene files).
    pub scene: String,
    /// Actor label.
    pub label: String,
    /// 1-based declaration line, when available.
    pub source_line: Option<usize>,
    /// Immediate parent label.
    pub parent: Option<String>,
    /// Labels of every ancestor, nearest first.
    pub ancestors: Vec<String>,
    /// Labels of every group-like ancestor.
    pub group_ancestors: Vec<String>,
    /// Labels of every `Stack` ancestor.
    pub stack_ancestors: Vec<String>,
    /// Labels of every effect-scope ancestor.
    pub effect_scope_ancestors: Vec<String>,
    /// Whether the immediate parent is a `Stack`.
    pub parent_is_stack: bool,
    /// Maximum effective opacity over every keyframe time in the scene.
    pub max_effective_opacity: f32,
    /// Directly renders visual ink.
    pub visual: bool,
    /// Container that lays out or groups children rather than drawing itself.
    pub is_group_like: bool,
    /// Primitive category (shape, text, media, plot, container, annotation).
    pub category: ActorCategory,
    /// Text-engine actor.
    pub is_text: bool,
    /// Plot/chart actor.
    pub is_plot: bool,
    /// Filter/glass effect scope.
    pub is_effect_scope: bool,
    /// Callout annotation.
    pub is_callout: bool,
    /// Legend host.
    pub is_legend: bool,
}

/// Per-actor measurements at one sample.
#[derive(Clone, Debug, Default)]
pub(crate) struct ActorFrameFact {
    /// Scene owning the actor.
    pub scene: String,
    /// Actor label.
    pub label: String,
    /// Effective inherited opacity, when evaluated.
    pub opacity: Option<f32>,
    /// World-space bounds, when the actor recorded them.
    pub bounds: Option<Box2>,
    /// Fraction of the actor's bounds that lies inside the canvas.
    pub on_frame_fraction: f64,
    /// Fraction of the padded bounds that lies inside the canvas. Zero-area
    /// primitives such as `Line` and `Arrow` need padding before this measure
    /// is meaningful.
    pub padded_on_frame_fraction: f64,
    /// Number of `SceneItem`s this actor emitted at this sample, when known.
    pub draw_items: Option<u32>,
    /// Number of `RenderCommand`s those items emitted, when known.
    pub draw_commands: Option<u32>,
    /// Evaluated `text_max_width` (0 = no wrap).
    pub text_max_width: Option<f32>,
    /// Evaluated `overflow` mode.
    pub text_overflow: Option<String>,
    /// Measured world-space text width, from bounds.
    pub measured_text_width: Option<f32>,
}

/// Measurements at one sample time.
#[derive(Clone, Debug, Default)]
pub(crate) struct FrameFact {
    /// Sample time in seconds.
    pub time_s: f64,
    /// Active scene label (`a -> b` during a transition).
    pub scene: String,
    /// Visual actors with positive opacity and on-frame bounds.
    pub cast: usize,
    /// Whole-frame ink fraction when the render tier measured this sample.
    pub ink_fraction: Option<f32>,
    /// Structural state digest used by `beat.static` without a renderer.
    pub state_digest: Option<u64>,
    /// Per-actor measurements.
    pub actors: Vec<ActorFrameFact>,
}

/// One build/action event retained for the entrance rule.
#[derive(Clone, Debug)]
pub(crate) struct ActionFact {
    /// Scene owning the event.
    pub scene: String,
    /// Action verb (`fade-in`, `wipe-in`, ...).
    pub verb: String,
    /// Target actor labels.
    pub targets: Vec<String>,
    /// Start time in seconds (global for multi-scene files).
    pub start_s: f64,
    /// Duration in seconds.
    pub duration_s: f64,
    /// Engine action category.
    pub category: ActionCategory,
}

/// One build diagnostic retained for `build.never-revealed`.
#[derive(Clone, Debug)]
pub(crate) struct BuildDiagnosticFact {
    /// Stable diagnostic code.
    pub code: String,
    /// Mapped severity.
    pub severity: Severity,
    /// Full diagnostic message.
    pub message: String,
    /// Actor label quoted by the diagnostic, when present.
    pub actor: Option<String>,
    /// 1-based line, when the diagnostic carried one.
    pub source_line: Option<usize>,
}

/// A consecutive keyframe pair whose sampled value never changes.
#[derive(Clone, Debug)]
pub(crate) struct RedundantKeyframeFact {
    /// Scene owning the actor.
    pub scene: String,
    /// Actor label.
    pub label: String,
    /// Property name.
    pub property: String,
    /// Time of the earlier keyframe.
    pub previous_time_s: f64,
    /// Time of the redundant keyframe.
    pub time_s: f64,
}

/// Target-level identity.
#[derive(Clone, Debug)]
pub(crate) struct TargetFacts {
    /// Single vs multi scene.
    pub kind: TargetKind,
    /// Playback duration.
    pub duration_s: f64,
    /// Canvas width.
    pub width: u32,
    /// Canvas height.
    pub height: u32,
}

/// Complete structural fact set handed to the rule engine.
#[derive(Clone, Debug)]
pub(crate) struct ReviewFacts {
    /// Target identity.
    pub target: TargetFacts,
    /// Structural samples (reported in `frames`).
    pub frames: Vec<FrameFact>,
    /// Extra rule probes (e.g. entrance end times) that are not report frames.
    pub probe_frames: Vec<FrameFact>,
    /// Static actor inventory.
    pub actors: Vec<ActorFact>,
    /// Entrance/action metadata.
    pub actions: Vec<ActionFact>,
    /// Build diagnostics.
    pub build_diagnostics: Vec<BuildDiagnosticFact>,
    /// Candidate redundant keyframes.
    pub redundant_keyframes: Vec<RedundantKeyframeFact>,
    /// Whether any timeline carries `always` modifier programs.
    pub has_modifier_programs: bool,
    /// Render-tier facts, filled after the render pass.
    pub render: Option<RenderFacts>,
    /// Site-policy resting measurements, filled after the render pass.
    pub site_rest: Option<SiteRestFacts>,
}

impl ReviewFacts {
    /// Look up an actor by scene and label.
    pub(crate) fn actor(&self, scene: &str, label: &str) -> Option<&ActorFact> {
        self.actors.iter().find(|actor| actor.scene == scene && actor.label == label)
    }

    /// All measured frames: reported samples first, then extra rule probes.
    pub(crate) fn all_frames(&self) -> impl Iterator<Item = &FrameFact> {
        self.frames.iter().chain(self.probe_frames.iter())
    }

    /// Look up an actor's frame measurements.
    pub(crate) fn actor_frame<'a>(
        &self,
        frame: &'a FrameFact,
        scene: &str,
        label: &str,
    ) -> Option<&'a ActorFrameFact> {
        frame.actors.iter().find(|actor| actor.scene == scene && actor.label == label)
    }
}

/// Properties whose repeated equal keyframes are worth an informational note.
const AUDITED_KEYFRAME_PROPERTIES: &[&str] = &[
    "opacity",
    "position",
    "scale",
    "rotation",
    "size",
    "color",
    "fill_opacity",
    "stroke_width",
    "stroke_progress",
    "text_max_width",
    "char_progress",
];

/// Collect structural facts for every sample.
pub(crate) fn collect_facts(
    target: &BuildTarget,
    dimensions: SceneDimensions,
    base_times: Vec<f64>,
    build_diagnostics: &[Diagnostic],
) -> Result<ReviewFacts, String> {
    let mut actors = Vec::new();
    let mut actions = Vec::new();
    let mut has_modifier_programs = false;
    match target {
        BuildTarget::SingleScene(timeline) => {
            collect_actors_from_timeline("main", timeline, &mut actors);
            collect_actions("main", timeline, 0.0, &mut actions);
            has_modifier_programs |= !timeline.modifier_programs.is_empty();
        },
        BuildTarget::MultiScene(composition) => {
            for (name, scene) in &composition.scenes {
                let start_s = composition.scene_start_times.get(name).copied().unwrap_or(0.0);
                collect_actors_from_timeline(name, &scene.timeline, &mut actors);
                collect_actions(name, &scene.timeline, start_s, &mut actions);
                has_modifier_programs |= !scene.timeline.modifier_programs.is_empty();
            }
        },
    }

    let mut frames = Vec::with_capacity(base_times.len());
    for &time_s in &base_times {
        frames.push(evaluate_frame(target, dimensions, time_s, &actors)?);
    }

    let duration_s = super::samples::target_duration_s(target);
    let mut probe_times = Vec::new();
    for action in &actions {
        if matches!(action.category, ActionCategory::Reorder) {
            continue;
        }
        let probe = (action.start_s + action.duration_s + 0.05).max(0.0).min(duration_s);
        if !base_times.iter().any(|sample| (*sample - probe).abs() < 0.001) {
            probe_times.push(probe);
        }
    }
    probe_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    probe_times.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    let mut probe_frames = Vec::with_capacity(probe_times.len());
    for &time_s in &probe_times {
        probe_frames.push(evaluate_frame(target, dimensions, time_s, &actors)?);
    }

    let redundant_keyframes = collect_redundant_keyframes(target);
    let build_diagnostics = build_diagnostics
        .iter()
        .map(|diagnostic| BuildDiagnosticFact {
            code: diagnostic.code.to_string(),
            severity: match diagnostic.severity {
                DiagnosticSeverity::Error => Severity::Error,
                DiagnosticSeverity::Warning => Severity::Warning,
                DiagnosticSeverity::Info | DiagnosticSeverity::Hint => Severity::Info,
            },
            message: diagnostic.message.clone(),
            actor: backticked_label(&diagnostic.message),
            source_line: diagnostic.location.line,
        })
        .collect();

    let target_kind = match target {
        BuildTarget::SingleScene(_) => TargetKind::SingleScene,
        BuildTarget::MultiScene(_) => TargetKind::MultiScene,
    };
    Ok(ReviewFacts {
        target: TargetFacts {
            kind: target_kind,
            duration_s: super::samples::target_duration_s(target),
            width: dimensions.width,
            height: dimensions.height,
        },
        frames,
        probe_frames,
        actors,
        actions,
        build_diagnostics,
        redundant_keyframes,
        has_modifier_programs,
        render: None,
        site_rest: None,
    })
}

fn collect_actors_from_timeline(
    scene: &str,
    timeline: &animatix::timeline::Timeline,
    out: &mut Vec<ActorFact>,
) {
    let world = timeline.world();
    let mut opacity_times: Vec<u64> = timeline
        .keyframe_times_s()
        .into_iter()
        .map(|time_s| (time_s * 1000.0).round() as u64)
        .collect();
    opacity_times.push(0);
    for label in timeline.actor_labels() {
        let Some(id) = world.get_id(label) else {
            continue;
        };
        let caps = world.caps(id);
        let mut ancestors = Vec::new();
        let mut group_ancestors = Vec::new();
        let mut stack_ancestors = Vec::new();
        let mut effect_scope_ancestors = Vec::new();
        let mut parent = None;
        let mut cursor = world.hierarchy(id).parent;
        while let Some(parent_id) = cursor {
            if let Some(parent_label) = world.label(parent_id) {
                ancestors.push(parent_label.to_string());
                let parent_caps = world.caps(parent_id);
                if parent_caps.is_group_like() {
                    group_ancestors.push(parent_label.to_string());
                }
                if world.actor_type(parent_id) == "Stack" {
                    stack_ancestors.push(parent_label.to_string());
                }
                if parent_caps.is_effect_scope() {
                    effect_scope_ancestors.push(parent_label.to_string());
                }
                if parent.is_none() {
                    parent = Some(parent_label.to_string());
                }
            }
            cursor = world.hierarchy(parent_id).parent;
        }
        let parent_is_stack = world
            .hierarchy(id)
            .parent
            .is_some_and(|parent_id| world.actor_type(parent_id) == "Stack");
        let max_effective_opacity = opacity_times
            .iter()
            .map(|time_ms| track_product(timeline, id, *time_ms))
            .fold(0.0f32, f32::max);
        out.push(ActorFact {
            scene: scene.to_string(),
            label: label.clone(),
            source_line: world.decl_span(id).map(|span| span.start_line),
            parent,
            ancestors,
            max_effective_opacity,
            group_ancestors,
            stack_ancestors,
            effect_scope_ancestors,
            parent_is_stack,
            visual: caps.renders_visual_content(),
            is_group_like: caps.is_group_like(),
            category: caps.category,
            is_text: caps.text.is_some() || caps.text_paths,
            is_plot: caps.category == ActorCategory::Plot,
            is_effect_scope: caps.is_effect_scope(),
            is_callout: caps.callout,
            is_legend: caps.legend_host,
        });
    }
}

fn collect_actions(
    scene: &str,
    timeline: &animatix::timeline::Timeline,
    scene_start_s: f64,
    out: &mut Vec<ActionFact>,
) {
    for event in &timeline.action_events {
        out.push(ActionFact {
            scene: scene.to_string(),
            verb: event.verb.clone(),
            targets: event.targets.clone(),
            start_s: scene_start_s + event.start_time_ms as f64 / 1000.0,
            duration_s: event.duration_ms as f64 / 1000.0,
            category: event.category,
        });
    }
}

fn evaluate_frame(
    target: &BuildTarget,
    dimensions: SceneDimensions,
    time_s: f64,
    actors: &[ActorFact],
) -> Result<FrameFact, String> {
    let program_frame = evaluate_program_frame(target, dimensions, time_s)?;
    let time_ms = (time_s * 1000.0) as u64;
    let mut by_actor: BTreeMap<(String, String), ActorFrameFact> = BTreeMap::new();

    for part in &program_frame.parts {
        let mut draw_counts: BTreeMap<&str, (u32, u32)> = BTreeMap::new();
        for item in &part.program.items {
            let entry = draw_counts.entry(item.label.as_str()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += item.commands.len() as u32;
        }
        for item in &part.program.items {
            let bounds = part
                .program
                .precise_bounds
                .get(&item.label)
                .map(|rect| Box2::new(rect.x0, rect.y0, rect.x1, rect.y1));
            let (text_max_width, text_overflow) =
                text_measurements(part.timeline, &item.label, time_ms);
            let measured_text_width = bounds.map(|b| (b.x1 - b.x0).max(0.0) as f32);
            let raw_on_frame_fraction = bounds.map_or(0.0, |b| on_frame_fraction(b, dimensions));
            let padded_on_frame_fraction =
                bounds.map_or(0.0, |b| on_frame_fraction(b.padded(BOUNDS_PAD), dimensions));
            let (draw_items, draw_commands) = draw_counts
                .get(item.label.as_str())
                .copied()
                .map_or((None, None), |(items, commands)| (Some(items), Some(commands)));
            by_actor.insert(
                (part.scene.clone(), item.label.clone()),
                ActorFrameFact {
                    scene: part.scene.clone(),
                    label: item.label.clone(),
                    opacity: Some(item.opacity),
                    bounds,
                    on_frame_fraction: raw_on_frame_fraction,
                    padded_on_frame_fraction,
                    draw_items,
                    draw_commands,
                    text_max_width,
                    text_overflow,
                    measured_text_width,
                },
            );
        }

        for actor in actors.iter().filter(|actor| actor.scene == part.scene) {
            let key = (part.scene.clone(), actor.label.clone());
            if by_actor.contains_key(&key) {
                continue;
            }
            let Some(id) = part.timeline.world().get_id(&actor.label) else {
                continue;
            };
            let opacity = track_product(part.timeline, id, time_ms);
            let (text_max_width, text_overflow) =
                text_measurements(part.timeline, &actor.label, time_ms);
            by_actor.insert(
                key,
                ActorFrameFact {
                    scene: part.scene.clone(),
                    label: actor.label.clone(),
                    opacity: Some(opacity),
                    bounds: None,
                    on_frame_fraction: 0.0,
                    padded_on_frame_fraction: 0.0,
                    draw_items: None,
                    draw_commands: None,
                    text_max_width,
                    text_overflow,
                    measured_text_width: None,
                },
            );
        }
    }

    let actor_frames: Vec<ActorFrameFact> = by_actor.into_values().collect();
    let cast = actor_frames
        .iter()
        .filter(|frame_actor| {
            let visual = actors.iter().any(|actor| {
                actor.scene == frame_actor.scene && actor.label == frame_actor.label && actor.visual
            });
            visual
                && frame_actor.opacity.unwrap_or(0.0) > 0.01
                && frame_actor.bounds.is_some()
                && frame_actor.padded_on_frame_fraction > 0.0
        })
        .count();
    let state_digest = frame_state_digest(&actor_frames);
    Ok(FrameFact {
        time_s,
        scene: program_frame.scene,
        cast,
        ink_fraction: None,
        state_digest: Some(state_digest),
        actors: actor_frames,
    })
}

/// Stable within a run; used only to compare consecutive structural samples.
fn frame_state_digest(actors: &[ActorFrameFact]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut write = |value: u64| {
        hash ^= value;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    };
    for actor in actors {
        for byte in actor.scene.as_bytes().iter().chain(actor.label.as_bytes()) {
            write(u64::from(*byte));
        }
        write((actor.opacity.unwrap_or(0.0) * 1000.0) as u64);
        for value in actor
            .bounds
            .map_or([0.0; 4], |bounds| [bounds.x0, bounds.y0, bounds.x1, bounds.y1])
        {
            write((value * 1000.0) as i64 as u64);
        }
        write(u64::from(actor.draw_items.unwrap_or(0)));
        write(u64::from(actor.draw_commands.unwrap_or(0)));
    }
    hash
}

fn text_measurements(
    timeline: &animatix::timeline::Timeline,
    label: &str,
    time_ms: u64,
) -> (Option<f32>, Option<String>) {
    let Some(id) = timeline.world().get_id(label) else {
        return (None, None);
    };
    let Some(text) = timeline.world().text(id) else {
        return (None, None);
    };
    let max_width = text.text_max_width.as_ref().map(|track| track.evaluate(time_ms));
    let overflow = text.overflow.as_ref().map(|track| track.evaluate(time_ms));
    (max_width, overflow)
}

fn track_product(
    timeline: &animatix::timeline::Timeline,
    id: animatix::timeline::EntityId,
    time_ms: u64,
) -> f32 {
    let world = timeline.world();
    let mut opacity = 1.0f32;
    let mut cursor = Some(id);
    while let Some(current) = cursor {
        opacity *= world.style(current).opacity.get(time_ms, 1.0);
        cursor = world.hierarchy(current).parent;
    }
    opacity
}

/// Fraction of `bounds` that lies inside the canvas (0 when bounds has no
/// area; 1 when fully on frame).
pub(crate) fn on_frame_fraction(bounds: Box2, dimensions: SceneDimensions) -> f64 {
    let width = (bounds.x1 - bounds.x0).max(0.0);
    let height = (bounds.y1 - bounds.y0).max(0.0);
    let area = width * height;
    if area <= f64::EPSILON {
        return 0.0;
    }
    let x_overlap = (bounds.x1.min(dimensions.width as f64) - bounds.x0.max(0.0)).max(0.0);
    let y_overlap = (bounds.y1.min(dimensions.height as f64) - bounds.y0.max(0.0)).max(0.0);
    (x_overlap * y_overlap / area).clamp(0.0, 1.0)
}

fn collect_redundant_keyframes(target: &BuildTarget) -> Vec<RedundantKeyframeFact> {
    let mut out = Vec::new();
    match target {
        BuildTarget::SingleScene(timeline) => {
            collect_redundant_from_timeline("main", timeline, &mut out);
        },
        BuildTarget::MultiScene(composition) => {
            for (name, scene) in &composition.scenes {
                collect_redundant_from_timeline(name, &scene.timeline, &mut out);
            }
        },
    }
    out
}

fn collect_redundant_from_timeline(
    scene: &str,
    timeline: &animatix::timeline::Timeline,
    out: &mut Vec<RedundantKeyframeFact>,
) {
    let world = timeline.world();
    for label in timeline.actor_labels() {
        let Some(id) = world.get_id(label) else {
            continue;
        };
        for &property in AUDITED_KEYFRAME_PROPERTIES {
            let times = world.list_keyframes(id, property);
            if times.len() < 2 {
                continue;
            }
            let Some(schema) = PROPERTY_REGISTRY.iter().find(|schema| schema.name == property)
            else {
                continue;
            };
            for pair in times.windows(2) {
                let (t0, t1) = (pair[0], pair[1]);
                if t1 <= t0 {
                    continue;
                }
                let mid = t0 + (t1 - t0) / 2;
                let start = world.read_property_value_or_default(id, schema, t0);
                let middle = world.read_property_value_or_default(id, schema, mid);
                let end = world.read_property_value_or_default(id, schema, t1);
                if start == middle && middle == end && !matches!(start, PropertyValue::PointList(_))
                {
                    out.push(RedundantKeyframeFact {
                        scene: scene.to_string(),
                        label: label.clone(),
                        property: property.to_string(),
                        previous_time_s: t0 as f64 / 1000.0,
                        time_s: t1 as f64 / 1000.0,
                    });
                }
            }
        }
    }
}
