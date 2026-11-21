//! Built-in effect definitions, callable descriptors, and the primitive
//! catalog.
//!
//! This module is the single author-visible source for what the language's
//! built-ins are. It sits alongside shared contracts in [`crate`]: the engine
//! uses it for behavior, and the parser derives author-visible contract tables
//! from it. Adding a built-in requires no parallel declaration elsewhere.
//!
//! Effects are complete implementations (parameters, WGSL passes, `pack`,
//! `support`); primitives are represented by their identity cards
//! ([`PrimitiveInfo`] / [`CATALOG`]) while their behaviour lives in the
//! engine, because building an actor needs the engine's `Timeline`. The
//! engine's `BUILT_INS` rows pair each behaviour with its card by symbol, so
//! metadata and behaviour cannot drift positionally.
//!
//! The normative contract for effect pass layout, uniforms, identity
//! semantics, and failure policy lives in `docs/effects.md`.

pub mod callable;
pub mod catalog;
pub mod effects;
pub mod members;

pub use catalog::{CATALOG, PrimitiveInfo, caps_for_type, caps_from_info, catalog_lookup};
pub use effects::{
    BLOOM, BLUR, CHROMATIC_ABERRATION, COLOR_GRADE, DROP_SHADOW, DUOTONE, EDGE, GRAIN,
    LENS_DISTORTION, LEVELS, MOTION_BLUR, POSTERIZE, SHARPEN, VIGNETTE,
};

use crate::effect::Effect;

// ── Built-in catalog ────────────────────────────────────────────────────────

/// Bootstrap list of all built-in effects, in registration order.
pub static EFFECTS: &[&dyn Effect] = &[
    &effects::BLUR,
    &effects::BLOOM,
    &effects::COLOR_GRADE,
    &effects::CHROMATIC_ABERRATION,
    &effects::SHARPEN,
    &effects::VIGNETTE,
    &effects::MOTION_BLUR,
    &effects::GRAIN,
    &effects::LEVELS,
    &effects::DROP_SHADOW,
    &effects::DUOTONE,
    &effects::EDGE,
    &effects::LENS_DISTORTION,
    &effects::POSTERIZE,
];

/// True when `name` is a built-in effect type.
///
/// Effect children (`Blur`, `Bloom`, …) live inside a `Filter`/`Glass` scope,
/// not in the primitive catalog. The browser feature walker must not mistake
/// them for an unknown actor and select the full engine.
pub fn is_effect(name: &str) -> bool {
    EFFECTS.iter().any(|effect| effect.type_name() == name)
}

mod registry;
pub use registry::{
    EffectHandle, PluginEffectData, PluginEffectInfo, effect, effect_for_type, pack_generic,
    plugin_effects, register_extension_effect, unregister_extension_effect,
};

#[cfg(test)]
mod tests;
