use super::*;

impl PropertyRegistry {
    /// Create a registry seeded with built-in property entries.
    ///
    /// Shared schema owns the descriptor (name, actor types, type, id), while
    /// `PROPERTY_REGISTRY` supplies the runtime binding (field, read source,
    /// flags, default). Guard tests keep the two sources aligned in both
    /// directions so adding a binding without a descriptor fails CI.
    pub fn new() -> Self {
        let builtins = animatix_syntax::schema::property_specs()
            .into_iter()
            .map(|spec| {
                let binding = crate::timeline::property_registry::property_schema_by_id(spec.id)
                    .map(|schema| PropertyBinding::Direct {
                        field: schema.field,
                        read_source: schema.read_source,
                        flags: schema.flags,
                        default_value: schema.default_value,
                    })
                    .unwrap_or(PropertyBinding::None);
                let injectable = matches!(
                    binding,
                    PropertyBinding::Direct { flags, .. }
                        if flags.contains(crate::timeline::property_registry::PropertyFlags::INJECTABLE)
                );
                PropertyEntry {
                    descriptor: crate::property_descriptor::from_schema(&spec, injectable),
                    binding,
                }
            })
            .collect();
        Self {
            builtins,
            extensions: Vec::new(),
            next_property_id: 1_000_000,
        }
    }

    /// Built-in property descriptors only, for timelines without extensions.
    pub fn builtin_descriptors() -> Vec<crate::property_descriptor::PropertyDescriptor> {
        static CACHE: std::sync::OnceLock<Vec<crate::property_descriptor::PropertyDescriptor>> =
            std::sync::OnceLock::new();
        CACHE
            .get_or_init(|| {
                Self::new().builtins.iter().map(|entry| entry.descriptor.clone()).collect()
            })
            .clone()
    }

    /// Register an external property for an actor type.
    pub fn register(
        &mut self,
        actor_type: &str,
        name: &str,
        kind: PropertyValueKind,
        injectable: bool,
    ) -> Result<PropertyId, PropertyRegistrationError> {
        self.register_full(actor_type, name, kind, injectable, None, None, None)
    }

    /// Register an external property with full tooling metadata.
    pub fn register_full(
        &mut self,
        actor_type: &str,
        name: &str,
        kind: PropertyValueKind,
        injectable: bool,
        display_name: Option<String>,
        group: Option<String>,
        help: Option<String>,
    ) -> Result<PropertyId, PropertyRegistrationError> {
        self.register_full_typed(
            actor_type,
            name,
            kind,
            None,
            injectable,
            display_name,
            group,
            help,
        )
    }

    /// Register an external property with a precise type annotation.
    pub fn register_full_typed(
        &mut self,
        actor_type: &str,
        name: &str,
        kind: PropertyValueKind,
        ty: Option<animatix_syntax::typing::Type>,
        injectable: bool,
        display_name: Option<String>,
        group: Option<String>,
        help: Option<String>,
    ) -> Result<PropertyId, PropertyRegistrationError> {
        if crate::timeline::property_registry::property_id(name).is_some()
            || self
                .extensions
                .iter()
                .any(|property| property.actor_type == actor_type && property.name == name)
        {
            return Err(PropertyRegistrationError::Duplicate(format!("{actor_type}.{name}")));
        }
        let id = PropertyId(self.next_property_id);
        self.next_property_id = self.next_property_id.saturating_add(1);
        self.extensions.push(ExtensionPropertySpec {
            id,
            actor_type: actor_type.to_string(),
            name: name.to_string(),
            kind,
            ty,
            injectable,
            display_name,
            group,
            help,
        });
        Ok(id)
    }

    /// Remove an external property by actor type and name.
    pub fn remove(&mut self, actor_type: &str, name: &str) -> bool {
        let before = self.extensions.len();
        self.extensions
            .retain(|property| property.actor_type != actor_type || property.name != name);
        self.extensions.len() != before
    }

    /// Look up an external property by actor type and name.
    pub fn spec(&self, actor_type: &str, name: &str) -> Option<&ExtensionPropertySpec> {
        self.extensions
            .iter()
            .find(|property| property.actor_type == actor_type && property.name == name)
    }

    /// Look up an external property by stable id.
    pub fn spec_by_id(&self, id: PropertyId) -> Option<&ExtensionPropertySpec> {
        self.extensions.iter().find(|property| property.id == id)
    }

    /// Return all external property descriptors in registration order.
    pub fn specs(&self) -> &[ExtensionPropertySpec] {
        &self.extensions
    }

    /// Return all built-in and extension descriptors in one view.
    pub fn descriptors(&self) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        let mut descriptors =
            self.builtins.iter().map(|entry| entry.descriptor.clone()).collect::<Vec<_>>();
        descriptors.extend(self.extensions.iter().map(crate::property_descriptor::from_extension));
        descriptors
    }

    /// Return extension-only descriptors in registration order.
    pub fn extension_descriptors(&self) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        self.extensions.iter().map(crate::property_descriptor::from_extension).collect()
    }
}
