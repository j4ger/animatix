use crate::ast::Stmt;

use super::expand::expand_statements;
use super::{LoadedProgram, inline_actions};

impl LoadedProgram {
    /// Run the gradual type checker on the program's statements.
    ///
    /// Validates component instantiation properties against parameter type
    /// annotations. Returns diagnostics for any type mismatches found.
    /// Unannotated parameters accept any value.
    pub fn typecheck(&mut self) -> Vec<crate::diagnostics::Diagnostic> {
        let strict_types = self.extract_strict_types();
        let mut env = crate::typecheck::TypeEnv::new(&self.components, &self.module_fns)
            .with_strict_types(strict_types);
        env.register_module_aliases(&self.namespaces);
        env.check_statements(&self.statements)
    }

    /// Extract `strict_types` config value from prelude config statements.
    fn extract_strict_types(&self) -> bool {
        for stmt in &self.statements {
            if let Stmt::Config { settings, .. } = stmt {
                for setting in settings {
                    if setting.name == "strict_types" {
                        return match &setting.value {
                            crate::ast::Expr::Bool(b) => *b,
                            crate::ast::Expr::Str(s) => s.parse().unwrap_or(false),
                            _ => false,
                        };
                    }
                }
            }
        }
        false
    }

    /// Expand component instances into concrete statements and expand
    /// timeline-function calls. Expansion diagnostics (recursion cycles,
    /// statement-level pure-function calls) are appended to `diagnostics`.
    pub fn expand_components(&self, diagnostics: &mut Vec<String>) -> Vec<Stmt> {
        let (stmts, registry) = expand_statements(&self.statements, &self.components);
        inline_actions::expand_fn_calls(stmts, &registry, &self.module_fns, diagnostics)
    }
}
