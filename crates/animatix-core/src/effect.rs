//! The effect definition surface: identity, parameter schema, and the
//! [`Effect`] trait.
//!
//! Everything here is engine-free — no wgpu, no scene graph. Built-in
//! implementations live in `animatix-std` (next to their WGSL); the engine
//! consumes the trait through its render pipeline, and the parser crate
//! derives the author-visible contract from the same catalog.
//!
//! Metadata strings are [`Cow<'static, str>`]: built-ins borrow string
//! literals from their `static` declarations (zero allocation), while plugin
//! effects own the strings they received over FFI. Nothing requires a
//! `'static` *reference*, so registering a plugin effect allocates nothing
//! permanently and unregistering frees it.

use std::borrow::Cow;

// ── Effect identity ─────────────────────────────────────────────────────────

/// Stable identity of an effect: its **authored type name**. Used as the
/// pipeline-cache key and persisted as the `kind` of an effect stage, so a
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
#[derive(Clone, Debug)]
pub struct EffectParamSpec {
    /// Parameter name as authored in `.amx`.
    pub name: Cow<'static, str>,
    /// Value type.
    pub kind: EffectParamKind,
    /// Value at which this parameter contributes nothing.
    pub identity: EffectParamValue,
    /// Byte offset in the author uniform buffer (generic packing).
    pub offset: u32,
    /// Byte size in the author uniform buffer.
    pub size: u32,
}

impl EffectParamSpec {
    /// Built-in declaration: the name is a string literal borrowed for
    /// `'static`, so this is usable in a `static` parameter table.
    pub const fn new(
        name: &'static str,
        kind: EffectParamKind,
        identity: EffectParamValue,
        offset: u32,
        size: u32,
    ) -> Self {
        Self {
            name: Cow::Borrowed(name),
            kind,
            identity,
            offset,
            size,
        }
    }
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

/// One compute pass of an effect: shader source plus entry point.
#[derive(Clone, Debug)]
pub struct EffectPassSpec {
    /// Human-readable label for the pipeline/pass.
    pub label: Cow<'static, str>,
    /// WGSL source. May be shared between passes (blur uses one shader twice).
    pub wgsl: Cow<'static, str>,
    /// Entry point name.
    pub entry: Cow<'static, str>,
}

impl EffectPassSpec {
    /// Built-in declaration: label, WGSL, and entry point borrow string
    /// literals for `'static`, so this is usable in a `static` pass table.
    pub const fn new(label: &'static str, wgsl: &'static str, entry: &'static str) -> Self {
        Self {
            label: Cow::Borrowed(label),
            wgsl: Cow::Borrowed(wgsl),
            entry: Cow::Borrowed(entry),
        }
    }
}

// ── Layout helpers ──────────────────────────────────────────────────────────

/// Identity value for a declared kind from its raw `[x, y, z, w]` form — the
/// same convention as the plugin ABI's `NativeEffectParam::identity`.
pub fn identity_for(kind: EffectParamKind, raw: [f32; 4]) -> EffectParamValue {
    match kind {
        EffectParamKind::F32 => EffectParamValue::F32(raw[0]),
        EffectParamKind::U32 => EffectParamValue::U32(raw[0].max(0.0) as u32),
        EffectParamKind::Bool => EffectParamValue::Bool(raw[0] != 0.0),
        EffectParamKind::Vec2 => EffectParamValue::Vec2([raw[0], raw[1]]),
        EffectParamKind::Vec4 => EffectParamValue::Vec4(raw),
    }
}

/// Byte size and natural alignment of one marshalled parameter kind.
pub fn kind_layout(kind: EffectParamKind) -> (u32, u32) {
    match kind {
        EffectParamKind::F32 | EffectParamKind::U32 | EffectParamKind::Bool => (4, 4),
        EffectParamKind::Vec2 => (8, 8),
        EffectParamKind::Vec4 => (16, 16),
    }
}

/// Uniform buffer size covering every parameter, rounded up to 16 bytes.
pub fn uniform_size_for(params: &[EffectParamSpec]) -> u32 {
    params
        .iter()
        .map(|spec| spec.offset + spec.size)
        .max()
        .unwrap_or(0)
        .next_multiple_of(16)
        .max(16)
}

// ── The effect trait ────────────────────────────────────────────────────────

/// The single declaration surface for a post-processing effect.
///
/// Built-ins implement this on a unit struct and register the singleton in
/// the `animatix-std` catalog; plugin effects adapt FFI-declared data to the
/// same trait. The renderer only ever sees `&dyn Effect`, so there is no
/// per-effect dispatch table to keep in sync.
pub trait Effect: Send + Sync {
    /// Authored type name (`.amx`) — the effect's identity.
    fn type_name(&self) -> &str;

    /// Human-readable label for palettes and tooltips.
    fn display_name(&self) -> &str {
        self.type_name()
    }

    /// Parameter schema, index-aligned with [`EffectParams::values`].
    fn params(&self) -> &[EffectParamSpec];

    /// Ordered compute passes.
    fn passes(&self) -> &[EffectPassSpec];

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
