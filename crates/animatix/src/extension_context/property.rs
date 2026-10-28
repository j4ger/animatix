use super::*;

impl PropertyRegistry {
    /// Create an empty extension property registry.
    ///
    /// Built-in properties are not seeded here: their names, applicability, and
    /// value kind live in `animatix_core::property::PROPERTY_DESCRIPTORS`, and
    /// their runtime binding lives in
    /// `crate::timeline::property_registry::PROPERTY_REGISTRY`. Materializing a
    /// second built-in table in every context only created a copy that could
    /// drift from both sources.
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
            next_property_id: 1_000_000,
        }
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

    /// Return extension-only descriptors in registration order.
    pub fn extension_descriptors(&self) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        self.extensions.iter().map(crate::property_descriptor::from_extension).collect()
    }
}
