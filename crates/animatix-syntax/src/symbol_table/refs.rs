use super::*;

impl SymbolTable {
    /// Collect every label reference in the AST.
    pub fn collect_references(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.collect_refs_from_stmt(stmt);
        }
    }

    fn collect_refs_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ActorDecl {
                ty,
                props,
                children,
                ..
            } => {
                for prop in props {
                    if ty == "Callout" && prop.name == "target" {
                        self.collect_callout_target_ref(&prop.value);
                    } else {
                        self.collect_refs_from_expr(&prop.value);
                    }
                }
                for child in children {
                    self.collect_inline_item(child);
                }
            },
            Stmt::FnDecl { body, .. } | Stmt::Block { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::Expr(expr, ..) => {
                self.collect_refs_from_expr(expr);
            },
            Stmt::Action(action, ..) => {
                for (target, index) in action.targets.iter().zip(action.target_index.iter()) {
                    if let Some(expr) = index {
                        // Indexed target (`row.b[j]`): reference the leaf base
                        // plus the index expression's own references.
                        let base = target.rsplit('.').next().unwrap_or(target);
                        self.referenced_labels.insert(base.to_string());
                        self.collect_refs_from_expr(expr);
                    } else if target.contains('.') {
                        // Dotted target (`eq.lhs`): reference the root (checked
                        // for undefined-label) and the leaf (the actual actor).
                        let root = target.split('.').next().unwrap_or(target);
                        let leaf = target.rsplit('.').next().unwrap_or(target);
                        self.referenced_labels.insert(root.to_string());
                        self.referenced_labels.insert(leaf.to_string());
                    } else {
                        self.referenced_labels.insert(target.clone());
                    }
                }
                for arg in &action.args {
                    self.collect_refs_from_expr(arg);
                }
            },
            Stmt::Assignment {
                target,
                property,
                value,
                ..
            } => {
                for seg in target {
                    match seg {
                        TargetSegment::Static(label) => {
                            self.referenced_labels.insert(label.clone());
                        },
                        TargetSegment::Indexed { base, index } => {
                            self.referenced_labels.insert(base.clone());
                            self.collect_refs_from_expr(index);
                        },
                    }
                }
                if property == "target" && self.target_is_callout(target) {
                    self.collect_callout_target_ref(value);
                }
                // The RHS may reference bindings (e.g. `cx` inside an always
                // block), so collect its references too.
                self.collect_refs_from_expr(value);
            },
            Stmt::Play { scene_name, .. } => {
                self.referenced_labels.insert(scene_name.clone());
            },
            // Recurse into blocks
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_refs_from_expr(condition);
                for stmt in then_branch {
                    self.collect_refs_from_stmt(stmt);
                }
                if let Some(else_stmts) = else_branch {
                    for stmt in else_stmts {
                        self.collect_refs_from_stmt(stmt);
                    }
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                self.collect_refs_from_expr(scrutinee);
                for (_, body) in arms {
                    for stmt in body {
                        self.collect_refs_from_stmt(stmt);
                    }
                }
            },
            Stmt::ForLoop { iterable, body, .. } => {
                self.collect_refs_from_expr(iterable);
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::LetDecl { value, .. } => {
                self.collect_refs_from_expr(value);
            },
            Stmt::ComponentDef(def, ..) => {
                for stmt in &def.body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::Scene { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                ..
            } => {
                for seg in target {
                    self.referenced_labels.insert(seg.label_str().to_string());
                    if let TargetSegment::Indexed { index, .. } = seg {
                        self.collect_refs_from_expr(index);
                    }
                }
                if property == "target" && self.target_is_callout(target) {
                    self.collect_callout_target_ref(value);
                }
            },
            _ => {},
        }
    }

    fn target_is_callout(&self, target: &[TargetSegment]) -> bool {
        target.first().is_some_and(|seg| {
            self.labels
                .get(seg.label_str())
                .is_some_and(|info| info.ty.as_deref() == Some("Callout"))
        })
    }

    fn collect_callout_target_ref(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(name) => {
                self.referenced_labels.insert(name.clone());
            },
            Expr::Path(parts) => {
                if let Some(first) = parts.first() {
                    self.referenced_labels.insert(first.clone());
                }
            },
            Expr::Str(name) => {
                self.referenced_labels.insert(name.clone());
            },
            _ => self.collect_refs_from_expr(expr),
        }
    }

    /// Collect inline item children into the symbol table.
    /// Walk an expression and mark any identifier references into `referenced_labels`.
    fn collect_refs_from_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(name) => {
                self.referenced_labels.insert(name.clone());
            },
            Expr::Path(parts) => {
                // First part is the receiver object (label reference)
                if let Some(first) = parts.first() {
                    self.referenced_labels.insert(first.clone());
                }
            },
            Expr::Index(target, index) => {
                self.collect_refs_from_expr(target);
                self.collect_refs_from_expr(index);
            },
            Expr::Tuple(items) | Expr::List(items) => {
                for item in items {
                    self.collect_refs_from_expr(item);
                }
            },
            Expr::Binary(left, _, right) => {
                self.collect_refs_from_expr(left);
                self.collect_refs_from_expr(right);
            },
            Expr::Unary(_, inner) => {
                self.collect_refs_from_expr(inner);
            },
            Expr::Call(_, args) => {
                for arg in args {
                    self.collect_refs_from_expr(arg);
                }
            },
            Expr::Method(receiver, _, args) => {
                self.collect_refs_from_expr(receiver);
                for arg in args {
                    self.collect_refs_from_expr(arg);
                }
            },
            Expr::Closure(_, body) => {
                self.collect_refs_from_expr(body);
            },
            Expr::Conditional(cond, then_branch, else_branch) => {
                self.collect_refs_from_expr(cond);
                self.collect_refs_from_expr(then_branch);
                self.collect_refs_from_expr(else_branch);
            },
            Expr::Match(scrutinee, arms) => {
                self.collect_refs_from_expr(scrutinee);
                for (_pat, arm_expr) in arms {
                    self.collect_refs_from_expr(arm_expr);
                }
            },
            Expr::Construct(_, props) => {
                for prop in props {
                    self.collect_refs_from_expr(&prop.value);
                }
            },
            // Literals (Num, Percent, Str, Bool, Null) have no identifier references
            _ => {},
        }
    }

    /// Collect inline item children into the symbol table.
    pub(super) fn collect_inline_item(&mut self, item: &InlineItem) {
        match item {
            InlineItem::Labeled {
                label,
                array_index,
                ty,
                props,
                children,
                ..
            } => {
                for prop in props {
                    self.collect_refs_from_expr(&prop.value);
                }
                self.labels.insert(
                    label.clone(),
                    LabelInfo {
                        name: label.clone(),
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
                // Declared types of inline children feed effect-stage
                // validation; consumers gate on `is_effect_type`, so recording
                // every label here is inert for non-effect children.
                self.inline_child_types.insert(label.clone(), ty.clone());
                if array_index.is_some() {
                    self.array_labels.insert(label.clone());
                }
                for child in children {
                    self.collect_inline_item(child);
                }
            },
            InlineItem::Anonymous { children, .. } => {
                for child in children {
                    self.collect_inline_item(child);
                }
            },
            InlineItem::ForLoop {
                var,
                index_var,
                body,
                ..
            } => {
                let var_names: Vec<String> = match var {
                    LoopPattern::Single(name) => vec![name.clone()],
                    LoopPattern::Tuple(names) => names.clone(),
                };
                for name in &var_names {
                    self.labels.insert(
                        name.clone(),
                        LabelInfo {
                            name: name.clone(),
                            kind: LabelKind::For,
                            line: 0,
                            col: 0,
                            span: None,
                            ty: None,
                            inferred_type: None,
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
                            line: 0,
                            col: 0,
                            span: None,
                            ty: None,
                            inferred_type: None,
                            is_pub: false,
                            plot_params: Vec::new(),
                        },
                    );
                }
                for item in body {
                    self.collect_inline_item(item);
                }
            },
            InlineItem::SlotMarker => {},
            InlineItem::SlotFill { items, .. } => {
                for item in items {
                    self.collect_inline_item(item);
                }
            },
        }
    }
}
