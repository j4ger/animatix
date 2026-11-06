//! Pivot-anchored scale presets: `grow-in` and `shrink-out`.
//!
//! The plain `scale` verb multiplies whatever scale the actor already samples
//! and scales around the actor's centre. These two presets answer a different
//! authoring question: *where does the object grow from?* `grow-in` ramps the
//! scale from zero to the actor's authored scale, so the actor is built out of
//! a point; `shrink-out` ramps it back to zero without fading, so an exit reads
//! as the object collapsing rather than evaporating.
//!
//! A non-centre pivot moves the actor while it scales: the pivot must stay at
//! its authored place, so the centre travels by `(1 - s(t)) * P` where `P` is
//! the half-extent vector from the centre to the chosen edge. Scale and
//! `motion_offset` are sampled on the same knots and interpolated linearly, so
//! the pivot stays exact between samples rather than drifting off the easing
//! curve. A centre pivot needs no offset and uses the plain two-keyframe ramp.

use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::{Action, Expr};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::{Easing, apply_easing};
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{DEFAULT_LAYOUT_HALF_SIZE, Timeline};

/// Number of sampled knots used by a non-centre pivot ramp. The specification
/// asks for 12–16 samples; sixteen keeps the linearly interpolated offset well
/// inside a tenth of a pixel of the sampled easing curve.
const PIVOT_SAMPLES: usize = 16;

/// Where a scale ramp is anchored, in the actor's local space.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ScaleOrigin {
    /// Scale around the actor's centre (the default).
    Center,
    /// Anchor the left edge: `P = (-half_width, 0)`.
    Left,
    /// Anchor the right edge: `P = (half_width, 0)`.
    Right,
    /// Anchor the top edge: `P = (0, -half_height)`.
    Top,
    /// Anchor the bottom edge: `P = (0, half_height)`.
    Bottom,
    /// An explicit local pivot offset from the actor's centre.
    Point([f32; 2]),
}

impl ScaleOrigin {
    /// The pivot's offset from the actor's centre, given the actor's half-size.
    fn pivot(self, half: [f32; 2]) -> [f32; 2] {
        match self {
            Self::Center => [0.0, 0.0],
            Self::Left => [-half[0], 0.0],
            Self::Right => [half[0], 0.0],
            Self::Top => [0.0, -half[1]],
            Self::Bottom => [0.0, half[1]],
            Self::Point(point) => point,
        }
    }

    /// Whether this origin is the default centre.
    fn is_center(self) -> bool {
        matches!(self, Self::Center)
    }
}

/// The direction of the scale ramp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    /// `grow-in`: zero → authored scale.
    Grow,
    /// `shrink-out`: authored scale → zero.
    Shrink,
}

fn push_origin_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    code: DiagnosticCode,
    message: impl Into<String>,
    subject: &str,
) {
    diagnostics.push(
        Diagnostic::warning(code, DiagnosticPhase::Build, message.into()).with_subject(subject),
    );
}

/// Parse the pivot name or coordinate tuple from the action's origin modifier.
///
/// Unknown values warn and fall back to the centre; this mirrors the other
/// modifier parsers, which keep the action useful instead of silently ignoring
/// an authoring mistake.
fn parse_origin(
    modifiers: &[crate::ast::Modifier],
    key: &str,
    action: &Action,
    timeline: &Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) -> ScaleOrigin {
    let mut origin = None;
    let mut saw_key = false;

    for modifier in modifiers {
        if modifier.name.as_deref() != Some(key) {
            continue;
        }
        if saw_key {
            push_origin_diagnostic(
                diagnostics,
                DiagnosticCode::ConflictingModifierKey,
                format!("Conflicting '{key}' modifiers on action; using the last value provided."),
                key,
            );
        }

        match &modifier.value {
            Expr::Ident(name) | Expr::Str(name) => match name.as_str() {
                "center" | "centre" => {
                    origin = Some(ScaleOrigin::Center);
                    saw_key = true;
                },
                "left" => {
                    origin = Some(ScaleOrigin::Left);
                    saw_key = true;
                },
                "right" => {
                    origin = Some(ScaleOrigin::Right);
                    saw_key = true;
                },
                "top" => {
                    origin = Some(ScaleOrigin::Top);
                    saw_key = true;
                },
                "bottom" => {
                    origin = Some(ScaleOrigin::Bottom);
                    saw_key = true;
                },
                other => {
                    push_origin_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported '{key}' origin '{other}' on {}; expected center, left, \
                             right, top, bottom, or an (x, y) point.",
                            action.verb
                        ),
                        key,
                    );
                },
            },
            Expr::Tuple(items) if items.len() == 2 => {
                match crate::timeline::evaluate_expr(&modifier.value, &timeline.env) {
                    Ok(crate::timeline::Value::Vec2([x, y])) => {
                        origin = Some(ScaleOrigin::Point([x as f32, y as f32]));
                        saw_key = true;
                    },
                    _ => {
                        push_origin_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "'{key}' origin on {} expects an (x, y) pair of numbers.",
                                action.verb
                            ),
                            key,
                        );
                    },
                }
            },
            other => {
                push_origin_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    format!(
                        "Unsupported '{key}' origin value {other:?} on {}; expected center, left, \
                         right, top, bottom, or an (x, y) point.",
                        action.verb
                    ),
                    key,
                );
            },
        }
    }

    origin.unwrap_or(ScaleOrigin::Center)
}

/// Resolve the origin against the actor's half-size, warning when a non-centre
/// pivot cannot be anchored because the actor measures zero in that direction.
fn resolve_pivot(
    origin: ScaleOrigin,
    half: [f32; 2],
    target: &str,
    action: &Action,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<[f32; 2]> {
    if origin.is_center() {
        return None;
    }
    let pivot = origin.pivot(half);
    if pivot == [0.0, 0.0] {
        push_origin_diagnostic(
            diagnostics,
            DiagnosticCode::InvalidModifierValue,
            format!(
                "'{}' targets '{}' with a non-centre origin but a zero-size actor; the scale \
                 anchor falls back to the actor's centre.",
                action.verb, target
            ),
            target,
        );
        return None;
    }
    Some(pivot)
}

/// Write the sampled scale + offset pair that keeps a non-centre pivot fixed.
fn write_pivot_ramp(
    geometry: &mut crate::timeline::animation_track::GeometryTracks,
    pivot: [f32; 2],
    start_offset: [f32; 2],
    start_scale: f32,
    end_scale: f32,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) {
    if let Some(track) = geometry.motion_offset.as_mut() {
        track.split_off_after(t_start_ms);
    }

    if t_end_ms == t_start_ms {
        let offset = [
            start_offset[0] + (1.0 - end_scale) * pivot[0],
            start_offset[1] + (1.0 - end_scale) * pivot[1],
        ];
        geometry.scale.ensure(1.0).add_keyframe(t_end_ms, end_scale, easing);
        geometry
            .motion_offset
            .ensure([0.0, 0.0])
            .add_keyframe(t_end_ms, offset, Easing::Linear);
        return;
    }

    let span = (t_end_ms - t_start_ms) as f64;
    // Sixteen knots for a normal ramp; a sub-16 ms ramp cannot fit sixteen
    // distinct millisecond keyframes, so use one per millisecond.
    let knots = PIVOT_SAMPLES.min((t_end_ms - t_start_ms) as usize + 1).max(2);

    for i in 0..knots {
        let progress = i as f32 / (knots - 1) as f32;
        let at = t_start_ms + (span * i as f64 / (knots - 1) as f64) as u64;
        let eased = apply_easing(progress, easing);
        let scale = start_scale + (end_scale - start_scale) * eased;
        let offset = [
            start_offset[0] + (1.0 - scale) * pivot[0],
            start_offset[1] + (1.0 - scale) * pivot[1],
        ];
        geometry.scale.ensure(1.0).add_keyframe(at, scale, Easing::Linear);
        geometry
            .motion_offset
            .ensure([0.0, 0.0])
            .add_keyframe(at, offset, Easing::Linear);
    }
}

/// The scale the actor rests at, ignoring an earlier `shrink-out`.
///
/// `grow-in` must be reversible: after `shrink-out` has driven the scale to
/// zero, the next `grow-in` has to return to the scale the actor was authored
/// at, not to the zero it currently samples. Scan the preceding keyframes for
/// the most recent positive value; a non-positive ramp end is not a resting
/// scale.
fn authored_scale(
    geometry: &crate::timeline::animation_track::GeometryTracks,
    time_ms: u64,
) -> f32 {
    if let Some(track) = geometry.scale.as_ref() {
        let mut authored = None;
        for keyframe in track.keyframes().as_slice() {
            if keyframe.time_ms > time_ms {
                break;
            }
            if keyframe.payload.0 > 0.0 {
                authored = Some(keyframe.payload.0);
            }
        }
        if let Some(scale) = authored {
            return scale;
        }
    }
    geometry.scale.get(time_ms, 1.0)
}

/// The actor's half-extents at `time_ms`.
///
/// `size` is the rendered half-size for ordinary shapes; layout-managed text
/// can carry a measured half-size only in `layout_size`, so fall back per axis
/// rather than treating a measured actor as zero-size.
fn half_extents(
    geometry: &crate::timeline::animation_track::GeometryTracks,
    time_ms: u64,
) -> [f32; 2] {
    let size = geometry.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);
    if size[0] > 0.0 && size[1] > 0.0 {
        return size;
    }
    let layout = geometry.layout_size_get(time_ms).unwrap_or([0.0, 0.0]);
    [
        if size[0] > 0.0 { size[0] } else { layout[0] },
        if size[1] > 0.0 { size[1] } else { layout[1] },
    ]
}

/// Run one pivot-anchored scale preset over the action's targets.
fn run_scale_preset(
    direction: Direction,
    origin_key: &str,
    action: &Action,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let timing = super::ActionTiming::parse(action, time_ms, diagnostics);
    let duration_ms = timing.duration_ms;
    let easing = timing.easing;
    let origin = parse_origin(&action.modifiers, origin_key, action, timeline, diagnostics);

    let t_start_ms = timing.t_start_ms;
    let t_end_ms = timing.t_end_ms;

    for target in &action.targets {
        if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
            continue;
        }

        // Grow-in is an entrance: it may lift the build-time hidden seed, but
        // it must leave an actor that is already visible at its authored
        // opacity alone (a pure scale entrance never fades a visible actor).
        if direction == Direction::Grow {
            super::lift_hidden_by_default_subtree(timeline, target, t_start_ms, t_end_ms, easing);
        }

        let Some(id) = timeline.world.get_id(target) else {
            continue;
        };
        let geometry = timeline.world.geometry_mut(id);
        let authored = authored_scale(geometry, t_start_ms);
        let half = half_extents(geometry, t_start_ms);
        let pivot = resolve_pivot(origin, half, target, action, diagnostics);
        let (start_scale, end_scale) = match direction {
            Direction::Grow => (0.0, authored),
            Direction::Shrink => (authored, 0.0),
        };
        if authored <= 0.0 {
            tracing::warn!(
                "{}: {} was authored with scale {authored}; the scale ramp has a non-positive end",
                target,
                action.verb
            );
        }

        match pivot {
            Some(pivot) => {
                let start_offset = geometry.motion_offset.get(t_start_ms, [0.0, 0.0]);
                write_pivot_ramp(
                    geometry,
                    pivot,
                    start_offset,
                    start_scale,
                    end_scale,
                    t_start_ms,
                    t_end_ms,
                    easing,
                );
            },
            None => {
                if duration_ms > 0.0 {
                    geometry.scale.ensure(1.0).add_keyframe(
                        t_start_ms,
                        start_scale,
                        Easing::Linear,
                    );
                }
                geometry.scale.ensure(1.0).add_keyframe(t_end_ms, end_scale, easing);
            },
        }
    }
}

fn signature(
    name: &str,
    category: &str,
    description: &str,
    origin_key: &str,
    origin_description: &str,
) -> ActionSignature {
    let mut modifiers = vec![ActionParam {
        name: origin_key.to_string(),
        description: origin_description.to_string(),
        type_info: "center | left | right | top | bottom | (x, y)".to_string(),
    }];
    modifiers.extend(base_timing_params());
    ActionSignature {
        name: name.to_string(),
        category: category.to_string(),
        description: description.to_string(),
        params: vec![],
        modifiers,
    }
}

/// Grows the target from zero scale to its authored scale.
pub struct GrowIn;

impl BuiltinAction for GrowIn {
    fn signature(&self) -> ActionSignature {
        signature(
            "grow-in",
            "Entrance",
            "Grows the target from zero to its authored scale, optionally anchored to an edge, \
             and lifts a hidden-by-default actor without fading an already-visible one.",
            "from",
            "Scale anchor: 'center' (default), 'left', 'right', 'top', 'bottom', or an (x, y) \
             local pivot offset.",
        )
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        run_scale_preset(Direction::Grow, "from", action, time_ms, timeline, diagnostics);
    }
}

/// Shrinks the target from its authored scale to zero without fading it.
pub struct ShrinkOut;

impl BuiltinAction for ShrinkOut {
    fn signature(&self) -> ActionSignature {
        signature(
            "shrink-out",
            "Exit",
            "Shrinks the target from its authored scale to zero without changing opacity, \
             optionally anchored to an edge; a later grow-in restores it.",
            "to",
            "Scale anchor: 'center' (default), 'left', 'right', 'top', 'bottom', or an (x, y) \
             local pivot offset.",
        )
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        run_scale_preset(Direction::Shrink, "to", action, time_ms, timeline, diagnostics);
    }
}
