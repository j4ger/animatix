//! Built-in post-processing effects and the effect-chain runtime model.
//!
//! Every built-in effect is a unit struct implementing [`Effect`], registered
//! once in the [`EFFECTS`] bootstrap array; plugin effects are wrapped in
//! [`PluginEffect`] over the data-only [`PluginEffectData`] and stored in the
//! process-wide plugin registry. Both are found through [`effect`] /
//! [`effect_for_type`], so there are no per-effect match arms to maintain.
//!
//! The effect *storage* (an animatable chain owned by a compositing scope) is
//! [`EffectChainTrack`]; the renderer-facing, fully-sampled form is
//! [`EffectChain`]. The normative contract for the pass layout, uniforms,
//! identity semantics, and failure policy lives in `docs/effects.md`.
//!
//! ## Adding a new effect
//!
//! 1. Create `timeline/effects/<name>.rs` implementing [`Effect`] — parameter schema, WGSL passes,
//!    `pack`, and `support` all live in that one file.
//! 2. Add `&<name>::CONST` to the [`EFFECTS`] array below.
//! 3. Add a variant to [`EffectId`].
//! 4. Add the author-visible parameters to `animatix-syntax/src/schema.rs::effect_specs()`.
//! 5. Document it (`docs/effects.md` identity table, `docs/spec.md` table).
//!
//! Steps 3-4 are hand-maintained because the enum is a persisted pipeline-cache
//! key and the analyzer's schema table lives in a crate the runtime cannot
//! depend on — the same constraint `primitives/mod.rs` documents for
//! `ActorKindId`. The `effect_specs_match_runtime_descriptors` test pins the two
//! tables together in both directions.

mod blur;
mod chain;
mod chromatic_aberration;
mod color_grade;
mod track;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

pub use blur::BLUR;
pub use chain::{EffectChain, EffectInstance, EffectRegion, FilterBackend, PendingComposite};
pub use chromatic_aberration::CHROMATIC_ABERRATION;
pub use color_grade::{COLOR_GRADE, compose_color_matrix};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
pub use track::{
    EffectChainTrack, EffectStage, effect_property_kind, identity_to_property, sample_params,
    value_to_property,
};

// ── Effect identity ─────────────────────────────────────────────────────────

/// Stable identity of an effect, used as the pipeline-cache key and as the
/// persisted `kind` of an [`EffectStage`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum EffectId {
    /// Gaussian blur; two passes (horizontal, then vertical).
    Blur,
    /// Colour matrix built from brightness/contrast/saturate/hue/sepia.
    ColorGrade,
    /// Radial channel separation; single pass through the linear sampler.
    ChromaticAberration,
    /// A plugin-authored effect, identified by its registry slot. The slot is
    /// assigned by [`register_extension_effect`] and stays valid for the
    /// process lifetime (descriptors are leaked once, never freed).
    Extension(u32),
}

// ── Parameter schema ────────────────────────────────────────────────────────

/// Value type of an effect parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectParamKind {
    /// 32-bit float.
    F32,
    /// 32-bit unsigned integer.
    U32,
    /// Two 32-bit floats.
    Vec2,
    /// Four 32-bit floats.
    Vec4,
    /// Boolean, marshalled as `u32`.
    Bool,
}

/// One resolved parameter value for a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectParamValue {
    /// `f32`
    F32(f32),
    /// `u32`
    U32(u32),
    /// `[f32; 2]`
    Vec2([f32; 2]),
    /// `[f32; 4]`
    Vec4([f32; 4]),
    /// `bool`
    Bool(bool),
}

/// Declares one author parameter: its type, the identity value that means "no
/// contribution", and where it lands in the author uniform buffer.
#[derive(Clone, Copy, Debug)]
pub struct EffectParamSpec {
    /// Parameter name as authored in `.amx`.
    pub name: &'static str,
    /// Value type.
    pub kind: EffectParamKind,
    /// Value at which this parameter contributes nothing.
    pub identity: EffectParamValue,
    /// Byte offset in the author uniform buffer (generic packing).
    pub offset: u32,
    /// Byte size in the author uniform buffer.
    pub size: u32,
}

/// Resolved parameters for one effect instance in a frame.
#[derive(Clone, Debug, Default)]
pub struct EffectParams {
    /// Values index-aligned with [`Effect::params`].
    pub values: Vec<EffectParamValue>,
}

impl EffectParams {
    /// Read parameter `index` as `f32` (0.0 for non-numeric or out of range).
    pub fn f32_at(&self, index: usize) -> f32 {
        match self.values.get(index) {
            Some(EffectParamValue::F32(v)) => *v,
            Some(EffectParamValue::U32(v)) => *v as f32,
            _ => 0.0,
        }
    }
}

// ── Passes and the effect trait ─────────────────────────────────────────────

/// One compute pass of an effect: shader source plus entry point.
#[derive(Clone, Copy, Debug)]
pub struct EffectPassSpec {
    /// Human-readable label for the pipeline/pass.
    pub label: &'static str,
    /// WGSL source. May be shared between passes (blur uses one shader twice).
    pub wgsl: &'static str,
    /// Entry point name.
    pub entry: &'static str,
}

/// The single declaration surface for a post-processing effect.
///
/// Built-ins implement this on a unit struct and register the singleton in
/// [`EFFECTS`]; plugin effects are wrapped in [`PluginEffect`], which
/// implements the same trait over FFI-declared data. The renderer only ever
/// sees `&dyn Effect`, so there is no per-effect dispatch table to keep in
/// sync.
pub trait Effect: Send + Sync {
    /// Pipeline-cache identity and persisted stage kind.
    fn id(&self) -> EffectId;

    /// Authored type name (`.amx`).
    fn type_name(&self) -> &'static str;

    /// Human-readable label for palettes and tooltips.
    fn display_name(&self) -> &'static str {
        self.type_name()
    }

    /// Parameter schema, index-aligned with [`EffectParams::values`].
    fn params(&self) -> &'static [EffectParamSpec];

    /// Ordered compute passes.
    fn passes(&self) -> &'static [EffectPassSpec];

    /// Size in bytes of the author uniform buffer (multiple of 16).
    fn author_uniform_size(&self) -> u32;

    /// Marshal parameters into `author_uniform_size` bytes.
    fn pack(&self, params: &EffectParams, out: &mut [u8]);

    /// How far outside a source pixel this effect reads, in scene pixels, for
    /// the given parameters. Used to pad a region of interest.
    fn support(&self, params: &EffectParams) -> f32;

    /// `true` when every parameter equals its identity value — the effect
    /// contributes nothing and its passes can be skipped.
    fn is_identity(&self, params: &EffectParams) -> bool {
        self.params()
            .iter()
            .enumerate()
            .all(|(i, spec)| params.values.get(i).is_none_or(|value| *value == spec.identity))
    }
}

/// Bootstrap list of all built-in effects, in registration order.
pub static EFFECTS: &[&dyn Effect] = &[
    &blur::BLUR,
    &color_grade::COLOR_GRADE,
    &chromatic_aberration::CHROMATIC_ABERRATION,
];

// ── Dispatch ────────────────────────────────────────────────────────────────

/// Look up a built-in or plugin effect by identity.
///
/// Returns `None` when an `Extension` slot is not (or no longer) registered —
/// callers must skip the stage with a diagnostic instead of panicking.
pub fn effect(id: EffectId) -> Option<&'static dyn Effect> {
    EFFECTS
        .iter()
        .copied()
        .find(|effect| effect.id() == id)
        .or_else(|| plugin_effect(id))
}

/// Look up an effect by its authored type name: built-ins first, then
/// plugin-registered effects.
pub fn effect_for_type(type_name: &str) -> Option<&'static dyn Effect> {
    EFFECTS
        .iter()
        .copied()
        .find(|effect| effect.type_name() == type_name)
        .or_else(|| plugin_effect_by_type(type_name))
}

// ── Extension (plugin-authored) effect registry ─────────────────────────────

/// FFI-declared effect data. Plugins supply WGSL source and a parameter schema,
/// never a GPU handle; the host compiles the shader with its own device and
/// owns all textures and synchronisation.
#[derive(Clone, Debug)]
pub struct PluginEffectData {
    /// Registry identity. Overwritten with the assigned slot on registration.
    pub id: EffectId,
    /// Authored type name (`.amx`).
    pub type_name: &'static str,
    /// Human-readable label; falls back to `type_name` when empty.
    pub display_name: &'static str,
    /// Declared parameters and their uniform offsets.
    pub params: &'static [EffectParamSpec],
    /// Ordered compute passes.
    pub passes: &'static [EffectPassSpec],
    /// Size in bytes of the author uniform buffer (multiple of 16).
    pub author_uniform_size: u32,
    /// Conservative spatial support in scene pixels. Plugin parameter schemas
    /// cross the FFI boundary, support functions do not.
    pub support_px: f32,
}

/// A registered plugin effect: data-only descriptor adapted to [`Effect`].
struct PluginEffect {
    data: PluginEffectData,
}

impl Effect for PluginEffect {
    fn id(&self) -> EffectId {
        self.data.id
    }

    fn type_name(&self) -> &'static str {
        self.data.type_name
    }

    fn display_name(&self) -> &'static str {
        if self.data.display_name.is_empty() {
            self.data.type_name
        } else {
            self.data.display_name
        }
    }

    fn params(&self) -> &'static [EffectParamSpec] {
        self.data.params
    }

    fn passes(&self) -> &'static [EffectPassSpec] {
        self.data.passes
    }

    fn author_uniform_size(&self) -> u32 {
        self.data.author_uniform_size
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        pack_generic(&self.data, params, out);
    }

    fn support(&self, _params: &EffectParams) -> f32 {
        self.data.support_px
    }
}

fn plugin_registry() -> &'static Mutex<BTreeMap<u32, &'static PluginEffect>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<u32, &'static PluginEffect>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Register a plugin-authored effect and return its registry slot.
///
/// The effect is leaked into `&'static` storage: registration happens once per
/// plugin load and the allocation is bounded by the number of effects.
/// Re-registering an existing type name is idempotent (it returns the existing
/// slot); a name that collides with a built-in effect is rejected.
pub fn register_extension_effect(mut data: PluginEffectData) -> Option<u32> {
    if EFFECTS.iter().any(|effect| effect.type_name() == data.type_name) {
        tracing::warn!(
            effect = %data.type_name,
            "plugin effect name collides with a built-in effect; rejected"
        );
        return None;
    }
    let mut registry = plugin_registry().lock().ok()?;
    if let Some((slot, _)) =
        registry.iter().find(|(_, existing)| existing.data.type_name == data.type_name)
    {
        return Some(*slot);
    }
    let slot = next_extension_effect_slot();
    data.id = EffectId::Extension(slot);
    let leaked: &'static PluginEffect = Box::leak(Box::new(PluginEffect { data }));
    registry.insert(slot, leaked);
    Some(slot)
}

/// Remove a plugin effect registration (rollback of a partially failed plugin
/// install). Stages already built against the slot will skip with a diagnostic
/// until rebuilt.
pub fn unregister_extension_effect(slot: u32) {
    if let Some(registry) = plugin_registry().lock().ok().as_mut() {
        registry.remove(&slot);
    }
}

fn next_extension_effect_slot() -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn plugin_effect(id: EffectId) -> Option<&'static dyn Effect> {
    let EffectId::Extension(slot) = id else {
        return None;
    };
    let registry = plugin_registry().lock().ok()?;
    registry.get(&slot).map(|effect| *effect as &'static dyn Effect)
}

fn plugin_effect_by_type(type_name: &str) -> Option<&'static dyn Effect> {
    let registry = plugin_registry().lock().ok()?;
    registry
        .values()
        .find(|effect| effect.data.type_name == type_name)
        .map(|effect| *effect as &'static dyn Effect)
}

/// Generic uniform packer for plugin effects: lays each declared parameter out
/// at its declared offset (the host assigns 4-byte-aligned offsets for scalars,
/// 8 for vec2, 16 for vec4 when registering).
pub fn pack_generic(data: &PluginEffectData, params: &EffectParams, out: &mut [u8]) {
    use EffectParamKind as K;
    use EffectParamValue as V;
    out.fill(0);
    for (index, spec) in data.params.iter().enumerate() {
        let Some(value) = params.values.get(index) else {
            continue;
        };
        let offset = spec.offset as usize;
        let write = |out: &mut [u8], bytes: &[u8]| {
            let end = (offset + bytes.len()).min(out.len());
            if offset < end {
                out[offset..end].copy_from_slice(&bytes[..end - offset]);
            }
        };
        match (spec.kind, value) {
            (K::F32, V::F32(v)) => write(out, &v.to_le_bytes()),
            (K::U32, V::U32(v)) => write(out, &v.to_le_bytes()),
            (K::Bool, V::Bool(v)) => write(out, &[u8::from(*v), 0, 0, 0]),
            (K::Vec2, V::Vec2(v)) => {
                let mut bytes = [0u8; 8];
                for (i, scalar) in v.iter().enumerate() {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(&scalar.to_le_bytes());
                }
                write(out, &bytes);
            },
            (K::Vec4, V::Vec4(v)) => {
                let mut bytes = [0u8; 16];
                for (i, scalar) in v.iter().enumerate() {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(&scalar.to_le_bytes());
                }
                write(out, &bytes);
            },
            (kind, value) => {
                tracing::warn!(
                    "effect parameter '{}' stored as {value:?} does not match declared {kind:?}",
                    spec.name
                );
            },
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

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

    /// The runtime effect declarations and the analyzer's shared effect table
    /// must agree exactly: same set, same display names, same parameter names
    /// and kinds. Bidirectional (count + per-entry), so a built-in added to
    /// only one side fails here — mirroring
    /// `primitives::registry_specs_match_shared_schema_for_builtins`.
    #[test]
    fn effect_specs_match_runtime_descriptors() {
        let shared = animatix_syntax::schema::effect_specs();
        assert_eq!(
            EFFECTS.len(),
            shared.len(),
            "runtime and shared-schema built-in effect counts drifted"
        );
        for effect in EFFECTS {
            let spec = animatix_syntax::schema::effect_spec(effect.type_name())
                .unwrap_or_else(|| panic!("missing shared effect spec for {}", effect.type_name()));
            assert_eq!(
                effect.display_name(),
                spec.display_name,
                "{} display name drifted",
                effect.type_name()
            );
            assert_eq!(
                effect.params().len(),
                spec.params.len(),
                "parameter count mismatch for {}",
                effect.type_name()
            );
            for (runtime, declared) in effect.params().iter().zip(spec.params) {
                assert_eq!(runtime.name, declared.name, "{} parameter order", effect.type_name());
                assert!(
                    kind_matches(runtime.kind, declared.kind),
                    "{}.{} kind mismatch: {:?} vs {:?}",
                    effect.type_name(),
                    runtime.name,
                    runtime.kind,
                    declared.kind
                );
            }
        }
    }

    #[test]
    fn every_effect_is_addressable_by_id_and_type() {
        for declared in EFFECTS {
            assert_eq!(
                effect(declared.id()).map(Effect::type_name),
                Some(declared.type_name()),
                "{} is not reachable by id",
                declared.type_name()
            );
            assert_eq!(
                effect_for_type(declared.type_name()).map(Effect::id),
                Some(declared.id()),
                "{} is not reachable by type name",
                declared.type_name()
            );
        }
    }

    #[test]
    fn built_in_type_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for effect in EFFECTS {
            assert!(seen.insert(effect.type_name()), "duplicate type name {}", effect.type_name());
        }
    }
}
