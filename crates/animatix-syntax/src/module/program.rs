use crate::ast::Stmt;

use super::LoadedProgram;

impl LoadedProgram {
    /// Run the gradual type checker on the program's statements.
    ///
    /// Validates component instantiation properties against parameter type
    /// annotations. Returns diagnostics for any type mismatches found.
    /// Unannotated parameters accept any value.
    pub fn typecheck(&mut self) -> Vec<crate::diagnostics::Diagnostic> {
        self.typecheck_with_callables(&[])
    }

    /// Type-check the program with host-provided callable contracts.
    ///
    /// Native extensions are not part of the built-in standard catalog, so
    /// hosts pass the descriptors available in their extension context here.
    pub fn typecheck_with_callables(
        &mut self,
        callables: &[animatix_core::callable::CallableDescriptor],
    ) -> Vec<crate::diagnostics::Diagnostic> {
        let strict_types = self.extract_strict_types();
        let mut env = crate::typecheck::TypeEnv::new(&self.components, &self.module_fns)
            .with_strict_types(strict_types);
        for callable in callables {
            env.register_callable(callable);
        }
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

    /// Expand component instances into concrete statements and preserve the
    /// method templates required by the engine's callable VM.
    pub fn expand_components(&self, diagnostics: &mut Vec<String>) -> super::ExpandedProgram {
        let report = self.expand_components_with_diagnostics();
        diagnostics.extend(report.diagnostics.iter().map(|diagnostic| diagnostic.message.clone()));
        report.output
    }

    /// Expand atomically and retain structured errors, including depth limits.
    pub fn expand_components_with_diagnostics(
        &self,
    ) -> crate::diagnostics::BuildReport<super::ExpandedProgram> {
        let mut diagnostics = Vec::new();
        let component_scopes = self
            .function_context
            .modules
            .iter()
            .map(|(path, module)| (path.clone(), module.components.clone()))
            .collect();
        let mut expanded = super::expand::expand_statements_with_component_scopes(
            &self.statements,
            &self.components,
            &component_scopes,
            &mut diagnostics,
        );
        expanded.expanded_function_context = Some(super::expand::expand_function_context(
            &self.function_context,
            &mut diagnostics,
        ));
        super::expand::finish_expansion(expanded, diagnostics)
    }
}
