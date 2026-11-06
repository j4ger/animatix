use super::*;

impl SymbolTable {
    /// Build a symbol-aware type environment from the current table.
    ///
    /// Cached for the table's lifetime: inference queries outnumber table
    /// mutations by a wide margin (every property value in a semantic check is
    /// one query), and rebuilding the environment per query made the analyzer's
    /// per-keystroke path allocate the whole vocabulary repeatedly.
    pub fn type_env(&self) -> &typing::TypeEnv {
        self.type_env_cache.get_or_init(|| self.build_type_env())
    }

    fn build_type_env(&self) -> typing::TypeEnv {
        let mut env = typing::TypeEnv::with_stdlib();
        for (name, info) in &self.labels {
            if let Some(inferred) = &info.inferred_type {
                match inferred {
                    typing::Type::Component(component) => {
                        env.declare_component_instance(name, component);
                    },
                    typing::Type::Actor(actor_ty) => {
                        env.declare_actor(name, actor_ty);
                    },
                    _ => {
                        env.bind(name, inferred.clone());
                    },
                }
            } else if let Some(ty) = &info.ty {
                if self.components.contains_key(ty) {
                    env.declare_component_instance(name, ty);
                } else {
                    env.declare_actor(name, ty);
                }
            }
        }
        for (name, info) in &self.components {
            let signature = component_signature(
                info.params
                    .iter()
                    .map(|param| (param.name.as_str(), param.param_type.as_ref(), None)),
                &mut |_default| None,
            );
            env.register_component(name, signature);
        }
        for (alias, namespace) in &self.namespaces {
            env.register_namespace(alias, namespace_type_from_symbols(namespace));
        }
        env
    }

    /// Infer an expression type using this table's symbols.
    pub fn infer_expr_type(&self, expr: &Expr) -> typing::Type {
        typing::infer_expr_type(expr, self.type_env())
    }

    /// Infer against a fresh snapshot of the table.
    ///
    /// The declaration pass runs before the table is complete, so it must not
    /// populate (or read) the memoized environment: a `let` later in the file
    /// has to see the bindings collected before it, and caching mid-collection
    /// would freeze the environment before those bindings exist.
    pub(super) fn infer_expr_type_snapshot(&self, expr: &Expr) -> typing::Type {
        typing::infer_expr_type(expr, &self.build_type_env())
    }

    /// Resolve a type annotation against the built-in type aliases.
    pub(super) fn typed_resolve_annotation(&self, annotation: &TypeAnnotation) -> typing::Type {
        typing::TypeEnv::with_stdlib().resolve_annotation(annotation)
    }
}

fn namespace_type_from_symbols(table: &SymbolTable) -> typing::NamespaceType {
    let mut values = HashMap::new();
    for (name, info) in &table.labels {
        let ty = match &info.inferred_type {
            Some(ty) => ty.clone(),
            None => match &info.ty {
                Some(actor_ty) if table.components.contains_key(actor_ty) => {
                    typing::Type::Component(actor_ty.clone())
                },
                Some(actor_ty) => typing::Type::Actor(actor_ty.clone()),
                None => typing::Type::Any,
            },
        };
        values.insert(name.clone(), typing::NamespaceType::Value(ty));
    }
    for (alias, namespace) in &table.namespaces {
        values.insert(alias.clone(), namespace_type_from_symbols(namespace));
    }
    typing::NamespaceType::Namespace(values)
}
