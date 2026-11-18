use super::stmt::{expand_embedded_plans_in_stmts, expand_expr};
use crate::module::{ComponentEntry, FnTemplate, FunctionContext, Namespace};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) type ComponentScopeMap = HashMap<PathBuf, Arc<HashMap<String, ComponentEntry>>>;

/// Maximum active component invocations before reporting a resource-limit error.
pub(super) const MAX_EXPANSION_DEPTH: usize = 64;

/// Mutable state threaded through the component expansion pipeline.
pub(super) struct ExpansionCtx {
    /// Monotonically increasing counter for generating unique anonymous labels.
    anon_counter: usize,
    /// Current active component invocation depth.
    pub(super) depth: usize,
    pub(super) active: Vec<(PathBuf, String, String)>,
    pub(super) diagnostics: Vec<crate::diagnostics::Diagnostic>,
    /// Per-source component maps used when expanding imported definitions.
    component_scopes: ComponentScopeMap,
}

impl ExpansionCtx {
    pub(super) fn new() -> Self {
        Self {
            anon_counter: 0,
            depth: 0,
            active: Vec::new(),
            diagnostics: Vec::new(),
            component_scopes: HashMap::new(),
        }
    }

    pub(super) fn with_component_scopes(component_scopes: ComponentScopeMap) -> Self {
        Self {
            component_scopes,
            ..Self::new()
        }
    }

    pub(super) fn component_scope(
        &self,
        path: &Path,
    ) -> Option<Arc<HashMap<String, ComponentEntry>>> {
        self.component_scopes.get(path).cloned()
    }

    /// Generate a unique label for an anonymous component instance.
    pub(super) fn next_anon_label(&mut self) -> String {
        let label = format!("__anon_{}", self.anon_counter);
        self.anon_counter += 1;
        label
    }
}

/// Expand module functions and values against the component scope at origin.
pub(in crate::module) fn expand_function_context(
    context: &FunctionContext,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
) -> FunctionContext {
    let scopes: ComponentScopeMap = context
        .modules
        .iter()
        .map(|(path, module)| (path.clone(), Arc::new(module.components.clone())))
        .collect();
    let mut expanded = context.clone();
    for module in Arc::make_mut(&mut expanded.modules).values_mut() {
        let mut ctx = ExpansionCtx::with_component_scopes(scopes.clone());
        module.statements =
            expand_embedded_plans_in_stmts(&module.statements, &module.components, &mut ctx);
        diagnostics.extend(ctx.diagnostics);
        for template in module.functions.values_mut() {
            expand_template(template, &scopes, diagnostics);
        }
        for template in module.visible_functions.values_mut() {
            expand_template(template, &scopes, diagnostics);
        }
        for namespace in module.namespaces.values_mut() {
            expand_namespace(namespace, &scopes, diagnostics);
        }
    }
    for template in Arc::make_mut(&mut expanded.visible).values_mut() {
        expand_template(template, &scopes, diagnostics);
    }
    expanded
}

fn expand_template(
    template: &mut FnTemplate,
    scopes: &ComponentScopeMap,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
) {
    let Some(components) = scopes.get(&template.definition_path) else {
        return;
    };
    let mut ctx = ExpansionCtx::with_component_scopes(scopes.clone());
    for param in &mut template.params {
        param.default = param.default.take().map(|expr| expand_expr(&expr, components, &mut ctx));
    }
    template.body = expand_embedded_plans_in_stmts(&template.body, components, &mut ctx);
    diagnostics.extend(ctx.diagnostics);
}

fn expand_namespace(
    namespace: &mut Namespace,
    scopes: &ComponentScopeMap,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
) {
    if let Some(components) = scopes.get(&namespace.definition_path) {
        let mut ctx = ExpansionCtx::with_component_scopes(scopes.clone());
        for export in namespace.exports.values_mut() {
            *export = expand_expr(export, components, &mut ctx);
        }
        diagnostics.extend(ctx.diagnostics);
    }
    for template in namespace.function_exports.values_mut() {
        expand_template(template, scopes, diagnostics);
    }
    for nested in namespace.namespaces.values_mut() {
        expand_namespace(nested, scopes, diagnostics);
    }
}
