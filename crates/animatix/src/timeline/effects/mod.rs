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
//! 1. Create `timeline/effects/<name>.rs` implementing [`Effect`] — WGSL passes, `pack`, and
//!    `support` all live in that one file.
//! 2. Add `&<name>::CONST` to the [`EFFECTS`] array below — this is the registration.
//! 3. Add the author-visible parameters (name, kind, identity) to
//!    `animatix-syntax/src/schema.rs::effect_specs()`.
//! 4. Document it (`docs/effects.md` identity table, `docs/spec.md` table).
//!
//! Identity is the authored type name ([`EffectId`]), so there is no enum variant to add; step 3
//! is hand-maintained because the analyzer's contract table lives in a crate the runtime cannot
//! depend on. The `effect_specs_match_runtime_descriptors` test pins the table in both directions.

mod blur;
mod chain;
mod chromatic_aberration;
mod color_grade;
mod track;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

pub use blur::BLUR;
pub use chain::{EffectChain, EffectInstance, EffectRegion, FilterBackend, PendingComposite};
pub use chromatic_aberration::CHROMATIC_ABERRATION;
pub use color_grade::{COLOR_GRADE, compose_color_matrix};
pub use track::{
    EffectChainTrack, EffectStage, effect_property_kind, identity_to_property, sample_params,
    value_to_property,
};

// ── Effect identity ─────────────────────────────────────────────────────────

/// Stable identity of an effect: its **authored type name**. Used as the
/// pipeline-cache key and persisted as the `kind` of an [`EffectStage`], so a
/// saved project keeps pointing at the same effect across runs — including
/// plugin effects, which have no host-assigned slot to drift.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct EffectId(
    /// The authored type name (`Blur`, `ColorGrade`, a plugin effect's name).
    pub Box<str>,
);

impl EffectId {
    /// Identity for an authored type name.
    pub fn new(name: impl Into<Box<str>>) -> Self {
        Self(name.into())
    }

    /// The authored type name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
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

// ── Author-visible contract (single source: the shared schema table) ───────

/// The author-visible contract for a built-in effect, derived once from
/// `animatix_syntax::schema::effect_specs()`. Parameter `offset`/`size` follow
/// the host layout rule (scalars 4-byte aligned, vec2 at 8, vec4 at 16,
/// sequential), so the generic packer can fill the uniform without a
/// hand-written layout.
struct EffectContract {
    display_name: &'static str,
    params: &'static [EffectParamSpec],
}

fn kind_from_shared(kind: animatix_syntax::schema::PropertyValueKind) -> Option<EffectParamKind> {
    use animatix_syntax::schema::PropertyValueKind as S;
    Some(match kind {
        S::F32 => EffectParamKind::F32,
        S::U32 => EffectParamKind::U32,
        S::Bool => EffectParamKind::Bool,
        S::Vec2 => EffectParamKind::Vec2,
        S::Vec4 => EffectParamKind::Vec4,
        other => {
            tracing::warn!(
                "effect contract parameter kind {other:?} is not marshalable; parameter skipped"
            );
            return None;
        },
    })
}

/// Identity value for a declared kind from its raw `[x, y, z, w]` form — the
/// same convention as the plugin ABI's `NativeEffectParam::identity`.
pub(crate) fn identity_for(kind: EffectParamKind, raw: [f32; 4]) -> EffectParamValue {
    match kind {
        EffectParamKind::F32 => EffectParamValue::F32(raw[0]),
        EffectParamKind::U32 => EffectParamValue::U32(raw[0].max(0.0) as u32),
        EffectParamKind::Bool => EffectParamValue::Bool(raw[0] != 0.0),
        EffectParamKind::Vec2 => EffectParamValue::Vec2([raw[0], raw[1]]),
        EffectParamKind::Vec4 => EffectParamValue::Vec4(raw),
    }
}

/// Byte size and natural alignment of one marshalled parameter kind.
fn kind_layout(kind: EffectParamKind) -> (u32, u32) {
    match kind {
        EffectParamKind::F32 | EffectParamKind::U32 | EffectParamKind::Bool => (4, 4),
        EffectParamKind::Vec2 => (8, 8),
        EffectParamKind::Vec4 => (16, 16),
    }
}

/// Uniform buffer size covering every parameter, rounded up to 16 bytes.
fn uniform_size_for(params: &[EffectParamSpec]) -> u32 {
    params
        .iter()
        .map(|spec| spec.offset + spec.size)
        .max()
        .unwrap_or(0)
        .next_multiple_of(16)
        .max(16)
}

fn built_in_contracts() -> &'static HashMap<&'static str, EffectContract> {
    static CONTRACTS: OnceLock<HashMap<&'static str, EffectContract>> = OnceLock::new();
    CONTRACTS.get_or_init(|| {
        animatix_syntax::schema::effect_specs()
            .iter()
            .filter_map(|spec| {
                let mut offset = 0u32;
                let params: Vec<EffectParamSpec> = spec
                    .params
                    .iter()
                    .filter_map(|declared| {
                        let kind = kind_from_shared(declared.kind)?;
                        let (size, align) = kind_layout(kind);
                        offset = offset.next_multiple_of(align);
                        let laid_out = EffectParamSpec {
                            name: declared.name,
                            kind,
                            identity: identity_for(kind, declared.identity),
                            offset,
                            size,
                        };
                        offset += size;
                        Some(laid_out)
                    })
                    .collect();
                let params: &'static [EffectParamSpec] = Box::leak(params.into_boxed_slice());
                let contract = EffectContract {
                    display_name: spec.display_name,
                    params,
                };
                Some((spec.type_name, contract))
            })
            .collect()
    })
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
    /// Authored type name (`.amx`) — the effect's identity.
    fn type_name(&self) -> &'static str;

    /// Human-readable label for palettes and tooltips, from the contract
    /// table.
    fn display_name(&self) -> &'static str {
        built_in_contracts()
            .get(self.type_name())
            .map_or(self.type_name(), |contract| contract.display_name)
    }

    /// Parameter schema, derived from the contract table
    /// (`animatix_syntax::schema::effect_specs()`). Plugin effects override
    /// this with their FFI-declared schema.
    fn params(&self) -> &'static [EffectParamSpec] {
        built_in_contracts()
            .get(self.type_name())
            .map_or(&[], |contract| contract.params)
    }

    /// Ordered compute passes.
    fn passes(&self) -> &'static [EffectPassSpec];

    /// Size in bytes of the author uniform buffer, derived from the parameter
    /// layout. An effect whose WGSL uniform struct is hand-padded beyond its
    /// parameters (e.g. a colour matrix) overrides this with the struct's real
    /// size.
    fn author_uniform_size(&self) -> u32 {
        uniform_size_for(self.params())
    }

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
/// Returns `None` for a name that is not (or no longer) registered — callers
/// must skip the stage with a diagnostic instead of panicking.
pub fn effect(id: &EffectId) -> Option<&'static dyn Effect> {
    effect_for_type(id.as_str())
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

fn plugin_registry() -> &'static Mutex<BTreeMap<Box<str>, &'static PluginEffect>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<Box<str>, &'static PluginEffect>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Register a plugin-authored effect under its authored type name.
///
/// The effect is leaked into `&'static` storage: registration happens once per
/// plugin load and the allocation is bounded by the number of effects.
/// Re-registering an existing type name is idempotent (it returns the existing
/// registration); a name that collides with a built-in effect is rejected.
pub fn register_extension_effect(data: PluginEffectData) -> Option<&'static str> {
    if EFFECTS.iter().any(|effect| effect.type_name() == data.type_name) {
        tracing::warn!(
            effect = %data.type_name,
            "plugin effect name collides with a built-in effect; rejected"
        );
        return None;
    }
    let mut registry = plugin_registry().lock().ok()?;
    if let Some(existing) =
        registry.values().find(|existing| existing.data.type_name == data.type_name)
    {
        return Some(existing.data.type_name);
    }
    let leaked: &'static PluginEffect = Box::leak(Box::new(PluginEffect { data }));
    let name = leaked.data.type_name;
    registry.insert(Box::from(name), leaked);
    Some(name)
}

/// Remove a plugin effect registration (rollback of a partially failed plugin
/// install). Stages already built against the name will skip with a diagnostic
/// until rebuilt.
pub fn unregister_extension_effect(type_name: &str) {
    if let Some(registry) = plugin_registry().lock().ok().as_mut() {
        registry.remove(type_name);
    }
}

/// Author-visible info for one registered plugin effect, for manifest
/// generation and tooling.
#[derive(Clone, Debug)]
pub struct PluginEffectInfo {
    /// Authored type name (`.amx`).
    pub type_name: &'static str,
    /// Human-readable label.
    pub display_name: &'static str,
    /// Declared parameters (names, kinds, identities).
    pub params: &'static [EffectParamSpec],
}

/// All currently registered plugin effects, sorted by type name.
pub fn plugin_effects() -> Vec<PluginEffectInfo> {
    let Some(registry) = plugin_registry().lock().ok() else {
        return Vec::new();
    };
    registry
        .values()
        .map(|effect| PluginEffectInfo {
            type_name: effect.data.type_name,
            display_name: if effect.data.display_name.is_empty() {
                effect.data.type_name
            } else {
                effect.data.display_name
            },
            params: effect.data.params,
        })
        .collect()
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

    fn kind_is_marshalable(shared: animatix_syntax::schema::PropertyValueKind) -> bool {
        use animatix_syntax::schema::PropertyValueKind as S;
        matches!(shared, S::F32 | S::U32 | S::Bool | S::Vec2 | S::Vec4)
    }

    /// Every built-in effect must have a contract row in the shared schema
    /// table — it is the single source of the effect's parameters, so a
    /// declared effect without one silently loses its parameters.
    #[test]
    fn every_built_in_effect_has_a_contract_row() {
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
                spec.params.iter().filter(|p| kind_is_marshalable(p.kind)).count(),
                "{} derived parameter count mismatch",
                effect.type_name()
            );
            for (runtime, declared) in effect.params().iter().zip(spec.params) {
                assert_eq!(runtime.name, declared.name, "{} parameter order", effect.type_name());
                assert!(
                    kind_is_marshalable(declared.kind),
                    "{}.{} kind {:?} is not marshalable",
                    effect.type_name(),
                    declared.name,
                    declared.kind
                );
            }
            // The derived layout must fill a valid uniform buffer.
            assert_eq!(effect.author_uniform_size() % 16, 0);
            assert!(effect.author_uniform_size() >= 16);
        }
    }

    /// The derived uniform layout matches the host rule (scalars 4-byte
    /// aligned, vec2 at 8, vec4 at 16, sequential), so packers can rely on the
    /// declared offsets.
    #[test]
    fn derived_contract_layout_follows_host_rule() {
        let blur = effect_for_type("Blur").expect("Blur");
        let params = blur.params();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "radius");
        assert_eq!(params[0].offset, 0);
        assert_eq!(params[0].size, 4);
        assert_eq!(params[0].identity, super::EffectParamValue::F32(0.0));
        assert_eq!(blur.author_uniform_size(), 16);

        let grade = effect_for_type("ColorGrade").expect("ColorGrade");
        let offsets: Vec<u32> = grade.params().iter().map(|spec| spec.offset).collect();
        assert_eq!(offsets, vec![0, 4, 8, 12, 16]);
        assert_eq!(grade.params()[0].identity, super::EffectParamValue::F32(1.0));
    }

    #[test]
    fn every_effect_is_addressable_by_id_and_type() {
        for declared in EFFECTS {
            let id = EffectId::new(declared.type_name());
            assert_eq!(
                effect(&id).map(Effect::type_name),
                Some(declared.type_name()),
                "{} is not reachable by id",
                declared.type_name()
            );
            assert_eq!(
                effect_for_type(declared.type_name()).map(Effect::type_name),
                Some(declared.type_name()),
                "{} is not reachable by type name",
                declared.type_name()
            );
        }
    }

    /// The persisted identity is the authored name, so a serialized stage kind
    /// round-trips through serde as a bare string — including for plugin
    /// effects, which have no host-assigned slot to drift between runs.
    #[cfg(feature = "serde")]
    #[test]
    fn effect_id_serializes_as_the_authored_name() {
        let id = EffectId::new("Blur");
        let json = serde_json::to_string(&id).expect("serialize EffectId");
        assert_eq!(json, "\"Blur\"");
        let round: EffectId = serde_json::from_str(&json).expect("deserialize EffectId");
        assert_eq!(round, id);
    }

    #[test]
    fn built_in_type_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for effect in EFFECTS {
            assert!(seen.insert(effect.type_name()), "duplicate type name {}", effect.type_name());
        }
    }
}
