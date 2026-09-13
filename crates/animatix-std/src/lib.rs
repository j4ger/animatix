//! The built-in standard library: effect definitions and the primitive
//! catalog.
//!
//! This crate is the single author-visible source for what the language's
//! built-ins are. It sits above [`animatix-core`](animatix_core) and below
//! everything else: the engine links it for behaviour, and the parser crate
//! derives the author-visible contract tables from it — so adding a built-in
//! is one file plus one registration line, in this crate, with no parallel
//! declaration anywhere else.
//!
//! Effects are complete implementations (parameters, WGSL passes, `pack`,
//! `support`); primitives are represented by their identity cards
//! ([`PrimitiveInfo`] / [`CATALOG`]) while their behaviour lives in the
//! engine, because building an actor needs the engine's `Timeline`.
//!
//! The normative contract for the effect pass layout, uniforms, identity
//! semantics, and failure policy lives in `docs/effects.md`.

pub mod catalog;
pub mod effects;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use animatix_core::effect::{Effect, EffectParamSpec, EffectParams, EffectPassSpec};

pub use catalog::{CATALOG, PrimitiveInfo, caps_for_type, caps_from_info, catalog_lookup};
pub use effects::{BLUR, CHROMATIC_ABERRATION, COLOR_GRADE};

// ── Built-in catalog ────────────────────────────────────────────────────────

/// Bootstrap list of all built-in effects, in registration order.
pub static EFFECTS: &[&dyn Effect] = &[
    &effects::BLUR,
    &effects::COLOR_GRADE,
    &effects::CHROMATIC_ABERRATION,
];

// ── Dispatch ────────────────────────────────────────────────────────────────

/// A resolved effect: either a built-in singleton or a registered plugin
/// effect, which the registry owns behind an `Arc`.
///
/// Derefs to `dyn Effect`, so callers that only read metadata or call
/// `pack`/`support` never need to name this type.
#[derive(Clone)]
pub struct EffectHandle(EffectHandleInner);

#[derive(Clone)]
enum EffectHandleInner {
    Builtin(&'static dyn Effect),
    Plugin(Arc<PluginEffect>),
}

impl EffectHandle {
    /// The effect behind this handle.
    pub fn as_effect(&self) -> &(dyn Effect + 'static) {
        match &self.0 {
            EffectHandleInner::Builtin(effect) => *effect,
            EffectHandleInner::Plugin(effect) => effect.as_ref(),
        }
    }
}

impl std::ops::Deref for EffectHandle {
    type Target = dyn Effect;

    fn deref(&self) -> &(dyn Effect + 'static) {
        self.as_effect()
    }
}

/// Look up a built-in or plugin effect by identity.
///
/// Returns `None` for a name that is not (or no longer) registered — callers
/// must skip the stage with a diagnostic instead of panicking.
pub fn effect(id: &animatix_core::effect::EffectId) -> Option<EffectHandle> {
    effect_for_type(id.as_str())
}

/// Look up an effect by its authored type name: built-ins first, then
/// plugin-registered effects.
pub fn effect_for_type(type_name: &str) -> Option<EffectHandle> {
    if let Some(builtin) = EFFECTS.iter().find(|effect| effect.type_name() == type_name) {
        return Some(EffectHandle(EffectHandleInner::Builtin(*builtin)));
    }
    let registry = plugin_registry().lock().ok()?;
    registry
        .get(type_name)
        .map(|effect| EffectHandle(EffectHandleInner::Plugin(Arc::clone(effect))))
}

// ── Extension (plugin-authored) effect registry ─────────────────────────────

/// FFI-declared effect data. Plugins supply WGSL source and a parameter schema,
/// never a GPU handle; the host compiles the shader with its own device and
/// owns all textures and synchronisation.
///
/// The host owns this data outright — registration into the registry below
/// transfers ownership, so nothing is leaked and a rollback
/// (`unregister_extension_effect`) frees it.
#[derive(Clone, Debug)]
pub struct PluginEffectData {
    /// Authored type name (`.amx`).
    pub type_name: Box<str>,
    /// Human-readable label; falls back to `type_name` when empty.
    pub display_name: Box<str>,
    /// Declared parameters and their uniform offsets.
    pub params: Vec<EffectParamSpec>,
    /// Ordered compute passes.
    pub passes: Vec<EffectPassSpec>,
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
    fn type_name(&self) -> &str {
        &self.data.type_name
    }

    fn display_name(&self) -> &str {
        if self.data.display_name.is_empty() {
            &self.data.type_name
        } else {
            &self.data.display_name
        }
    }

    fn params(&self) -> &[EffectParamSpec] {
        &self.data.params
    }

    fn passes(&self) -> &[EffectPassSpec] {
        &self.data.passes
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

fn plugin_registry() -> &'static Mutex<BTreeMap<Box<str>, Arc<PluginEffect>>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<Box<str>, Arc<PluginEffect>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Register a plugin-authored effect under its authored type name.
///
/// The registry owns the data; the returned name identifies the registration.
/// Re-registering an existing type name is idempotent (it returns the existing
/// registration); a name that collides with a built-in effect is rejected.
pub fn register_extension_effect(data: PluginEffectData) -> Option<Box<str>> {
    if EFFECTS.iter().any(|effect| effect.type_name() == data.type_name.as_ref()) {
        tracing::warn!(
            effect = %data.type_name,
            "plugin effect name collides with a built-in effect; rejected"
        );
        return None;
    }
    let mut registry = plugin_registry().lock().ok()?;
    if let Some(existing) = registry.get(data.type_name.as_ref()) {
        return Some(existing.data.type_name.clone());
    }
    let name = data.type_name.clone();
    registry.insert(data.type_name.clone(), Arc::new(PluginEffect { data }));
    Some(name)
}

/// Remove a plugin effect registration (rollback of a partially failed plugin
/// install); the effect's data is dropped. Stages already built against the
/// name will skip with a diagnostic until rebuilt.
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
    pub type_name: Box<str>,
    /// Human-readable label.
    pub display_name: Box<str>,
    /// Declared parameters (names, kinds, identities).
    pub params: Vec<EffectParamSpec>,
}

/// All currently registered plugin effects, sorted by type name.
pub fn plugin_effects() -> Vec<PluginEffectInfo> {
    let Some(registry) = plugin_registry().lock().ok() else {
        return Vec::new();
    };
    registry
        .values()
        .map(|effect| PluginEffectInfo {
            type_name: effect.data.type_name.clone(),
            display_name: if effect.data.display_name.is_empty() {
                effect.data.type_name.clone()
            } else {
                effect.data.display_name.clone()
            },
            params: effect.data.params.clone(),
        })
        .collect()
}

/// Generic uniform packer for plugin effects: lays each declared parameter out
/// at its declared offset (the host assigns 4-byte-aligned offsets for scalars,
/// 8 for vec2, 16 for vec4 when registering).
pub fn pack_generic(data: &PluginEffectData, params: &EffectParams, out: &mut [u8]) {
    use animatix_core::effect::{EffectParamKind as K, EffectParamValue as V};
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
    use animatix_core::effect::{EffectParamKind, EffectParamValue, uniform_size_for};

    /// Declared parameters follow the host layout rule (scalars 4-byte
    /// aligned, vec2 at 8, vec4 at 16, sequential), so packers can rely on
    /// the offsets and the derived uniform size fills a valid buffer.
    #[test]
    fn declared_param_layout_follows_host_rule() {
        for effect in EFFECTS {
            let mut offset = 0u32;
            for spec in effect.params() {
                let (size, align) = animatix_core::effect::kind_layout(spec.kind);
                assert_eq!(spec.offset, offset, "{}.{} offset", effect.type_name(), spec.name);
                assert_eq!(spec.size, size, "{}.{} size", effect.type_name(), spec.name);
                offset = (offset + size).next_multiple_of(align);
            }
            assert_eq!(effect.author_uniform_size() % 16, 0);
            // The uniform buffer must cover every declared parameter; effects
            // with hand-padded WGSL structs (ColorGrade's 4×4 matrix) may be
            // larger than the parameter layout.
            assert!(
                effect.author_uniform_size() >= uniform_size_for(effect.params()),
                "{} uniform size under-covers its parameters",
                effect.type_name()
            );
        }
    }

    #[test]
    fn every_effect_is_addressable_by_id_and_type() {
        for declared in EFFECTS {
            let id = animatix_core::effect::EffectId::new(declared.type_name());
            assert_eq!(
                effect(&id).map(|effect| effect.type_name().to_owned()).as_deref(),
                Some(declared.type_name()),
                "{} is not reachable by id",
                declared.type_name()
            );
            assert_eq!(
                effect_for_type(declared.type_name())
                    .map(|effect| effect.type_name().to_owned())
                    .as_deref(),
                Some(declared.type_name()),
                "{} is not reachable by type name",
                declared.type_name()
            );
        }
    }

    /// The persisted identity is the authored name, so a serialized stage kind
    /// round-trips through serde as a bare string — including for plugin
    /// effects, which have no host-assigned slot to drift between runs.
    #[test]
    fn effect_id_serializes_as_the_authored_name() {
        let id = animatix_core::effect::EffectId::new("Blur");
        let json = serde_json::to_string(&id).expect("serialize EffectId");
        assert_eq!(json, "\"Blur\"");
        let round: animatix_core::effect::EffectId =
            serde_json::from_str(&json).expect("deserialize EffectId");
        assert_eq!(round, id);
    }

    #[test]
    fn built_in_type_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for effect in EFFECTS {
            assert!(seen.insert(effect.type_name()), "duplicate type name {}", effect.type_name());
        }
    }

    #[test]
    fn plugin_effect_registers_by_name_and_rejects_builtin_collision() {
        let params = vec![EffectParamSpec::new(
            "size",
            EffectParamKind::F32,
            EffectParamValue::F32(0.0),
            0,
            4,
        )];
        let passes = vec![EffectPassSpec::new(
            "std-test-pixelate",
            "@compute fn main() {}",
            "main",
        )];

        let registered = register_extension_effect(PluginEffectData {
            type_name: "StdTestPixelate".into(),
            display_name: "Std Test Pixelate".into(),
            params: params.clone(),
            passes: passes.clone(),
            author_uniform_size: 16,
            support_px: 0.0,
        })
        .expect("plugin effect registers");
        assert_eq!(&*registered, "StdTestPixelate");
        assert!(effect_for_type("StdTestPixelate").is_some());

        // A built-in name is rejected.
        assert!(
            register_extension_effect(PluginEffectData {
                type_name: "Blur".into(),
                display_name: "".into(),
                params,
                passes,
                author_uniform_size: 16,
                support_px: 0.0,
            })
            .is_none(),
            "built-in name collision must be rejected"
        );

        unregister_extension_effect("StdTestPixelate");
        assert!(effect_for_type("StdTestPixelate").is_none());
    }

    /// A plugin effect's data is owned by the registry: unregistering drops it,
    /// so a register/unregister churn cannot leak (the old `Box::leak`
    /// registration could never reclaim anything).
    #[test]
    fn unregister_reclaims_plugin_effect_data() {
        fn register(name: &str) {
            let _ = register_extension_effect(PluginEffectData {
                type_name: name.into(),
                display_name: name.into(),
                params: vec![],
                passes: vec![],
                author_uniform_size: 16,
                support_px: 0.0,
            });
        }

        for i in 0..64 {
            let name = format!("ChurnEffect{i}");
            register(&name);
            assert!(effect_for_type(&name).is_some());
            unregister_extension_effect(&name);
            assert!(effect_for_type(&name).is_none(), "{name} survived unregister");
        }
        assert!(plugin_effects().is_empty(), "churn left registered effects behind");
    }
}
