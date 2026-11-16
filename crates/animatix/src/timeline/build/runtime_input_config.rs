//! Parsing and validating the `config { runtime_inputs: ... }` name list.

use super::*;

pub(super) fn parse_runtime_input_names(
    value: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<String> {
    let Expr::List(items) = value else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::InvalidConfigValue,
                DiagnosticPhase::Build,
                "Config key 'runtime_inputs' expects a brace-list of names, e.g. {\"phase\", \"gain\"}.",
            )
            .with_subject("runtime_inputs"),
        );
        return Vec::new();
    };

    let mut names = std::collections::BTreeSet::new();
    for item in items {
        let Some(name) = (match item {
            Expr::Str(name) | Expr::Ident(name) => Some(name.as_str()),
            _ => None,
        }) else {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidConfigValue,
                    DiagnosticPhase::Build,
                    format!(
                        "Config key 'runtime_inputs' expects quoted strings or bare identifiers, got {item:?}."
                    ),
                )
                .with_subject("runtime_inputs"),
            );
            continue;
        };
        if !is_runtime_input_identifier(name) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidConfigValue,
                    DiagnosticPhase::Build,
                    format!("Config key 'runtime_inputs' contains invalid variable name '{name}'."),
                )
                .with_subject(name),
            );
            continue;
        }
        if matches!(name, "t" | "scene_width" | "scene_height") {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidConfigValue,
                    DiagnosticPhase::Build,
                    format!("'{name}' is a built-in frame variable and cannot be a runtime input."),
                )
                .with_subject(name),
            );
            continue;
        }
        if env.get_ref(name).is_some() {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidConfigValue,
                    DiagnosticPhase::Build,
                    format!(
                        "'{name}' already names a built-in or seeded value and cannot be declared as a runtime input."
                    ),
                )
                .with_subject(name),
            );
            continue;
        }
        if !names.insert(name.to_string()) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidConfigValue,
                    DiagnosticPhase::Build,
                    format!("Config key 'runtime_inputs' repeats '{name}'."),
                )
                .with_subject(name),
            );
        }
    }
    names.into_iter().collect()
}

fn is_runtime_input_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {},
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
