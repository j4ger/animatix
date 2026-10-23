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
    pub fn property_descriptors(&self) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        self.properties.descriptors()
    }

    /// Return extension-only property descriptors.
    pub fn extension_property_descriptors(
        &self,
    ) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        self.properties.extension_descriptors()
    }

    /// Register a custom action handler.
    ///
    /// The action name must be unique within this context.
    pub fn register_action(
        &mut self,
        action: Box<dyn BuiltinAction>,
    ) -> Result<(), ExtensionRegistrationError> {
        let name = action.signature().name.clone();
        if self.actions.iter().any(|existing| existing.signature().name == name) {
            return Err(ExtensionRegistrationError::Duplicate(name));
        }
        self.actions.push(action);
        Ok(())
    }

    /// Remove a custom action by name.
    pub fn remove_action(&mut self, name: &str) -> bool {
        let before = self.actions.len();
        self.actions.retain(|action| action.signature().name != name);
        self.actions.len() != before
    }

    /// Look up a custom action by name.
    pub fn action(&self, name: &str) -> Option<&dyn BuiltinAction> {
        self.actions
            .iter()
            .find(|action| action.signature().name == name)
            .map(|action| action.as_ref())
    }

    /// Return signatures for custom actions.
    pub fn action_signatures(&self) -> Vec<ActionSignature> {
        self.actions.iter().map(|action| action.signature()).collect()
    }

    /// Register a native expression function.
    ///
    /// The function name must be unique within this context.
    pub fn register_function<F>(
        &mut self,
        name: impl Into<String>,
        call: F,
    ) -> Result<(), ExtensionRegistrationError>
    where
        F: Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync + 'static,
    {
        let name = name.into();
        let descriptor = animatix_syntax::schema::FunctionDescriptor {
            name: name.clone(),
            params: Vec::new(),
            return_type: None,
            help: None,
        };
        self.register_function_with_descriptor(name, call, descriptor)
    }

    /// Register a function together with tooling metadata.
    pub fn register_function_with_descriptor<F>(
        &mut self,
        name: impl Into<String>,
        call: F,
        descriptor: animatix_syntax::schema::FunctionDescriptor,
    ) -> Result<(), ExtensionRegistrationError>
    where
        F: Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync + 'static,
    {
        let name = name.into();
        if self.functions.iter().any(|(existing, _)| *existing == name) {
            return Err(ExtensionRegistrationError::Duplicate(name));
        }
        self.functions.push((name.clone(), Arc::new(call)));
        self.function_descriptors.push(descriptor);
        Ok(())
    }

    /// Return registered function descriptors.
    pub fn function_descriptors(&self) -> Vec<animatix_syntax::schema::FunctionDescriptor> {
        self.function_descriptors.clone()
    }

    /// Remove a registered expression function.
    pub fn remove_function(&mut self, name: &str) -> bool {
        let before = self.functions.len();
        self.functions.retain(|(existing, _)| existing != name);
        self.function_descriptors.retain(|descriptor| descriptor.name != name);
        self.functions.len() != before
    }

    /// Install registered functions into an environment.
    pub fn install_functions(&self, env: &mut Environment) {
        for (name, call) in &self.functions {
            env.set(name, Value::NativeFn(Arc::clone(call)));
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
