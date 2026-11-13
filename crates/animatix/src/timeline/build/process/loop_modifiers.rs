//! Preserve build-loop bindings in reactive bodies using the existing IR scope.

use super::*;

impl Timeline {
    pub(super) fn collect_scoped_modifiers(
        &mut self,
        body: Vec<Stmt>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if self.build_loop_scopes.is_empty() || body.is_empty() {
            self.modifiers.extend(body);
            return;
        }
        let mut bindings = std::collections::BTreeMap::new();
        for name in self.build_loop_scopes.iter().flatten() {
            if let Some(value) = self.env.get(name) {
                if let Some(expr) = crate::timeline::value_parser::value_to_expr(&value) {
                    bindings.insert(name.clone(), expr);
                } else {
                    diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!("Reactive loop capture '{name}' cannot capture {value:?}; use scalar/list loop values."),
                    ));
                }
            }
        }
        let (names, values): (Vec<_>, Vec<_>) = bindings.into_iter().unzip();
        // A single-iteration lexical scope preserves shadowing by runtime
        // loops/locals and avoids substituting identifiers in expression trees.
        // Global scene variables remain frame-time reads; only loop locals bind.
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
