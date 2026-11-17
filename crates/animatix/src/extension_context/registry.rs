use super::*;

impl ExtensionRegistry {
    /// Create a context initialized with the built-in primitive registry.
    pub fn new() -> Self {
        Self {
            // Seed the built-in primitive set so capability-based dispatch
            // (plot hosting, graph hosts, recursive containers) still resolves
            // built-ins when the context backs a CLI/GUI build.
            primitives: Arc::new(PrimitiveRegistry::new()),
            properties: PropertyRegistry::new(),
            ..Self::default()
        }
    }

    /// Register an extension primitive with its identity card.
    ///
    /// The engine trait is behaviour-only, so extensions supply their
    /// author-visible metadata (display name, category, icon, capabilities,
    /// child processing) as data at registration time — the same shape the
    /// native ABI already uses.
    pub fn register_primitive(
        &mut self,
        primitive: Arc<dyn Primitive>,
        info: animatix_std::PrimitiveInfo,
    ) -> Result<(), PrimitiveRegistrationError> {
        Arc::make_mut(&mut self.primitives).register(primitive, info)
    }

    /// Return the primitive registry snapshot for this context.
    pub fn primitive_registry(&self) -> Arc<PrimitiveRegistry> {
        Arc::clone(&self.primitives)
    }

    /// Remove a custom primitive registered by this context.
    pub fn remove_primitive(&mut self, name: &str) -> bool {
        Arc::make_mut(&mut self.primitives).remove(name)
    }

    /// Register an external property for an actor type.
    ///
    /// The context allocates a stable `PropertyId` so plugins do not have to
    /// coordinate ids across documents or hot reloads.
    pub fn register_property(
        &mut self,
        actor_type: impl Into<String>,
        name: impl Into<String>,
        kind: PropertyValueKind,
        injectable: bool,
    ) -> Result<PropertyId, PropertyRegistrationError> {
        self.properties.register(&actor_type.into(), &name.into(), kind, injectable)
    }

    /// Register an external property with full tooling metadata.
    pub fn register_property_full(
        &mut self,
        actor_type: impl Into<String>,
        name: impl Into<String>,
        kind: PropertyValueKind,
        injectable: bool,
        display_name: Option<String>,
        group: Option<String>,
        help: Option<String>,
    ) -> Result<PropertyId, PropertyRegistrationError> {
        self.properties.register_full(
            &actor_type.into(),
            &name.into(),
            kind,
            injectable,
            display_name,
            group,
            help,
        )
    }

    /// Register an external property with a precise tooling type.
    pub fn register_property_full_typed(
        &mut self,
        actor_type: impl Into<String>,
        name: impl Into<String>,
        kind: PropertyValueKind,
        ty: Option<animatix_syntax::typing::Type>,
        injectable: bool,
        display_name: Option<String>,
        group: Option<String>,
        help: Option<String>,
    ) -> Result<PropertyId, PropertyRegistrationError> {
        self.properties.register_full_typed(
            &actor_type.into(),
            &name.into(),
            kind,
            ty,
            injectable,
            display_name,
            group,
            help,
        )
    }

    /// Remove an external property by actor type and name.
    pub fn remove_property(&mut self, actor_type: &str, name: &str) -> bool {
        self.properties.remove(actor_type, name)
    }

    /// Look up an external property by actor type and name.
    pub fn property_spec(&self, actor_type: &str, name: &str) -> Option<&ExtensionPropertySpec> {
        self.properties.spec(actor_type, name)
    }

    /// Look up an external property by stable id.
    pub fn property_spec_by_id(&self, id: PropertyId) -> Option<&ExtensionPropertySpec> {
        self.properties.spec_by_id(id)
    }

    /// Return all external property descriptors in registration order.
    pub fn property_specs(&self) -> &[ExtensionPropertySpec] {
        self.properties.specs()
    }

    /// Return all built-in and extension property descriptors.
    ///
    /// Built-ins are materialized from the shared descriptor/binding tables on
    /// demand; this context owns only the extension rows.
    pub fn property_descriptors(&self) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        let mut descriptors = crate::timeline::property_registry::builtin_descriptors();
        descriptors.extend(self.properties.extension_descriptors());
        descriptors
    }

    /// Return extension-only property descriptors.
    pub fn extension_property_descriptors(
        &self,
    ) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        self.properties.extension_descriptors()
    }

    /// Register a namespaced function with one contract for tooling and execution.
    ///
    /// Timeline-producing callbacks return an inert `Value::TimelinePlan` and
    /// must not mutate a World. The ordinary call binder handles all arguments.
    pub fn register_callable<F>(
        &mut self,
        descriptor: CallableDescriptor,
        call: F,
    ) -> Result<(), ExtensionRegistrationError>
    where
        F: Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync + 'static,
    {
        descriptor.validate().map_err(ExtensionRegistrationError::InvalidContract)?;
        if descriptor.namespace.split("::").next() == Some("std") {
            return Err(ExtensionRegistrationError::InvalidContract(
                "the std namespace is reserved".into(),
            ));
        }
        let name = descriptor.qualified_name();
        if self.functions.iter().any(|(existing, _)| *existing == name) {
            return Err(ExtensionRegistrationError::Duplicate(name));
        }
        self.functions.push((name, Arc::new(call)));
        self.callable_descriptors.push(descriptor);
        Ok(())
    }

    /// Every extension callable's authoritative contract, in registration order.
    pub fn callable_descriptors(&self) -> &[CallableDescriptor] {
        &self.callable_descriptors
    }

    /// Remove a function using its canonical qualified name.
    pub fn remove_callable(&mut self, name: &str) -> bool {
        let before = self.functions.len();
        self.functions.retain(|(existing, _)| existing != name);
        self.callable_descriptors
            .retain(|descriptor| descriptor.qualified_name() != name);
        self.functions.len() != before
    }

    /// Install registered functions and their contracts into an environment.
    pub fn install_functions(&self, env: &mut Environment) {
        for ((_, call), descriptor) in self.functions.iter().zip(&self.callable_descriptors) {
            env.register_callable(descriptor.clone(), Arc::clone(call));
        }
    }

    /// Provide a typed service visible to extension handlers.
    ///
    /// The service name must be unique within this context.
    pub fn provide<T>(
        &mut self,
        name: impl Into<String>,
        service: T,
    ) -> Result<(), ExtensionRegistrationError>
    where
        T: Any + Send + Sync,
    {
        let name = name.into();
        let descriptor = animatix_syntax::schema::ServiceDescriptor {
            name: name.clone(),
            type_info: None,
            help: None,
        };
        self.provide_with_descriptor(name, service, descriptor)
    }

    /// Provide a service together with tooling metadata.
    pub fn provide_with_descriptor<T>(
        &mut self,
        name: impl Into<String>,
        service: T,
        descriptor: animatix_syntax::schema::ServiceDescriptor,
    ) -> Result<(), ExtensionRegistrationError>
    where
        T: Any + Send + Sync,
    {
        let name = name.into();
        if self.services.contains_key(&name) {
            return Err(ExtensionRegistrationError::Duplicate(name));
        }
        self.services.insert(name.clone(), Arc::new(service));
        self.service_descriptors.push(descriptor);
        Ok(())
    }

    /// Return registered service descriptors.
    pub fn service_descriptors(&self) -> Vec<animatix_syntax::schema::ServiceDescriptor> {
        self.service_descriptors.clone()
    }

    /// Remove a provided service.
    pub fn remove_service(&mut self, name: &str) -> bool {
        let removed = self.services.remove(name).is_some();
        self.service_descriptors.retain(|descriptor| descriptor.name != name);
        removed
    }

    /// Read a typed service.
    pub fn get<T>(&self, name: &str) -> Option<&T>
    where
        T: Any + Send + Sync,
    {
        self.services.get(name).and_then(|service| service.downcast_ref::<T>())
    }
}
