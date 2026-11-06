use super::*;

impl SymbolTable {
    /// `true` when `ty` names a built-in or manifest-declared effect.
    pub fn is_effect_type(&self, ty: &str) -> bool {
        self.effect_names.contains(ty)
    }

    /// Declared type of an inline child label, if collected.
    pub fn inline_child_type(&self, label: &str) -> Option<&str> {
        self.inline_child_types.get(label).map(String::as_str)
    }

    /// Get the capability flags for an actor type (built-in or extension).
    pub fn caps_of(&self, ty: &str) -> Option<animatix_core::caps::ActorCaps> {
        animatix_std::caps_for_type(ty)
    }

    /// True when `ty` is a container or effect compositing scope.
    pub fn is_container_type(&self, ty: &str) -> bool {
        self.caps_of(ty).is_some_and(|c| c.is_container || c.is_effect_scope())
    }

    /// Retrieve author-facing documentation for this type.
    pub fn doc_of(&self, ty: &str) -> Option<&str> {
        animatix_std::catalog_lookup(ty).and_then(|info| {
            if info.doc.is_empty() {
                None
            } else {
                Some(info.doc)
            }
        })
    }
}

impl SymbolTable {
    /// Build a symbol table from one parsed AST in two passes.
    ///
    /// Pass 1 collects declarations (labels, components, scenes, aliases);
    /// pass 2 resolves the occurrences that can only be classified once every
    /// declaration exists (`Callout` targets, unused-label references). This is
    /// the entry point consumers should use; calling [`Self::build_from_ast`]
    /// and [`Self::collect_references`] by hand is the same work spelled out.
    pub fn build(stmts: &[Stmt]) -> Self {
        let mut table = Self::build_from_ast(stmts);
        table.collect_references(stmts);
        table
    }
}
