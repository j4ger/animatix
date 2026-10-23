use super::*;

impl Timeline {
    /// Build a `Timeline` from an AST with the default font context and
    /// production quality.
    pub fn build(ast: &[Stmt]) -> Self {
        Self::build_with_font_context(
            ast,
            std::sync::Arc::new(crate::renderer::text::FontContext::new()),
        )
    }

    /// Build a `Timeline` from an AST with a custom font context.
    pub fn build_with_font_context(
        ast: &[Stmt],
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    ) -> Self {
        Self::build_with_diagnostics_and_font_context(
            ast,
            &std::collections::HashMap::new(),
            font_context,
            super::BuildQuality::Production,
        )
        .output
    }

    /// Build a `Timeline` from an AST, collecting diagnostics and using the
    /// default font context.
    #[instrument(skip(ast, namespaces), fields(ast_statements = ast.len()))]
    pub fn build_with_diagnostics(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
    ) -> BuildReport<Self> {
        Self::build_with_diagnostics_and_font_context(
            ast,
            namespaces,
            std::sync::Arc::new(crate::renderer::text::FontContext::new()),
            super::BuildQuality::Production,
        )
    }

    /// Build a `Timeline` from an AST with `--set NAME=VALUE` overrides applied
    /// to the build environment.
    pub fn build_with_diagnostics_and_defines(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        defines: &[(String, String)],
    ) -> BuildReport<Self> {
        Self::build_impl(
            ast,
            namespaces,
            std::sync::Arc::new(crate::renderer::text::FontContext::new()),
            super::BuildQuality::Production,
            None,
            None,
            None,
            None,
            defines,
        )
    }

    /// Build a `Timeline` from an AST with the default font context and an
    /// existing asset cache carried from a previous build.
    pub fn build_with_diagnostics_and_asset_cache(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        asset_cache: Option<std::sync::Arc<super::assets::AssetCache>>,
    ) -> BuildReport<Self> {
        Self::build_with_diagnostics_and_font_context_and_asset_cache(
            ast,
            namespaces,
            std::sync::Arc::new(crate::renderer::text::FontContext::new()),
            super::BuildQuality::Production,
            asset_cache,
        )
    }

    /// Build a `Timeline` from an AST with full control over diagnostics,
    /// font context, build quality, and the asset cache carried into the build.
    #[instrument(skip(ast, namespaces, font_context), fields(ast_statements = ast.len()))]
    pub fn build_with_diagnostics_and_font_context(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
    ) -> BuildReport<Self> {
        Self::build_with_diagnostics_and_font_context_and_asset_cache(
            ast,
            namespaces,
            font_context,
            build_quality,
            None,
        )
    }

    /// Build a `Timeline` from an AST with a primitive registry.
    pub fn build_with_primitive_registry(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        primitive_registry: std::sync::Arc<crate::primitives::PrimitiveRegistry>,
    ) -> BuildReport<Self> {
        Self::build_with_diagnostics_and_font_context_and_asset_cache_and_primitive_registry(
            ast,
            namespaces,
            std::sync::Arc::new(crate::renderer::text::FontContext::new()),
            super::BuildQuality::Production,
            None,
            primitive_registry,
        )
    }

    /// Build a `Timeline` with a cordis-inspired extension context.
    pub fn build_with_context(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        context: std::sync::Arc<crate::extension_context::ExtensionContext>,
    ) -> BuildReport<Self> {
        Self::build_with_diagnostics_and_font_context_and_asset_cache_and_extension_context(
            ast,
            namespaces,
            std::sync::Arc::new(crate::renderer::text::FontContext::new()),
            super::BuildQuality::Production,
            None,
            context,
        )
    }

    /// Build a `Timeline` from an AST with full control over diagnostics,
    /// font context, build quality, and an existing asset cache.
    #[instrument(skip(ast, namespaces, font_context, asset_cache), fields(ast_statements = ast.len()))]
    pub fn build_with_diagnostics_and_font_context_and_asset_cache(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
        asset_cache: Option<std::sync::Arc<super::assets::AssetCache>>,
    ) -> BuildReport<Self> {
        Self::build_with_diagnostics_and_font_context_and_asset_cache_and_primitive_registry(
            ast,
            namespaces,
            font_context,
            build_quality,
            asset_cache,
            std::sync::Arc::new(crate::primitives::PrimitiveRegistry::new()),
        )
    }

    /// Build a `Timeline` from an AST with full control over diagnostics,
    /// font context, build quality, asset cache, and primitive registry.
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        skip(ast, namespaces, font_context, asset_cache, primitive_registry),
        fields(ast_statements = ast.len())
    )]
    pub fn build_with_diagnostics_and_font_context_and_asset_cache_and_primitive_registry(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
        asset_cache: Option<std::sync::Arc<super::assets::AssetCache>>,
        primitive_registry: std::sync::Arc<crate::primitives::PrimitiveRegistry>,
    ) -> BuildReport<Self> {
        Self::build_impl(
            ast,
            namespaces,
            font_context,
            build_quality,
            None,
            asset_cache,
            Some(primitive_registry),
            None,
            &[],
        )
    }

    /// Build a `Timeline` from an AST with an extension context.
    #[allow(clippy::too_many_arguments)]
    pub fn build_with_diagnostics_and_font_context_and_asset_cache_and_extension_context(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
        asset_cache: Option<std::sync::Arc<super::assets::AssetCache>>,
        context: std::sync::Arc<crate::extension_context::ExtensionContext>,
    ) -> BuildReport<Self> {
        let primitive_registry = context.primitive_registry();
        Self::build_impl(
            ast,
            namespaces,
            font_context,
            build_quality,
            None,
            asset_cache,
            Some(primitive_registry),
            Some(context),
            &[],
        )
    }

    /// Build a `Timeline` with a pre-seeded carry bag injected before
    /// statement processing.
    ///
    /// Used by the multi-scene composition engine to thread actor state
    /// from a predecessor scene into the current scene.
    ///
    /// `carry` — optional carry bag from the predecessor scene.  When `None`
    /// this is identical to `build_with_diagnostics_and_font_context`.
    ///
    /// `source_timeline` — the predecessor timeline, used to resolve
    /// layout-managed world positions (Phase 3 re-rooting).
    ///
    /// `source_duration_ms` — duration of the predecessor scene in ms.
    ///
    /// `dims` — scene pixel dimensions `[width, height]`.
    pub fn build_with_carry(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
        carry: Option<&crate::timeline::persistence::CarryBag>,
        source_timeline: Option<&Timeline>,
        source_duration_ms: u64,
        dims: [f64; 2],
    ) -> BuildReport<Self> {
        Self::build_with_carry_and_asset_cache(
            ast,
            namespaces,
            font_context,
            build_quality,
            carry,
            source_timeline,
            source_duration_ms,
            dims,
            None,
        )
    }

    /// Carry-aware scene build that also accepts an existing asset cache.
    pub fn build_with_carry_and_asset_cache(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
        carry: Option<&crate::timeline::persistence::CarryBag>,
        source_timeline: Option<&Timeline>,
        source_duration_ms: u64,
        dims: [f64; 2],
        asset_cache: Option<std::sync::Arc<super::assets::AssetCache>>,
    ) -> BuildReport<Self> {
        let carry_params =
            carry.zip(source_timeline).map(|(c, s)| (c, s, source_duration_ms, dims));
        Self::build_impl(
            ast,
            namespaces,
            font_context,
            build_quality,
            carry_params,
            asset_cache,
            None,
            None,
            &[],
        )
    }

    /// Carry-aware scene build that also accepts an extension context.
    #[allow(clippy::too_many_arguments)]
    pub fn build_with_carry_and_asset_cache_and_extension_context(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
        carry: Option<&crate::timeline::persistence::CarryBag>,
        source_timeline: Option<&Timeline>,
        source_duration_ms: u64,
        dims: [f64; 2],
        asset_cache: Option<std::sync::Arc<super::assets::AssetCache>>,
        context: std::sync::Arc<crate::extension_context::ExtensionContext>,
    ) -> BuildReport<Self> {
        let carry_params =
            carry.zip(source_timeline).map(|(c, s)| (c, s, source_duration_ms, dims));
        let primitive_registry = context.primitive_registry();
        Self::build_impl(
            ast,
            namespaces,
            font_context,
            build_quality,
            carry_params,
            asset_cache,
            Some(primitive_registry),
            Some(context),
            &[],
        )
    }
}
