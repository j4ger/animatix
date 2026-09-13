//! Effect-chain storage and the compositing backend boundary.
//!
//! The effect *definitions* (the `Effect` trait, parameter schema, built-in
//! implementations, and the plugin registry) live in `animatix-core` /
//! `animatix-std`; this module re-exports them at the historical paths and
//! holds the engine-side halves: the animatable chain owned by a compositing
//! scope (`EffectChainTrack`) and the renderer-facing types
//! (`EffectChain`, `FilterBackend`). The normative contract lives in
//! `docs/effects.md`; the authoring checklist lives in `animatix-std`.

mod chain;
mod track;

pub use animatix_core::effect::{
    Effect, EffectId, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams,
    EffectPassSpec, identity_for, kind_layout, uniform_size_for,
};
pub use animatix_std::effects::compose_color_matrix;
pub use animatix_std::{
    BLUR, CHROMATIC_ABERRATION, COLOR_GRADE, EFFECTS, PluginEffectData, PluginEffectInfo, effect,
    effect_for_type, pack_generic, plugin_effects, register_extension_effect,
    unregister_extension_effect,
};
pub use chain::{EffectChain, EffectInstance, EffectRegion, FilterBackend, PendingComposite};
pub use track::{
    EffectChainTrack, EffectStage, effect_property_kind, identity_to_property, sample_params,
    value_to_property,
};
