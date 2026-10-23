use super::*;

impl SymbolTable {
    /// Build a symbol table for one parsed AST.
    pub fn build_from_ast(stmts: &[Stmt]) -> Self {
        let mut table = Self {
            types: crate::builtins::types().iter().cloned().collect(),
            keywords: crate::builtins::KEYWORDS.iter().map(|s| s.to_string()).collect(),
            actions: crate::builtins::ACTIONS.iter().map(|s| s.to_string()).collect(),
            properties: typing::known_properties().clone(),
            property_types: typing::known_property_types().clone(),
            scenes: HashMap::new(),
            ..Default::default()
        };

        // Effects are declared like actors but are not primitives; register their
        // type names and declared parameters so property diagnostics and
        // completion treat `soft: Blur, radius: 10` as valid.
        for spec in crate::schema::effect_specs() {
            table.types.insert(spec.type_name.to_string());
            table.effect_names.insert(spec.type_name.to_string());
            let mut names: Vec<String> =
                spec.params.iter().map(|param| param.name.to_string()).collect();
            // Every effect has the implicit `enabled` toggle (runtime default
            // `true`); seed it so `soft.enabled = true` validates.
            names.push("enabled".to_string());
            table.properties.insert(spec.type_name.to_string(), names.clone());
            for param in &spec.params {
                if let Some(ty) = effect_param_type(param.kind) {
                    table
                        .property_types
                        .insert((spec.type_name.to_string(), param.name.to_string()), ty);
                }
            }
            table
                .property_types
                .insert((spec.type_name.to_string(), "enabled".to_string()), typing::Type::Bool);
        }

        for stmt in stmts {
            table.collect_stmt(stmt);
        }

        table
    }

    fn collect_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::LetDecl {
                is_pub,
                name,
                value,
                span,
                ..
            } => {
                let inferred = self.infer_expr_type(value);
                self.labels.insert(
                    name.clone(),
                    LabelInfo {
                        name: name.clone(),
                        kind: LabelKind::Let,
                        line: 0, // populated by Analyzer::enrich_positions from the token stream
                        col: 0,  // populated by Analyzer::enrich_positions from the token stream
                        span: *span,
                        ty: None,
                        inferred_type: Some(inferred),
                        is_pub: *is_pub,
                        plot_params: Vec::new(),
                    },
                );
            },

            Stmt::ActorDecl {
                is_pub,
                label,
                array_index,
                ty,
                span,
                children,
                props,
                ..
            } => {
                let plot_params = crate::semantic_diagnostics::plot_runtime_params(ty, props);
                self.labels.insert(
                    label.clone(),
                    LabelInfo {
                        name: label.clone(),
                        kind: LabelKind::Actor,
                        line: 0, // populated by Analyzer::enrich_positions from the token stream
                        col: 0,  // populated by Analyzer::enrich_positions from the token stream
                        span: *span,
                        ty: Some(ty.clone()),
                        inferred_type: Some(if self.components.contains_key(ty) {
                            typing::Type::Component(ty.clone())
                        } else {
                            typing::Type::Actor(ty.clone())
                        }),
                        is_pub: *is_pub,
                        plot_params,
                    },
                );
                if array_index.is_some() {
                    self.array_labels.insert(label.clone());
                }
                for child in children {
                    self.collect_inline_item(child);
                }
            },

            Stmt::ComponentDef(def, span) => {
                self.types.insert(def.name.clone());
                collect_component_internal_labels(&def.body, &mut self.component_internal_labels);
                self.components.insert(
                    def.name.clone(),
                    ComponentInfo {
                        name: def.name.clone(),
                        params: def
                            .params
                            .iter()
                            .map(|p| ParamInfo {
                                name: p.name.clone(),
                                param_type: p.param_type.clone(),
                                default: p.default.as_ref().map(|e| e.to_source()),
                            })
                            .collect(),
                        line: 0, // populated by Analyzer::enrich_positions from the token stream
                        col: 0,  // populated by Analyzer::enrich_positions from the token stream
                        span: *span,
                        is_pub: def.is_pub,
                    },
                );

                // Recurse into component body
                for stmt in &def.body {
                    self.collect_stmt(stmt);
                }
            },

            Stmt::FnDecl {
                name, params, body, ..
            } => {
                self.actions.insert(name.clone());
                // Function parameters and `self` are valid labels inside the
                // function body.
                for param in params {
                    let param_ty = param
                        .param_type
                        .as_ref()
                        .map(|annotation| self.typed_resolve_annotation(annotation))
                        .or_else(|| {
                            param.default.as_ref().map(|value| {
                                typing::infer_expr_type(value, &typing::TypeEnv::with_stdlib())
                            })
                        });
                    self.labels.insert(
                        param.name.clone(),
                        LabelInfo {
                            name: param.name.clone(),
                            kind: LabelKind::For,
                            line: 0,
                            col: 0,
                            span: None,
                            ty: None,
                            inferred_type: param_ty,
                            is_pub: false,
                            plot_params: Vec::new(),
                        },
                    );
                }
                // `self` is implicitly available inside timeline functions
                // when invoked target-style.
                self.labels.insert(
                    "self".to_string(),
                    LabelInfo {
                        name: "self".to_string(),
                        kind: LabelKind::Actor,
                        line: 0,
                        col: 0,
                        span: None,
                        ty: None,
                        inferred_type: None,
                        is_pub: false,
                        plot_params: Vec::new(),
                    },
                );
                for stmt in body {
                    self.collect_stmt(stmt);
                }
            },

            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                span,
                ..
            } => {
                let var_names: Vec<String> = match var {
                    LoopPattern::Single(name) => vec![name.clone()],
                    LoopPattern::Tuple(names) => names.clone(),
                };
                // Infer loop-variable types from the iterable so destructured
                // variables resolve (e.g. `for (x, y), i in {(900,320), ...}`).
                let iterable_ty = self.infer_expr_type(iterable);
                let element_types: Vec<typing::Type> = match &iterable_ty {
                    typing::Type::List(element) => match element.as_ref() {
                        typing::Type::Vec2
                        | typing::Type::Vec3
                        | typing::Type::Vec4
                        | typing::Type::Color => {
                            vec![typing::Type::Num; var_names.len()]
                        },
                        typing::Type::Tuple(types) => types.clone(),
                        other => vec![(*other).clone(); var_names.len()],
                    },
                    _ => vec![typing::Type::Any; var_names.len()],
                };
                for (index, name) in var_names.iter().enumerate() {
                    self.labels.insert(
                        name.clone(),
                        LabelInfo {
                            name: name.clone(),
                            kind: LabelKind::For,
                            line: 0, /* populated by Analyzer::enrich_positions from the token
                                      * stream */
                            col: 0, // populated by Analyzer::enrich_positions from the token stream
                            span: *span,
                            ty: None,
                            inferred_type: element_types.get(index).cloned(),
                            is_pub: false,
                            plot_params: Vec::new(),
                        },
                    );
                }

                if let Some(iv) = index_var {
                    self.labels.insert(
                        iv.clone(),
                        LabelInfo {
                            name: iv.clone(),
                            kind: LabelKind::For,
                            line: 0, /* populated by Analyzer::enrich_positions from the token
                                      * stream */
                            col: 0, // populated by Analyzer::enrich_positions from the token stream
                            span: *span,
                            ty: None,
                            inferred_type: None,
                            is_pub: false,
                            plot_params: Vec::new(),
                        },
                    );
                }

                for stmt in body {
                    self.collect_stmt(stmt);
                }
            },

            // Recurse into blocks
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                for stmt in body {
                    self.collect_stmt(stmt);
                }
            },
            Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. } => {
                for stmt in body {
                    self.collect_stmt(stmt);
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                for stmt in then_branch {
                    self.collect_stmt(stmt);
                }
                if let Some(else_stmts) = else_branch {
                    for stmt in else_stmts {
                        self.collect_stmt(stmt);
                    }
                }
            },
            Stmt::Match { arms, .. } => {
                for (_, body) in arms {
                    for stmt in body {
                        self.collect_stmt(stmt);
                    }
                }
            },

            Stmt::Scene { name, span, .. } => {
                self.scenes.insert(
                    name.clone(),
                    SceneInfo {
                        name: name.clone(),
                        line: 0, // populated by Analyzer::enrich_positions from the token stream
                        col: 0,  // populated by Analyzer::enrich_positions from the token stream
                        span: *span,
                    },
                );
                // Recurse into scene body
                if let Stmt::Scene { body, .. } = stmt {
                    for s in body {
                        self.collect_stmt(s);
                    }
                }
            },

            Stmt::Play { .. } => {
                // No new symbols to declare, but we could track play references here.
            },

            Stmt::Import {
                path, alias, span, ..
            } => {
                self.imports.push(ImportInfo {
                    path: path.clone(),
                    alias: alias.clone(),
                    span: *span,
                });
            },

            Stmt::TypeAlias {
                is_pub: true,
                name,
                annotation,
                ..
            } => {
                self.type_aliases.insert(name.clone(), annotation.clone());
            },

            // Actions, assignments, etc. — no symbols to extract
            _ => {},
        }
    }
}
