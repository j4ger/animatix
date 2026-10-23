use super::*;

impl TypeEnv {
    /// Create an empty type environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an environment seeded with the language's built-in vocabulary.
    ///
    /// The built-ins are **not** copied into this environment: they live in the
    /// `STDLIB_*` tables below and are consulted by the lookups after the
    /// environment's own maps. Seeding per environment used to insert ~60
    /// `String` keys, and a keystroke rebuild constructs one per annotation
    /// (`SymbolTable::typed_resolve_annotation`) and per inferred expression
    /// (`SymbolTable::infer_expr_type`), so the copy was a fixed cost on every
    /// analyzer update rather than a one-time one.
    pub fn with_stdlib() -> Self {
        Self::new()
    }

    /// Register a user-facing type alias, resolving it lazily so aliases may
    /// reference aliases declared later in the file.
    pub fn register_alias(&mut self, name: &str, annotation: &TypeAnnotation) {
        self.aliases.insert(name.to_string(), annotation.clone());
    }

    /// Resolve a type annotation, following named aliases.
    ///
    /// Unresolved aliases resolve to `Any` instead of failing so the gradual
    /// type checker keeps accepting otherwise valid files; callers that need a
    /// hard diagnostic should use [`Self::unresolved_alias`] on the annotation
    /// before treating the value as usable.
    pub fn resolve_annotation(&self, annotation: &TypeAnnotation) -> Type {
        self.resolve_annotation_inner(annotation, &mut HashSet::new())
            .unwrap_or(Type::Any)
    }

    /// Return the first unresolved alias path in this annotation, if any.
    pub fn unresolved_alias(&self, annotation: &TypeAnnotation) -> Option<String> {
        self.resolve_annotation_inner(annotation, &mut HashSet::new()).err()
    }

    pub(super) fn resolve_annotation_inner(
        &self,
        annotation: &TypeAnnotation,
        visiting: &mut HashSet<String>,
    ) -> Result<Type, String> {
        match annotation {
            TypeAnnotation::Alias(name) => self.resolve_alias_path(name, visiting),
            TypeAnnotation::List(inner) => {
                Ok(Type::List(Box::new(self.resolve_annotation_inner(inner, visiting)?)))
            },
            TypeAnnotation::Tuple(items) => Ok(Type::Tuple(
                items
                    .iter()
                    .map(|item| self.resolve_annotation_inner(item, visiting))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            TypeAnnotation::Function { params, ret } => Ok(Type::Function {
                params: params
                    .iter()
                    .map(|param| self.resolve_annotation_inner(param, visiting))
                    .collect::<Result<Vec<_>, _>>()?,
                ret: Box::new(self.resolve_annotation_inner(ret, visiting)?),
            }),
            TypeAnnotation::Union(types) => Ok(Type::Union(
                types
                    .iter()
                    .map(|ty| self.resolve_annotation_inner(ty, visiting))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            _ => Ok(Type::from_annotation(annotation)),
        }
    }

    pub(super) fn resolve_alias_path(
        &self,
        name: &str,
        visiting: &mut HashSet<String>,
    ) -> Result<Type, String> {
        if !visiting.insert(name.to_string()) {
            return Err(name.to_string());
        }
        let result = match self.aliases.get(name) {
            Some(annotation) => self.resolve_annotation_inner(annotation, visiting),
            None => Err(name.to_string()),
        };
        visiting.remove(name);
        result
    }

    /// Push a lexical scope.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop a lexical scope.
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Bind a local variable, parameter, or loop variable.
    pub fn bind(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        } else {
            self.builtins.insert(name.to_string(), ty);
        }
    }

    /// Declare an actor label with its primitive/component type name.
    pub fn declare_actor(&mut self, label: &str, ty: &str) {
        self.actors.insert(label.to_string(), Type::Actor(ty.to_string()));
    }

    /// Declare an array base label and its element type.
    pub fn declare_array(&mut self, label: &str, element_type: Type) {
        self.arrays.insert(label.to_string(), Type::List(Box::new(element_type)));
    }

    /// Register a component signature.
    pub fn register_component(&mut self, name: &str, signature: ComponentSignature) {
        self.components.insert(name.to_string(), signature);
    }

    /// Declare a component instance label.
    pub fn declare_component_instance(&mut self, label: &str, component_name: &str) {
        self.instances.insert(label.to_string(), component_name.to_string());
    }

    /// Look up a bare identifier: local scope, actor, component instance,
    /// array base, then builtin constant.
    pub fn lookup_ident(&self, name: &str) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        if let Some(ty) = self.actors.get(name) {
            return Some(ty.clone());
        }
        if let Some(component) = self.instances.get(name) {
            return Some(Type::Component(component.clone()));
        }
        if let Some(ty) = self.arrays.get(name) {
            return Some(ty.clone());
        }
        if let Some(ty) = self.builtins.get(name) {
            return Some(ty.clone());
        }
        STDLIB_BUILTINS.get(name).cloned()
    }

    /// Register a concrete value in an aliased namespace.
    pub fn register_namespace_value(&mut self, alias: &str, name: &str, ty: Type) {
        let entry = self
            .namespaces
            .entry(alias.to_string())
            .or_insert_with(|| NamespaceType::Namespace(HashMap::new()));
        if let NamespaceType::Namespace(values) = entry {
            values.insert(name.to_string(), NamespaceType::Value(ty));
        }
    }

    /// Register a nested namespace value.
    pub fn register_namespace(&mut self, alias: &str, namespace: NamespaceType) {
        self.namespaces.insert(alias.to_string(), namespace);
    }

    /// Look up a namespaced path such as `deck.bar` or `accent.primary`.
    pub(super) fn lookup_path(&self, parts: &[String]) -> Option<Type> {
        if parts.len() >= 2 && COLOR_NAMESPACES.contains(&parts[0].as_str()) {
            return Some(Type::Color);
        }
        // The `scene` pseudo-object exposes layout facts to expressions.
        if parts.first().map(String::as_str) == Some("scene") {
            return match parts.get(1).map(String::as_str) {
                Some("center") | Some("dimensions") | Some("top") | Some("bottom")
                | Some("left") | Some("right") => {
                    if parts.len() > 2 && matches!(parts[2].as_str(), "x" | "y") {
                        Some(Type::Num)
                    } else {
                        Some(Type::Vec2)
                    }
                },
                Some("width") | Some("height") => Some(Type::Num),
                Some("background") => Some(Type::Color),
                Some("stats") => Some(Type::List(Box::new(Type::Num))),
                _ => None,
            };
        }
        let first = parts.first()?;
        if let Some(namespace) = self.namespaces.get(first) {
            let mut current = namespace;
            for part in &parts[1..parts.len() - 1] {
                match current {
                    NamespaceType::Namespace(values) => current = values.get(part)?,
                    NamespaceType::Value(_) => return None,
                }
            }
            if let Some(last) = parts.last() {
                match current {
                    NamespaceType::Namespace(values) => {
                        if let Some(NamespaceType::Value(ty)) = values.get(last) {
                            return Some(ty.clone());
                        }
                    },
                    NamespaceType::Value(ty) if parts.len() == 2 => {
                        return Some(ty.clone());
                    },
                    NamespaceType::Value(_) => {},
                }
            }
        }
        let base = self.lookup_ident(first)?;
        if parts.len() == 1 {
            return Some(base);
        }
        match base {
            Type::Actor(actor_ty) => {
                self.lookup_type_path(Type::Actor(actor_ty), first, &parts[1..])
            },
            Type::Component(component) => {
                self.lookup_type_path(Type::Component(component), first, &parts[1..])
            },
            _ => None,
        }
    }

    pub(super) fn lookup_type_path(
        &self,
        receiver: Type,
        base_name: &str,
        parts: &[String],
    ) -> Option<Type> {
        let Some(first) = parts.first() else {
            return Some(receiver);
        };
        match receiver {
            Type::Actor(actor_ty) => {
                let ty = property_type(&actor_ty, first)?;
                self.lookup_type_path(ty, first, &parts[1..])
            },
            Type::Component(component) => {
                let sig = self.components.get(&component)?;
                let ty = sig
                    .params
                    .get(first)
                    .cloned()
                    .or_else(|| self.arrays.get(&format!("{base_name}.{first}")).cloned())?;
                self.lookup_type_path(ty, first, &parts[1..])
            },
            _ => None,
        }
    }

    /// Resolve an actor/component property by name.
    pub(super) fn lookup_property(&self, receiver: &Type, name: &str) -> Option<Type> {
        match receiver {
            Type::Actor(actor_ty) => property_type(actor_ty, name),
            Type::Component(component) => {
                let sig = self.components.get(component)?;
                sig.params.get(name).cloned()
            },
            _ => None,
        }
    }

    /// Look up a function return type.
    pub fn function_type(&self, name: &str) -> Option<Type> {
        if let Some(ty) = self.functions.get(name) {
            return Some(ty.clone());
        }
        STDLIB_FUNCTIONS.get(name).cloned()
    }

    /// Look up a construct type.
    pub fn construct_type(&self, name: &str) -> Option<Type> {
        if let Some(ty) = self.construct_types.get(name) {
            return Some(ty.clone());
        }
        STDLIB_CONSTRUCTS.get(name).cloned()
    }
}

/// Accepted source forms for `transform`: 2/4/6-element tuples.
pub(crate) fn transform_type() -> Type {
    Type::Union(vec![
        Type::Vec2,
        Type::Vec4,
        Type::Tuple(vec![
            Type::Num,
            Type::Num,
            Type::Num,
            Type::Num,
            Type::Num,
            Type::Num,
        ]),
    ])
}
