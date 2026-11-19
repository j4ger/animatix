//! Effect dispatch and plugin registry for the built-in catalog.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::EFFECTS;
use crate::effect::{Effect, EffectParamSpec, EffectParams, EffectPassSpec};

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
pub fn effect(id: &crate::effect::EffectId) -> Option<EffectHandle> {
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
    use crate::effect::{EffectParamKind as K, EffectParamValue as V};
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
