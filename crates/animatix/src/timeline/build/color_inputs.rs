//! Colorscheme parsing and selection shared by timeline and resource preparation.

use super::*;

/// Parse authored colorschemes and resolve their inheritance without a Timeline.
pub(super) fn load_colorscheme_declarations(
    ast: &[Stmt],
    diagnostics: &mut Vec<Diagnostic>,
) -> std::collections::HashMap<String, ResolvedColorscheme> {
    let mut schemes = std::collections::HashMap::new();
    let mut inheritance_edges = std::collections::HashMap::new();

    for stmt in ast {
        if let Stmt::LetDecl {
            name,
            value: Expr::Construct(type_name, properties),
            ..
        } = stmt
            && type_name == "Colorscheme"
        {
            let mut extends = None;
            let mut scheme_props = Vec::new();
            for prop in properties {
                if prop.name == "extends" {
                    if let Expr::Str(base) = &prop.value {
                        extends = Some(base.clone());
                    }
                } else {
                    scheme_props.push(prop.clone());
                }
            }
            if let Some(scheme) =
                ResolvedColorscheme::from_properties(name.clone(), &scheme_props, diagnostics)
            {
                if let Some(base_name) = extends {
                    inheritance_edges.insert(name.clone(), base_name);
                }
                schemes.insert(name.clone(), scheme);
            }
        }
    }

    let mut resolved = std::collections::HashMap::new();
    for name in schemes.keys() {
        if let Some(scheme) = resolve_colorscheme_with_inheritance(
            name,
            &schemes,
            &inheritance_edges,
            &mut resolved,
            &mut std::collections::HashSet::new(),
            diagnostics,
        ) {
            resolved.insert(name.clone(), scheme);
        }
    }
    resolved
}

fn resolve_colorscheme_with_inheritance(
    name: &str,
    schemes: &std::collections::HashMap<String, ResolvedColorscheme>,
    edges: &std::collections::HashMap<String, String>,
    resolved: &mut std::collections::HashMap<String, ResolvedColorscheme>,
    visiting: &mut std::collections::HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ResolvedColorscheme> {
    if let Some(scheme) = resolved.get(name) {
        return Some(scheme.clone());
    }

    if !visiting.insert(name.to_string()) {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::ColorschemeInheritanceCycle,
                DiagnosticPhase::Build,
                format!("Colorscheme inheritance cycle detected involving '{}'.", name),
            )
            .with_subject(name),
        );
        return None;
    }

    let mut scheme = schemes.get(name)?.clone();
    if let Some(base_name) = edges.get(name) {
        if let Some(base_builtin) = BuiltInColorscheme::from_name(base_name) {
            scheme.merge_with_base(&base_builtin.resolved());
        } else if let Some(base_resolved) = resolve_colorscheme_with_inheritance(
            base_name,
            schemes,
            edges,
            resolved,
            visiting,
            diagnostics,
        ) {
            scheme.merge_with_base(&base_resolved);
        } else {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownColorscheme,
                    DiagnosticPhase::Build,
                    format!(
                        "Colorscheme '{}' extends unknown base '{}'; using as-is.",
                        name, base_name
                    ),
                )
                .with_subject(name),
            );
        }
    }

    visiting.remove(name);
    Some(scheme)
}

/// Resolve a valid config colorscheme name. `None` preserves the active default
/// when the name is unknown, matching timeline config processing.
pub(super) fn select_colorscheme(
    name: &str,
    external: &std::collections::HashMap<String, ResolvedColorscheme>,
) -> Option<ResolvedColorscheme> {
    BuiltInColorscheme::from_name(name)
        .map(BuiltInColorscheme::resolved)
        .or_else(|| external.get(name).cloned())
}
