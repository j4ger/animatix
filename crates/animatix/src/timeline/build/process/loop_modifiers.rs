//! Preserve build-time lexical bindings in reactive bodies using the existing IR scope.

use super::*;

static NEXT_CAPTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Timeline {
    pub(super) fn collect_scoped_modifiers(
        &mut self,
        body: Vec<Stmt>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        if (self.build_loop_scopes.is_empty() && self.block_scope.is_empty()) || body.is_empty() {
            self.modifiers.extend(body);
            return;
        }
        let names = self
            .build_loop_scopes
            .iter()
            .flatten()
            .chain(self.block_scope.iter().flat_map(|scope| scope.keys()))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        // Actor identity is static at this point. Materialize aliases in both
        // reads and assignment targets rather than looking them up per frame.
        let aliases = names
            .iter()
            .filter_map(|name| match self.env.get(name) {
                Some(Value::ActorRef(actor)) => Some((name.clone(), actor.label().to_owned())),
                _ => None, // Scalar and callable captures are bound below.
            })
            .collect();
        let body = crate::timeline::actor_aliases::resolve_actor_aliases(body, aliases);
        let referenced = crate::timeline::plan_value::referenced_statement_names(&body);
        let mut bindings = std::collections::BTreeMap::new();
        for name in names {
            if !referenced.contains(&name) {
                continue;
            }
            let Some(value) = self.env.get(&name) else {
                continue;
            };
            if matches!(value, Value::ActorRef(_)) {
                continue; // Already rewritten to its concrete actor above.
            }
            let expr = crate::timeline::value_parser::value_to_expr(&value).unwrap_or_else(|| {
                // The existing environment base also holds compiler literals.
                // An unspellable key preserves callable/closure identity without
                // inventing source syntax or serializing a live value to text.
                let id = NEXT_CAPTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let key = format!("\0reactive_capture_{id}");
                self.env.set(&key, value);
                Expr::Ident(key)
            });
            bindings.insert(name, expr);
        }
        let (names, values): (Vec<_>, Vec<_>) = bindings.into_iter().unzip();
        // A single-iteration lexical scope preserves shadowing by runtime
        // loops/locals and avoids substituting identifiers in expression trees.
        // Global scene variables remain frame-time reads; only lexical locals bind.
        self.modifiers.push(Stmt::ForLoop {
            var: LoopPattern::Tuple(names),
            index_var: None,
            iterable: Expr::List(vec![Expr::List(values)]),
            modifiers: vec![],
            body,
            span: None,
        });
    }
}
