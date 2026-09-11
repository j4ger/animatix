//! Effect chain storage owned by a compositing scope.
//!
//! Effects are not primitives or actors: a `Filter` scope lowers its effect
//! children into an [`EffectChainTrack`] on its own [`AnimationTrack`]. The
//! chain holds ordered [`EffectStage`]s whose parameters animate through
//! [`DynTrack`], and is sampled into the renderer-facing
//! [`EffectChain`](crate::timeline::filter::EffectChain) at frame time.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::timeline::filter::{
    EffectChain, EffectDescriptor, EffectId, EffectInstance, EffectParamKind, EffectParamValue,
    EffectParams, descriptor,
};
use crate::timeline::plan::{DynTrack, PropertyKind};
use crate::timeline::property_engine::PropertyValue;

/// An ordered effect chain owned by a compositing scope's track.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EffectChainTrack {
    /// Effect stages in application order.
    pub stages: Vec<EffectStage>,
}

/// One effect instance inside a scope's chain.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EffectStage {
    /// Author-visible label within the scope (`bg.soft.radius`).
    pub label: String,
    /// Which effect to run.
    pub kind: EffectId,
    /// Parameter tracks keyed by the descriptor's parameter name.
    pub params: BTreeMap<String, DynTrack>,
    /// Implicit `enabled` flag; an absent track means `true`.
    pub enabled: DynTrack,
}

impl EffectStage {
    /// Create an empty stage: parameters at identity, enabled.
    pub fn new(label: String, kind: EffectId) -> Self {
        Self {
            label,
            kind,
            params: BTreeMap::new(),
            enabled: DynTrack::empty(PropertyKind::Bool),
        }
    }

    /// Mutable parameter track for `name`, created lazily with `kind`.
    pub fn param_track_mut(&mut self, name: &str, kind: EffectParamKind) -> &mut DynTrack {
        self.params
            .entry(name.to_string())
            .or_insert_with(|| DynTrack::empty(effect_property_kind(kind)))
    }
}

impl EffectChainTrack {
    /// `true` when no stage is declared.
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Look up a stage by its authored label.
    pub fn stage(&self, label: &str) -> Option<&EffectStage> {
        self.stages.iter().find(|stage| stage.label == label)
    }

    /// Mutable lookup by authored label.
    pub fn stage_mut(&mut self, label: &str) -> Option<&mut EffectStage> {
        self.stages.iter_mut().find(|stage| stage.label == label)
    }

    /// Sample the chain at `time_ms`, dropping disabled and identity stages.
    pub fn build_chain(&self, time_ms: u64) -> EffectChain {
        let mut instances = Vec::new();
        for stage in &self.stages {
            let desc = descriptor(stage.kind);
            let enabled = match stage.enabled.sample(time_ms) {
                Some(PropertyValue::Bool(enabled)) => enabled,
                _ => true,
            };
            if !enabled {
                continue;
            }
            let params = sample_params(desc, stage, time_ms);
            if desc.is_identity(&params) {
                continue;
            }
            instances.push(EffectInstance {
                id: stage.kind,
                enabled: true,
                params,
            });
        }
        EffectChain {
            instances,
            time_ms: time_ms as f32,
        }
    }
}

/// Sample every declared parameter, falling back to its identity value.
pub fn sample_params(desc: &EffectDescriptor, stage: &EffectStage, time_ms: u64) -> EffectParams {
    EffectParams {
        values: desc
            .params
            .iter()
            .map(|spec| {
                stage
                    .params
                    .get(spec.name)
                    .and_then(|track| track.sample(time_ms))
                    .and_then(|value| effect_param_value(value, spec.kind))
                    .unwrap_or(spec.identity)
            })
            .collect(),
    }
}

/// Map an effect parameter's declared kind to its dynamic-track kind.
pub fn effect_property_kind(kind: EffectParamKind) -> PropertyKind {
    match kind {
        EffectParamKind::F32 => PropertyKind::F32,
        EffectParamKind::U32 => PropertyKind::U32,
        EffectParamKind::Vec2 => PropertyKind::Vec2,
        EffectParamKind::Vec4 => PropertyKind::Vec4,
        EffectParamKind::Bool => PropertyKind::Bool,
    }
}

/// Convert an evaluated `.amx` value into a parameter's declared storage kind.
///
/// Returns `None` (with a warning) on a kind mismatch; callers report or skip.
pub fn value_to_property(
    kind: EffectParamKind,
    value: crate::timeline::Value,
    subject: &str,
) -> Option<PropertyValue> {
    use crate::timeline::Value;
    let converted = match (kind, value) {
        (EffectParamKind::F32, Value::Num(n)) => PropertyValue::F32(n as f32),
        (EffectParamKind::U32, Value::Num(n)) if n >= 0.0 => PropertyValue::U32(n as u32),
        (EffectParamKind::Bool, Value::Bool(b)) => PropertyValue::Bool(b),
        (EffectParamKind::Vec2, Value::Vec2(v)) => PropertyValue::Vec2([v[0] as f32, v[1] as f32]),
        (EffectParamKind::Vec4, Value::Vec4(v)) | (EffectParamKind::Vec4, Value::Color(v)) => {
            PropertyValue::Vec4([v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32])
        },
        (kind, other) => {
            tracing::warn!(
                "{subject}: effect parameter expects {kind:?}, got {:?}; ignoring",
                other
            );
            return None;
        },
    };
    Some(converted)
}

/// Declared identity value as a `PropertyValue`, for keyframe starts.
pub fn identity_to_property(identity: EffectParamValue) -> PropertyValue {
    match identity {
        EffectParamValue::F32(v) => PropertyValue::F32(v),
        EffectParamValue::U32(v) => PropertyValue::U32(v),
        EffectParamValue::Vec2(v) => PropertyValue::Vec2(v),
        EffectParamValue::Vec4(v) => PropertyValue::Vec4(v),
        EffectParamValue::Bool(v) => PropertyValue::Bool(v),
    }
}

/// Coerce a stored `PropertyValue` into the declared parameter value.
///
/// Returns `None` (with a warning) when the stored value kind does not match
/// the declaration, so the caller can fall back to the declared identity.
fn effect_param_value(value: PropertyValue, kind: EffectParamKind) -> Option<EffectParamValue> {
    let coerced = match (kind, value) {
        (EffectParamKind::F32, PropertyValue::F32(v)) => EffectParamValue::F32(v),
        (EffectParamKind::F32, PropertyValue::U32(v)) => EffectParamValue::F32(v as f32),
        (EffectParamKind::U32, PropertyValue::U32(v)) => EffectParamValue::U32(v),
        (EffectParamKind::Bool, PropertyValue::Bool(v)) => EffectParamValue::Bool(v),
        (EffectParamKind::Vec2, PropertyValue::Vec2(v)) => EffectParamValue::Vec2(v),
        (EffectParamKind::Vec4, PropertyValue::Vec4(v))
        | (EffectParamKind::Vec4, PropertyValue::Color(v)) => EffectParamValue::Vec4(v),
        (kind, other) => {
            tracing::warn!(
                "effect parameter stored as {other:?} does not match declared {kind:?}; using identity"
            );
            return None;
        },
    };
    Some(coerced)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_matches(
        runtime: EffectParamKind,
        shared: animatix_syntax::schema::PropertyValueKind,
    ) -> bool {
        use animatix_syntax::schema::PropertyValueKind as S;
        matches!(
            (runtime, shared),
            (EffectParamKind::F32, S::F32)
                | (EffectParamKind::U32, S::U32)
                | (EffectParamKind::Bool, S::Bool)
                | (EffectParamKind::Vec2, S::Vec2)
                | (EffectParamKind::Vec4, S::Vec4)
        )
    }

    /// The runtime effect descriptors and the analyzer's shared effect table
    /// must declare the same parameter names and kinds.
    #[test]
    fn effect_descriptors_match_shared_effect_specs() {
        for id in [EffectId::Blur, EffectId::ColorGrade] {
            let desc = descriptor(id);
            let shared = animatix_syntax::schema::effect_spec(desc.type_name)
                .unwrap_or_else(|| panic!("missing shared effect spec for {}", desc.type_name));
            assert_eq!(
                desc.params.len(),
                shared.params.len(),
                "parameter count mismatch for {}",
                desc.type_name
            );
            for (runtime, declared) in desc.params.iter().zip(shared.params) {
                assert_eq!(runtime.name, declared.name, "{} parameter order", desc.type_name);
                assert!(
                    kind_matches(runtime.kind, declared.kind),
                    "{}.{} kind mismatch: {:?} vs {:?}",
                    desc.type_name,
                    runtime.name,
                    runtime.kind,
                    declared.kind
                );
            }
        }
    }

    /// Identity and disabled stages are dropped when sampling the chain.
    #[test]
    fn build_chain_skips_identity_and_disabled_stages() {
        let mut track = EffectChainTrack::default();

        let mut identity = EffectStage::new("identity".into(), EffectId::Blur);
        identity
            .param_track_mut("radius", EffectParamKind::F32)
            .add_keyframe(0, PropertyValue::F32(0.0));
        track.stages.push(identity);

        let mut active = EffectStage::new("active".into(), EffectId::Blur);
        active
            .param_track_mut("radius", EffectParamKind::F32)
            .add_keyframe(0, PropertyValue::F32(8.0));
        track.stages.push(active);

        let mut disabled = EffectStage::new("disabled".into(), EffectId::Blur);
        disabled
            .param_track_mut("radius", EffectParamKind::F32)
            .add_keyframe(0, PropertyValue::F32(8.0));
        disabled.enabled.add_keyframe(0, PropertyValue::Bool(false));
        track.stages.push(disabled);

        let chain = track.build_chain(0);
        assert_eq!(chain.instances.len(), 1);
        assert_eq!(chain.instances[0].params.f32_at(0), 8.0);
    }
}
