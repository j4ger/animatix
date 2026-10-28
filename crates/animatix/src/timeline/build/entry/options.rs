//! Optional inputs for the timeline build entry point.
//!
//! [`BuildOptions`] is the single carrier for everything the build accepts
//! beyond the AST and the namespace map. Every public build entry point builds
//! one of these and delegates to [`Timeline::build_with_options`], so the
//! lowering pass has exactly one signature to thread.

use std::sync::Arc;

use super::*;

/// A predecessor scene's carried state plus what re-rooting it needs.
///
/// Bundled as one value because the four fields only make sense together: a
/// carry bag without its source timeline cannot resolve layout-managed world
/// positions, and `source_duration_ms`/`dims` are sampled from that timeline.
#[derive(Clone, Copy)]
pub struct CarryInput<'a> {
    /// Actors (and camera) to thread into the scene being built.
    pub bag: &'a crate::timeline::persistence::CarryBag,
    /// The predecessor timeline the bag was computed from.
    pub source_timeline: &'a Timeline,
    /// Duration of the predecessor scene in milliseconds.
    pub source_duration_ms: u64,
    /// Scene pixel dimensions `[width, height]`.
    pub dims: [f64; 2],
}

/// Optional inputs for [`Timeline::build_with_options`].
///
/// `..Default::default()` reproduces [`Timeline::build_with_diagnostics`]:
/// fresh font context, production quality, no carry, no asset cache, the
/// built-in primitive registry, no extensions and no `--set` overrides.
///
/// Cheap to clone: the heavy inputs are `Arc`s and the rest are references.
#[derive(Clone, Default)]
pub struct BuildOptions<'a> {
    /// Shared font database. `None` creates a fresh one for this build.
    pub font_context: Option<Arc<crate::renderer::text::FontContext>>,
    /// Build fidelity. `None` is [`BuildQuality::Production`].
    pub quality: Option<BuildQuality>,
    /// Carried state from a predecessor scene (multi-scene composition).
    pub carry: Option<CarryInput<'a>>,
    /// Asset cache carried from a previous build.
    pub asset_cache: Option<Arc<super::assets::AssetCache>>,
    /// Primitive registry to install. `None` keeps the timeline's default, or
    /// the one carried by [`Self::extensions`] when an extension context is set.
    pub primitive_registry: Option<Arc<crate::primitives::PrimitiveRegistry>>,
    /// Extension context installed into the timeline and its build environment.
    pub extensions: Option<Arc<crate::extension_context::ExtensionContext>>,
    /// `--set NAME=VALUE` overrides applied to the build environment.
    pub defines: &'a [(String, String)],
}

impl<'a> BuildOptions<'a> {
    /// Use `font_context` instead of creating a fresh one.
    #[must_use]
    pub fn with_font_context(
        mut self,
        font_context: Arc<crate::renderer::text::FontContext>,
    ) -> Self {
        self.font_context = Some(font_context);
        self
    }

    /// Build at `quality` instead of [`BuildQuality::Production`].
    #[must_use]
    pub fn with_quality(mut self, quality: BuildQuality) -> Self {
        self.quality = Some(quality);
        self
    }

    /// Carry `input` from a predecessor scene into this build.
    #[must_use]
    pub fn with_carry(mut self, input: CarryInput<'a>) -> Self {
        self.carry = Some(input);
        self
    }

    /// Reuse `asset_cache` (and make it the timeline's cache).
    #[must_use]
    pub fn with_asset_cache(mut self, asset_cache: Arc<super::assets::AssetCache>) -> Self {
        self.asset_cache = Some(asset_cache);
        self
    }

    /// Install `primitive_registry` for this build.
    #[must_use]
    pub fn with_primitive_registry(
        mut self,
        primitive_registry: Arc<crate::primitives::PrimitiveRegistry>,
    ) -> Self {
        self.primitive_registry = Some(primitive_registry);
        self
    }

    /// Install an extension context (its primitives and functions included).
    #[must_use]
    pub fn with_context(
        mut self,
        context: Arc<crate::extension_context::ExtensionContext>,
    ) -> Self {
        self.extensions = Some(context);
        self
    }

    /// Apply `--set NAME=VALUE` overrides to the build environment.
    #[must_use]
    pub fn with_defines(mut self, defines: &'a [(String, String)]) -> Self {
        self.defines = defines;
        self
    }

    /// The registry this build should install, if any.
    ///
    /// An explicit registry wins; otherwise the extension context's registry is
    /// used so extension primitives stay resolvable when only a context was set.
    pub(super) fn selected_primitive_registry(
        &self,
    ) -> Option<Arc<crate::primitives::PrimitiveRegistry>> {
        self.primitive_registry
            .clone()
            .or_else(|| self.extensions.as_ref().map(|context| context.primitive_registry()))
    }
}
