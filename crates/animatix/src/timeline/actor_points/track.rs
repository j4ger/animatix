//! Keyframed coordinates and actor points. Actor endpoints are rebound in the receiving scene.

use crate::ast::Expr;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::actor_points::ActorPoint;
use crate::timeline::{
    Easing, EntityId, Environment, Interpolate, PropertyTrack, SceneAnchor, SceneDimensions,
};
use kurbo::Point;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) enum PointValue {
    Literal([f32; 2]),
    Scene(SceneAnchor),
    Actor {
        label: String,
        point: ActorPoint,
        #[cfg_attr(feature = "serde", serde(skip))]
        id: Option<EntityId>,
    },
    Mix(Box<(Self, Self, f32)>),
}

pub(crate) const DEFAULT_CENTER: PointValue = PointValue::Literal([0.5, 0.5]);

impl PointValue {
    pub(crate) fn reference(expr: &Expr, env: &Environment) -> Option<Self> {
        if let Some(anchor) = crate::timeline::position::parse_scene_anchor(expr) {
            return Some(Self::Scene(anchor));
        }
        let (label, point) = crate::timeline::lookup::actor_point_reference(expr, env)?;
        Some(Self::Actor {
            label,
            point,
            id: None,
        })
    }

    pub(crate) fn parse(
        expr: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
    ) -> Option<Self> {
        use crate::timeline::lookup::{evaluate_expr_with_lookup_diagnostic, list_as_vec2};
        if let Some(reference) = Self::reference(expr, env) {
            return Some(reference);
        }
        let value = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
        let Some(uv) = list_as_vec2(&value) else {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    "a point expects a coordinate pair or an actor point",
                )
                .with_subject(subject),
            );
            return None;
        };
        Some(Self::Literal(uv))
    }

    fn bind(&mut self, resolve: &mut impl FnMut(&str, ActorPoint) -> Option<EntityId>) {
        match self {
            Self::Actor { label, point, id } => *id = resolve(label, *point),
            Self::Mix(mix) => {
                mix.0.bind(resolve);
                mix.1.bind(resolve);
            },
            Self::Literal(_) | Self::Scene(_) => {},
        }
    }

    fn sample(
        &self,
        dimensions: Option<SceneDimensions>,
        fallback: [f32; 2],
        resolve: &impl Fn(EntityId, ActorPoint) -> Option<Point>,
    ) -> PointSample {
        match self {
            Self::Literal(uv) => PointSample {
                uv: uv.map(f64::from),
                ..Default::default()
            },
            Self::Scene(anchor) => dimensions.map_or_else(
                || Self::Literal(fallback).sample(dimensions, fallback, resolve),
                |dims| PointSample::from_scene(crate::timeline::scene_anchor_point(*anchor, dims)),
            ),
            Self::Actor { id, point, .. } => {
                if let Some(position) = id.and_then(|id| resolve(id, *point)) {
                    PointSample::from_scene(position)
                } else {
                    Self::Literal(fallback).sample(dimensions, fallback, resolve)
                }
            },
            Self::Mix(mix) => mix
                .0
                .sample(dimensions, fallback, resolve)
                .interpolate(mix.1.sample(dimensions, fallback, resolve), mix.2),
        }
    }
}

impl Interpolate for PointValue {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        if t == 0.0 {
            return self.clone();
        }
        if t == 1.0 {
            return other.clone();
        }
        match (self, other) {
            (Self::Literal(from), Self::Literal(to)) => Self::Literal(from.interpolate(to, t)),
            _ => Self::Mix(Box::new((self.clone(), other.clone(), t))),
        }
    }
}

pub(crate) fn write_point(
    track: &mut PropertyTrack<PointValue>,
    value: PointValue,
    start: u64,
    end: u64,
    easing: Easing,
) {
    if start != end {
        track.add_keyframe(start, track.evaluate(start), Easing::Linear);
    }
    track.add_keyframe(end, value, easing);
}

pub(crate) fn bind_track(
    track: &mut PropertyTrack<PointValue>,
    resolve: &mut impl FnMut(&str, ActorPoint) -> Option<EntityId>,
) {
    track.default_value.bind(resolve);
    for key in track.keyframes.as_mut_slice() {
        key.payload.0.bind(resolve);
    }
}

pub(crate) fn sample_point(
    track: &PropertyTrack<PointValue>,
    time: u64,
    dimensions: Option<SceneDimensions>,
    fallback: [f32; 2],
    resolve: &impl Fn(EntityId, ActorPoint) -> Option<Point>,
) -> PointSample {
    let sample = |value: &PointValue| value.sample(dimensions, fallback, resolve);
    if let Some((end, from, to, t, easing)) = track.interpolation_segment(time) {
        if time == end || from == to {
            return sample(to);
        }
        return sample(from).interpolate(sample(to), crate::easing::apply_easing(t, *easing));
    }
    let keys = track.keyframes.as_slice();
    match keys.first() {
        Some(first) if time <= first.time_ms => sample(&first.payload.0),
        Some(_) => sample(&keys.last().expect("nonempty point track").payload.0),
        None => sample(&track.default_value),
    }
}

/// Weighted contributions in texture UV and projected scene coordinates.
/// The renderer adds the projected part after allocating the actual crop.
#[derive(Clone, Copy, Debug, Default)]
pub struct PointSample {
    pub(crate) uv: [f64; 2],
    position: [f64; 2],
    pub(crate) weight: f64,
}

impl PointSample {
    fn from_scene(point: Point) -> Self {
        Self {
            uv: [0.0; 2],
            position: [point.x, point.y],
            weight: 1.0,
        }
    }

    pub(crate) fn transform_scene(&mut self, transform: kurbo::Affine) {
        let [a, b, c, d, e, f] = transform.as_coeffs();
        let [x, y] = self.position;
        self.position = [
            a * x + c * y + e * self.weight,
            b * x + d * y + f * self.weight,
        ];
    }
    fn interpolate(self, other: Self, t: f32) -> Self {
        let t = f64::from(t.clamp(0.0, 1.0));
        let lerp = |a: f64, b: f64| a + (b - a) * t;
        Self {
            uv: std::array::from_fn(|i| lerp(self.uv[i], other.uv[i])),
            position: std::array::from_fn(|i| lerp(self.position[i], other.position[i])),
            weight: lerp(self.weight, other.weight),
        }
    }

    /// Normalize the scene contribution against the renderer's scratch target.
    pub fn texture_uv(mut self, shifted: kurbo::Affine, size: [u32; 2]) -> [f32; 2] {
        self.transform_scene(shifted);
        [
            (self.uv[0] + self.position[0] / f64::from(size[0])) as f32,
            (self.uv[1] + self.position[1] / f64::from(size[1])) as f32,
        ]
    }
}
