use super::*;

mod diff;

impl Timeline {
    pub(super) fn prepare_text_sources(
        &mut self,
        kind: TextDeclarationKind,
        label: &str,
        props: &[Property],
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        parsed: &mut ParsedTextProperties,
    ) -> Result<Option<animatix_text::TypstSources>, RenderError> {
        let mut value = |name| {
            props.iter().rev().find(|prop| prop.name == name).and_then(|prop| {
                evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, label)
            })
        };
        let entry = value("text_file");
        let file_backed = matches!(entry, Some(Value::Str(_)));
        let modules = value("typst_modules");
        let previous = value("diff_from");
        let previous_file = value("diff_file");
        let cache = std::sync::Arc::make_mut(&mut self.asset_cache);
        let mut load = |path: &str| {
            let key = cache.normalize_project_file(path);
            let text = cache.load_text_source(&key).map_err(RenderError::TextCompilation)?;
            cache.record_usage(&key, label);
            Ok::<_, RenderError>(text.to_string())
        };
        if let Some(Value::Str(path)) = entry {
            if props.iter().any(|prop| kind.content_matches(&prop.name)) {
                return Err(RenderError::TextCompilation(
                    "specify either text_file or inline source".into(),
                ));
            }
            parsed.text_content = load(&path)?;
        }
        match (previous, previous_file) {
            (Some(_), Some(_)) => {
                return Err(RenderError::TextCompilation(
                    "specify either diff_from or diff_file".into(),
                ));
            },
            (Some(Value::Str(source)), None) => {
                parsed.text_content = diff::line_diff(&source, &parsed.text_content)?;
                parsed.code_diff = true;
            },
            (None, Some(Value::Str(path))) => {
                parsed.text_content = diff::line_diff(&load(&path)?, &parsed.text_content)?;
                parsed.code_diff = true;
            },
            _ => {}, // Neither diff input is present or its expression already reported a diagnostic.
        }
        let Some(modules) = modules else {
            return Ok((kind == TextDeclarationKind::Typst && file_backed)
                .then(animatix_text::TypstSources::default));
        };
        let Value::List(modules) = modules else {
            return Err(RenderError::TextCompilation(
                "typst_modules expects a list of {virtual_name, file} pairs".into(),
            ));
        };
        let mut sources = std::collections::BTreeMap::new();
        for module in modules.iter() {
            if let Value::List(pair) = module
                && let [Value::Str(name), Value::Str(path)] = pair.as_ref()
                && name.ends_with(".typ")
                && name.split('/').all(|part| !matches!(part, "" | "." | ".."))
                && !name.contains('\\')
                && name != "main.typ"
                && !sources.contains_key(name)
            {
                sources.insert(name.clone(), load(path)?);
            } else {
                return Err(RenderError::TextCompilation(
                    "each Typst module needs a unique relative virtual .typ name and a UTF-8 file path".into(),
                ));
            }
        }
        Ok(Some(animatix_text::TypstSources(sources.into_iter().collect())))
    }
}

pub(super) fn parse_spans(
    props: &[Property],
    env: &Environment,
    label: &str,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<Vec<crate::timeline::animation_track::TextSpan>>, RenderError> {
    use crate::timeline::animation_track::TextSpan;
    let Some(property) = props.iter().rev().find(|prop| prop.name == "text_spans") else {
        return Ok(None);
    };
    let Some(Value::List(entries)) =
        evaluate_expr_with_lookup_diagnostic(&property.value, env, diagnostics, label)
    else {
        return Err(RenderError::TextCompilation(
            "text_spans expects a list of span entries".into(),
        ));
    };
    let length = text.chars().count() as f64;
    let mut names = std::collections::HashSet::new();
    let mut spans = Vec::new();
    for entry in entries.iter() {
        if let Value::List(fields) = entry
            && let [
                Value::Str(name),
                Value::Num(start),
                Value::Num(end),
                Value::Color(color) | Value::Vec4(color),
                Value::Num(opacity),
                Value::Bool(underline),
            ] = fields.as_ref()
            && !name.is_empty()
            && names.insert(name.clone())
            && *start >= 0.0
            && *end > *start
            && *end <= length
            && start.fract() == 0.0
            && end.fract() == 0.0
            && (0.0..=1.0).contains(opacity)
        {
            spans.push(TextSpan {
                name: name.clone(),
                range: (*start as u32)..(*end as u32),
                color: animatix_text::f32_color_to_rgba8(&color.map(|n| n as f32)),
                opacity: *opacity as f32,
                underline: *underline,
            });
        } else {
            return Err(RenderError::TextCompilation(
                "text_spans entries require {unique_name, start, end, color, opacity, underline} and in-bounds Unicode scalar offsets".into(),
            ));
        }
    }
    Ok(Some(spans))
}
