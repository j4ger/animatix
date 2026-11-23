use super::*;

pub(super) fn apply(timeline: &mut Timeline, ast: &[Stmt], diagnostics: &mut Vec<Diagnostic>) {
    let settings = ast
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Config { settings, .. } => Some(settings.as_slice()),
            _ => None,
        })
        .flatten();
    let mut project_only = false;
    for setting in settings.clone().filter(|setting| setting.name == "font_files_only") {
        match &setting.value {
            crate::ast::Expr::Bool(value) => project_only = *value,
            _ => diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidConfigValue,
                    DiagnosticPhase::Build,
                    "font_files_only expects true or false",
                )
                .with_subject("font_files_only"),
            ),
        }
    }
    if project_only {
        timeline.font_context = std::sync::Arc::new(crate::renderer::text::FontContext::empty());
    }
    for setting in settings.filter(|setting| setting.name == "font_files") {
        if let Err(error) = register(timeline, &setting.value) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidConfigValue,
                    DiagnosticPhase::Build,
                    error,
                )
                .with_subject("font_files"),
            );
        }
    }
    if project_only && timeline.font_context.registered_face_count() == 0 {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::InvalidConfigValue,
                DiagnosticPhase::Build,
                "font_files_only requires at least one usable project font",
            )
            .with_subject("font_files_only"),
        );
    }
}

fn register(timeline: &mut Timeline, expr: &crate::ast::Expr) -> Result<(), String> {
    let Value::List(paths) =
        crate::timeline::evaluate_expr(expr, &timeline.env).map_err(|error| error.to_string())?
    else {
        return Err("font_files expects a list of font file paths".into());
    };
    let cache = std::sync::Arc::make_mut(&mut timeline.asset_cache);
    let mut context = timeline.font_context.as_ref().clone();
    for value in paths.iter() {
        let Value::Str(path) = value else {
            return Err("font_files expects string paths".into());
        };
        let key = cache.normalize_project_file(path);
        let bytes = cache.load_font_file(&key)?;
        let before = context.registered_face_count();
        context.load_font_data([bytes]);
        if context.registered_face_count() == before {
            return Err(format!("font '{path}' has no usable font faces"));
        }
        cache.record_usage(&key, "font_files");
    }
    timeline.font_context = std::sync::Arc::new(context);
    Ok(())
}
