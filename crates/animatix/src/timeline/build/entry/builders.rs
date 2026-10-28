//! Public timeline build entry points.
//!
//! There are four: one implementation ([`Timeline::build_with_options`]) and
//! three conveniences that exist because callers overwhelmingly want exactly
//! one of these shapes. Every convenience delegates here — none of them
//! duplicates build setup.

use super::*;

impl Timeline {
    /// Build a `Timeline` from an AST with the default font context and
    /// production quality.
    pub fn build(ast: &[Stmt]) -> Self {
        Self::build_with_options(ast, &std::collections::HashMap::new(), BuildOptions::default())
            .output
    }

    /// Build a `Timeline` from an AST, collecting diagnostics and using the
    /// default font context.
    pub fn build_with_diagnostics(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
    ) -> BuildReport<Self> {
        Self::build_with_options(ast, namespaces, BuildOptions::default())
    }

    /// Build a `Timeline` with a cordis-inspired extension context.
    ///
    /// The context's primitives and expression functions are installed for the
    /// build; use [`Self::build_with_options`] when quality, an asset cache or
    /// `--set` overrides are also involved.
    pub fn build_with_context(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        context: std::sync::Arc<crate::extension_context::ExtensionContext>,
    ) -> BuildReport<Self> {
        Self::build_with_options(ast, namespaces, BuildOptions::default().with_context(context))
    }

    /// Build a `Timeline` with explicit [`BuildOptions`].
    ///
    /// The single lowering entry point: font context, quality, carry, asset
    /// cache, primitive registry, extension context and `--set` overrides all
    /// travel in `options`.
    #[instrument(skip(ast, namespaces, options), fields(ast_statements = ast.len()))]
    pub fn build_with_options(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        options: BuildOptions<'_>,
    ) -> BuildReport<Self> {
        Self::build_impl(ast, namespaces, options)
    }
}
