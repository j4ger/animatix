use super::*;

impl SymbolTable {
    /// Merge symbols from another table into this one.
    pub fn merge(&mut self, other: &SymbolTable) {
        for (name, info) in &other.labels {
            if !self.labels.contains_key(name) {
                self.labels.insert(name.clone(), info.clone());
            }
        }
        for (name, info) in &other.components {
            // Cross-file component visibility requires `pub`, including for
            // direct imports. ModuleGraph applies the same rule when building
            // its component registry.
            if info.is_pub && !self.components.contains_key(name) {
                self.components.insert(name.clone(), info.clone());
            }
        }
        for (name, info) in &other.scenes {
            if !self.scenes.contains_key(name) {
                self.scenes.insert(name.clone(), info.clone());
            }
        }
        for (name, annotation) in &other.type_aliases {
            self.type_aliases.entry(name.clone()).or_insert_with(|| annotation.clone());
        }
        for (name, props) in &other.properties {
            let entry = self.properties.entry(name.clone()).or_default();
            for prop in props {
                if !entry.contains(prop) {
                    entry.push(prop.clone());
                }
            }
        }
        // Union known action names (timeline fns defined in the imported
        // file) so semantic checks accept invocations of component fns
        // across file boundaries.
        for name in &other.actions {
            self.actions.insert(name.clone());
        }
        // The unused-label pass reads three sets besides `labels`, and a
        // direct import flattens `labels` without them, which made every
        // private name in an imported library look unused *in the importing
        // file* — `check examples/animation/36_light_pack.amx` reported
        // `Unused binding: 'p'` for a `let` inside `Ticker`'s `always` block
        // that the very next line reads, at the importer's 1:1. References
        // belong to the table that declares the name, and the component/array
        // exemptions are not scene actors in either file, so all three travel
        // with the labels they describe.
        for name in &other.referenced_labels {
            self.referenced_labels.insert(name.clone());
        }
        for name in &other.component_internal_labels {
            self.component_internal_labels.insert(name.clone());
        }
        for name in &other.array_labels {
            self.array_labels.insert(name.clone());
        }
    }

    /// Return a table containing only exports visible through an aliased
    /// import: `pub let` values, `pub component` definitions, `pub type`
    /// aliases, scenes, and nested namespaces.
    pub fn exported_namespace(&self) -> SymbolTable {
        SymbolTable {
            labels: self
                .labels
                .iter()
                .filter(|(_, info)| info.is_pub && matches!(info.kind, LabelKind::Let))
                .map(|(name, info)| (name.clone(), info.clone()))
                .collect(),
            components: self
                .components
                .iter()
                .filter(|(_, info)| info.is_pub)
                .map(|(name, info)| (name.clone(), info.clone()))
                .collect(),
            type_aliases: self.type_aliases.clone(),
            scenes: self.scenes.clone(),
            namespaces: self.namespaces.clone(),
            ..Default::default()
        }
    }

    /// Resolve a namespace table by qualified path (e.g., "lib.inner").
    pub fn namespace_table(&self, qualified_name: &str) -> Option<&SymbolTable> {
        let Some((namespace, rest)) = qualified_name.split_once('.') else {
            return self.namespaces.get(qualified_name);
        };
        let ns = self.namespaces.get(namespace)?;
        ns.namespace_table(rest)
    }

    /// Look up a label by namespace-qualified name (e.g., "lib.inner.value").
    /// Returns the label info if found in the specified namespace.
    pub fn resolve_namespaced_label(&self, qualified_name: &str) -> Option<&LabelInfo> {
        let (namespace, rest) = qualified_name.split_once('.')?;
        let ns = self.namespaces.get(namespace)?;
        if rest.contains('.') {
            ns.resolve_namespaced_label(rest)
        } else {
            ns.labels.get(rest)
        }
    }

    /// Look up a component by namespace-qualified name (e.g., "lib.inner.MyComponent").
    pub fn resolve_namespaced_component(&self, qualified_name: &str) -> Option<&ComponentInfo> {
        let (namespace, rest) = qualified_name.split_once('.')?;
        let ns = self.namespaces.get(namespace)?;
        if rest.contains('.') {
            ns.resolve_namespaced_component(rest)
        } else {
            ns.components.get(rest)
        }
    }

    /// Get all labels in a specific namespace (for completions).
    pub fn namespace_labels(&self, namespace: &str) -> Vec<&str> {
        self.namespace_table(namespace)
            .map(|ns| ns.labels.keys().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get all components in a specific namespace (for completions).
    pub fn namespace_components(&self, namespace: &str) -> Vec<&str> {
        self.namespace_table(namespace)
            .map(|ns| ns.components.keys().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}
