//! Evaluate resource requirements with the shared expression VM, never a build.

use std::collections::{HashMap, HashSet};

use animatix::timeline::{Environment, Value, evaluate_expr, prepare_input_environment};
use animatix_core::features::FeatureMask;
use animatix_std::catalog::catalog_lookup;
use animatix_syntax::ast::{
    Expr, InlineItem, LoopPattern, Property, Stmt, TargetSegment, array_actor_label,
};
use animatix_syntax::module::{ComponentEntry, LoadedProgram, Namespace, SceneData};
use animatix_syntax::visit::{AstVisitor, walk_inline_item, walk_stmt};

use super::prepare::{AnalysisDto, AssetRequirement, error_diagnostic};

struct Requirements<'a> {
    env: Environment,
    overrides: HashSet<&'a str>,
    actors: HashMap<String, String>,
    children: HashMap<String, HashSet<String>>,
    inline_parent: Option<String>,
    legend_text: Vec<String>,
    has_legend_host: bool,
    dynamic_legend_text: bool,
    transcripts: HashMap<String, Vec<String>>,
    caption_sources: HashSet<String>,
    dynamic_caption_source: bool,
    analysis: &'a mut AnalysisDto,
}

pub(super) fn collect(
    statements: &[Stmt],
    namespaces: &HashMap<String, Namespace>,
    vars: &[(String, String)],
    analysis: &mut AnalysisDto,
) {
    let vars = if statements.iter().any(|stmt| matches!(stmt, Stmt::Scene { .. })) {
        &[][..]
    } else {
        vars
    };
    let (mut env, diagnostics) = prepare_input_environment(statements, namespaces, vars);
    super::resources::extensions().install_functions(&mut env);
    analysis
        .diagnostics
        .extend(diagnostics.iter().map(crate::dto::DiagnosticDto::from_diagnostic));
    let overrides = vars.iter().map(|(name, _)| name.as_str()).collect();
    let mut visitor = Requirements {
        env,
        overrides,
        actors: HashMap::new(),
        children: HashMap::new(),
        inline_parent: None,
        legend_text: Vec::new(),
        has_legend_host: false,
        dynamic_legend_text: false,
        transcripts: HashMap::new(),
        caption_sources: HashSet::new(),
        dynamic_caption_source: false,
        analysis,
    };
    for statement in statements {
        visitor.visit_stmt(statement);
    }
    visitor.namespace_scenes(namespaces, reachable_play_targets(statements));
    for (actor, ids) in &visitor.transcripts {
        if visitor.dynamic_caption_source || visitor.caption_sources.contains(actor) {
            for id in ids {
                push_unique(&mut visitor.analysis.fonts.transcript_ids, id.clone());
            }
        }
    }
    if visitor.has_legend_host {
        for text in std::mem::take(&mut visitor.legend_text) {
            visitor.analysis.fonts.text.push_str(&text);
            visitor.analysis.fonts.text.push('\n');
        }
        visitor.analysis.fonts.dynamic |= visitor.dynamic_legend_text;
    }
}

impl Requirements<'_> {
    fn loop_values(&self, iterable: &Expr) -> Option<Vec<Value>> {
        const MAX_ITERATIONS: usize = 100_000;
        match evaluate_expr(iterable, &self.env) {
            Ok(Value::List(values)) if values.len() <= MAX_ITERATIONS => Some(values.to_vec()),
            Ok(Value::List(_)) => None,
            Ok(Value::Vec2([start, end])) => {
                let start = start as i64;
                let end = end as i64;
                let len = end.saturating_sub(start);
                (len >= 0 && len as usize <= MAX_ITERATIONS)
                    .then(|| (start..end).map(|index| Value::Num(index as f64)).collect())
            },
            Ok(Value::Vec3(values)) => Some(values.into_iter().map(Value::Num).collect()),
            Ok(Value::Vec4(values)) => Some(values.into_iter().map(Value::Num).collect()),
            Ok(value) => Some(vec![value]),
            Err(error) => {
                tracing::debug!("unresolved inline resource-analysis loop: {error}");
                None
            },
        }
    }

    fn visit_conservative_branches(&mut self, branches: &[&[Stmt]]) {
        let saved_env = self.env.clone();
        for branch in branches {
            self.env = saved_env.clone();
            for statement in *branch {
                self.visit_stmt(statement);
            }
        }
        self.env = saved_env;
    }

    fn visit_conditional(
        &mut self,
        condition: &Expr,
        then_branch: &[Stmt],
        else_branch: Option<&[Stmt]>,
    ) {
        match evaluate_expr(condition, &self.env) {
            Ok(value) if value.is_truthy() => {
                for statement in then_branch {
                    self.visit_stmt(statement);
                }
            },
            Ok(_) => {
                if let Some(branch) = else_branch {
                    for statement in branch {
                        self.visit_stmt(statement);
                    }
                }
            },
            Err(_) => {
                let mut branches = vec![then_branch];
                if let Some(branch) = else_branch {
                    branches.push(branch);
                }
                self.visit_conservative_branches(&branches);
            },
        }
    }

    fn visit_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[(animatix_syntax::ast::MatchPattern, Vec<Stmt>)],
    ) {
        let selector = Expr::Match(
            Box::new(scrutinee.clone()),
            arms.iter()
                .enumerate()
                .map(|(index, (pattern, _))| (pattern.clone(), Box::new(Expr::Num(index as f64))))
                .chain(std::iter::once((
                    animatix_syntax::ast::MatchPattern::Wildcard,
                    Box::new(Expr::Num(-1.0)),
                )))
                .collect(),
        );
        match evaluate_expr(&selector, &self.env) {
            Ok(Value::Num(index)) if index >= 0.0 && index == index.floor() => {
                if let Some((_, branch)) = arms.get(index as usize) {
                    for statement in branch {
                        self.visit_stmt(statement);
                    }
                }
            },
            Ok(Value::Num(-1.0)) => {},
            _ => {
                let branches: Vec<_> = arms.iter().map(|(_, branch)| branch.as_slice()).collect();
                self.visit_conservative_branches(&branches);
            },
        }
    }

    fn actor_type(&self, path: &str) -> Option<&str> {
        if let Some(ty) = self.actors.get(path) {
            return Some(ty);
        }
        let mut segments = path.split('.');
        let mut current = segments.next()?;
        for segment in segments {
            current = self.children.get(current)?.get(segment)?;
        }
        self.actors.get(current).map(String::as_str)
    }

    fn indexed_label(&self, base: &str, index: Option<&Expr>) -> Result<String, String> {
        let Some(index) = index else {
            return Ok(base.to_string());
        };
        match evaluate_expr(index, &self.env) {
            Ok(Value::Num(index)) if index >= 0.0 && index == index.floor() => {
                Ok(array_actor_label(base, index as usize))
            },
            Ok(Value::Num(index)) => {
                Err(format!("array index for '{base}' must be a non-negative integer, got {index}"))
            },
            Ok(value) => Err(format!("array index for '{base}' must be a number, got {value:?}")),
            Err(error) => Err(format!("array index for '{base}' could not be evaluated: {error}")),
        }
    }

    fn assignment_label(&self, target: &[TargetSegment]) -> Result<String, String> {
        target
            .iter()
            .map(|segment| match segment {
                TargetSegment::Static(label) => Ok(label.clone()),
                TargetSegment::Indexed { base, index } => self.indexed_label(base, Some(index)),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|segments| segments.join("."))
    }

    fn namespace_scenes(
        &mut self,
        namespaces: &HashMap<String, Namespace>,
        mut pending: Vec<String>,
    ) {
        let mut visited = HashSet::new();
        while let Some(target) = pending.pop() {
            if !visited.insert(target.clone()) {
                continue;
            }
            let Some((namespace, scene)) = resolve_namespace_scene(namespaces, &target) else {
                continue;
            };
            let saved = self.env.clone();
            let statements = scene.file_prelude.iter().chain(scene.body.iter()).cloned().collect();
            // Cross-file scenes retain their source AST until composition build.
            // Expand their exported components through the syntax module's normal
            // expansion path so resource/font closure sees the concrete actors.
            let mut definitions =
                animatix_syntax::module::discovery::collect_component_defs(&scene.file_prelude)
                    .into_iter()
                    .map(|definition| (definition.name.clone(), definition))
                    .collect::<HashMap<_, _>>();
            definitions.extend(
                namespace
                    .component_exports
                    .iter()
                    .map(|(name, definition)| (name.clone(), definition.clone())),
            );
            let components = definitions
                .into_iter()
                .map(|(name, definition)| {
                    (
                        name.clone(),
                        ComponentEntry {
                            definition: definition.clone(),
                            source_path: std::path::PathBuf::from("<namespace-scene>"),
                            actions: animatix_syntax::module::discovery::collect_component_actions(
                                &definition,
                            ),
                        },
                    )
                })
                .collect();
            let mut expansion_errors = Vec::new();
            let expanded = LoadedProgram {
                statements,
                components,
                ..Default::default()
            }
            .expand_components(&mut expansion_errors);
            for error in expansion_errors {
                self.analysis
                    .diagnostics
                    .push(error_diagnostic("expansion-error", &error, None));
            }
            for statement in &expanded {
                self.visit_stmt(statement);
            }
            self.env = saved;
            if let Some(target) = first_play_target(&scene.body) {
                pending.push(target);
            }
        }
    }
    fn text_value(&mut self, property: &Property) {
        match evaluate_expr(&property.value, &self.env) {
            Ok(Value::Str(text)) => match property.name.as_str() {
                "font_family" => push_unique(&mut self.analysis.fonts.families, text),
                "font_style" => push_unique(&mut self.analysis.fonts.styles, text),
                "font_weight" => {
                    let weight = animatix::renderer::text::parse_font_weight(&text);
                    push_unique(&mut self.analysis.fonts.weights, weight);
                },
                _ => {
                    self.analysis.fonts.text.push_str(&text);
                    self.analysis.fonts.text.push('\n');
                },
            },
            Ok(Value::Num(weight)) if property.name == "font_weight" => {
                push_unique(&mut self.analysis.fonts.weights, weight as f32);
            },
            Ok(_) => {
                self.analysis.fonts.dynamic = true;
            },
            Err(error) => {
                tracing::debug!("dynamic font requirement '{}': {error}", property.name);
                self.analysis.fonts.dynamic = true;
            },
        }
    }

    fn legend_value(&mut self, property: &Property, actor_label: &str) {
        match evaluate_expr(&property.value, &self.env) {
            Ok(Value::Str(text)) if text == "auto" => {
                self.legend_text.push(prettify_legend_label(actor_label));
            },
            Ok(Value::Str(text)) if text != "hidden" => {
                self.legend_text.push(text);
            },
            Ok(Value::Bool(true)) => self.legend_text.push(prettify_legend_label(actor_label)),
            Ok(Value::Str(_) | Value::Bool(false)) => {},
            Ok(_) | Err(_) => self.dynamic_legend_text = true,
        }
    }

    fn asset(&mut self, kind: &str, value: &Expr, label: &str) -> Option<String> {
        match evaluate_expr(value, &self.env) {
            Ok(Value::Str(url)) if !url.is_empty() => {
                if self.analysis.assets.iter().any(|asset| asset.id == url && asset.kind != kind) {
                    self.analysis.diagnostics.push(error_diagnostic(
                        "asset-kind-collision",
                        "one resource URL cannot have different decoding kinds",
                        Some(label),
                    ));
                    return None;
                }
                if !self.analysis.assets.iter().any(|asset| asset.id == url && asset.kind == kind) {
                    self.analysis.assets.push(AssetRequirement {
                        id: url.clone(),
                        url: url.clone(),
                        kind: kind.to_string(),
                    });
                }
                Some(url)
            },
            result => {
                self.analysis.diagnostics.push(error_diagnostic(
                    "dynamic-asset",
                    &format!("asset URL must be determinable before playback: {result:?}"),
                    Some(label),
                ));
                None
            },
        }
    }

    fn transcript(&mut self, value: &Expr, label: &str) {
        if let Some(id) = self.asset("text", value, label) {
            push_unique(self.transcripts.entry(label.to_string()).or_default(), id);
        }
    }

    fn caption_source(&mut self, value: &Expr) {
        match evaluate_expr(value, &self.env) {
            Ok(Value::Str(actor)) => {
                self.caption_sources.insert(actor);
            },
            result => {
                tracing::debug!("dynamic Captions source in font preparation: {result:?}");
                self.dynamic_caption_source = true;
            },
        }
    }

    fn actor(&mut self, ty: &str, label: &str, props: &[Property]) {
        self.actors.insert(label.to_string(), ty.to_string());
        if let Some(parent) = &self.inline_parent {
            self.children.entry(parent.clone()).or_default().insert(label.to_string());
        }
        let Some(info) = catalog_lookup(ty) else {
            if !animatix_std::is_effect(ty) {
                self.analysis.required_features |= FeatureMask::UNKNOWN.bits();
                self.analysis.diagnostics.push(error_diagnostic(
                    "unknown-actor-type",
                    &format!("actor type '{ty}' is unavailable in the web runtime"),
                    Some(label),
                ));
            }
            return;
        };
        self.analysis.required_features |= info.required_features.bits();
        if info.required_features.contains(FeatureMask::RICH_TEXT)
            || matches!(ty, "Equation" | "Fragment")
        {
            self.analysis.fonts.rich_text = true;
        }
        if matches!(ty, "Typst" | "Math" | "Equation" | "Fragment") {
            // A Typst program can generate characters absent from its source
            // (for example str.from-unicode). Prepare the default fallback book
            // before compilation; plain Text and literal Code remain demand-led.
            self.analysis.fonts.dynamic = true;
        }
        self.has_legend_host |= info.capabilities.legend_host;
        if info.capabilities.transcript {
            for prop in props.iter().filter(|prop| prop.name == "transcript") {
                self.transcript(&prop.value, label);
            }
        }
        if ty == "Captions" {
            self.caption_sources.insert(label.to_string());
            for prop in props.iter().filter(|prop| prop.name == "source") {
                self.caption_source(&prop.value);
            }
        }
        if ty == "Equation" || ty == "Fragment" {
            self.analysis.required_features |= FeatureMask::RICH_TEXT.bits();
        }
        if ty == "Math" || ty == "Equation" || ty == "Fragment" {
            push_unique(&mut self.analysis.fonts.families, "Fira Math".to_string());
        }
        if info.capabilities.text_paths
            || info.capabilities.plot_geometry
            || info.capabilities.plot_host
            || info.capabilities.legend_host
            || info.capabilities.callout
            || matches!(ty, "Captions" | "Counter" | "Legend")
        {
            push_unique(&mut self.analysis.fonts.families, "Open Sans".to_string());
            push_unique(&mut self.analysis.fonts.weights, 400.0);
            push_unique(&mut self.analysis.fonts.styles, "normal".to_string());
            for prop in props.iter().filter(|prop| is_text_property(&prop.name)) {
                self.text_value(prop);
            }
            if ty == "Counter" || info.capabilities.plot_host || info.capabilities.plot_geometry {
                self.analysis.fonts.text.push_str("0123456789.-+e");
            }
        }
        if is_legend_candidate(&info.capabilities) {
            if let Some(prop) = props.iter().find(|prop| prop.name == "legend") {
                self.legend_value(prop, label);
            } else {
                self.legend_text.push(prettify_legend_label(label));
            }
        }
        if matches!(ty, "Image" | "Svg")
            && let Some(prop) = props.iter().find(|prop| prop.name == "url")
        {
            // Errors are recorded in analysis; image URLs need no extra text closure.
            let _ = self.asset(if ty == "Svg" { "svg" } else { "image" }, &prop.value, label);
        }
    }

    fn locate_new_diagnostics(&mut self, from: usize, span: Option<animatix_syntax::ast::Span>) {
        if let Some(span) = span {
            for diagnostic in &mut self.analysis.diagnostics[from..] {
                diagnostic.line = Some(span.start_line);
                diagnostic.column = Some(span.start_col);
            }
        }
    }

    fn bind_loop(&mut self, pattern: &LoopPattern, value: Value, index: usize) {
        let diagnostics = animatix::timeline::bind_input_loop(&mut self.env, pattern, value, index);
        self.analysis
            .diagnostics
            .extend(diagnostics.iter().map(crate::dto::DiagnosticDto::from_diagnostic));
    }
}

fn reachable_play_targets(statements: &[Stmt]) -> Vec<String> {
    let mut targets = Vec::new();
    let mut has_scenes = false;
    for statement in statements {
        match statement {
            Stmt::Scene { body, .. } => {
                has_scenes = true;
                if let Some(target) = first_play_target(body) {
                    targets.push(target);
                }
            },
            Stmt::Play { scene_name, .. } if !has_scenes => targets.push(scene_name.clone()),
            _ => {},
        }
    }
    targets
}

fn first_play_target(statements: &[Stmt]) -> Option<String> {
    statements.iter().find_map(|statement| match statement {
        Stmt::Play { scene_name, .. } => Some(scene_name.clone()),
        _ => None,
    })
}

fn resolve_namespace_scene<'a>(
    namespaces: &'a HashMap<String, Namespace>,
    target: &str,
) -> Option<(&'a Namespace, &'a SceneData)> {
    let parts: Vec<_> = target.split('.').collect();
    if parts.len() < 2 || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let mut namespace = namespaces.get(parts[0])?;
    for part in &parts[1..parts.len() - 1] {
        namespace = namespace.namespaces.get(*part)?;
    }
    let scene = namespace.scenes.get(*parts.last()?)?;
    Some((namespace, scene))
}

impl<'ast> AstVisitor<'ast> for Requirements<'_> {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::Config { settings, .. } => {
                for prop in settings {
                    if prop.name == "resolution"
                        && let Ok(Value::Vec2([width, height])) =
                            evaluate_expr(&prop.value, &self.env)
                    {
                        self.env.set("scene_width", Value::Num(width));
                        self.env.set("scene_height", Value::Num(height));
                    }
                }
            },
            Stmt::LetDecl { name, value, .. } if !self.overrides.contains(name.as_str()) => {
                match evaluate_expr(value, &self.env) {
                    Ok(value) => self.env.set(name, value),
                    Err(error) => {
                        tracing::debug!("deferred let '{name}' in resource analysis: {error}")
                    },
                }
            },
            Stmt::ActorDecl {
                ty,
                label,
                array_index,
                props,
                span,
                ..
            } => {
                let before = self.analysis.diagnostics.len();
                let resolved = self.indexed_label(label, array_index.as_ref());
                match &resolved {
                    Ok(label) => self.actor(ty, label, props),
                    Err(error) => self.analysis.diagnostics.push(error_diagnostic(
                        "array-index",
                        error,
                        Some(label),
                    )),
                }
                let saved_parent = self.inline_parent.clone();
                if let Ok(label) = resolved {
                    self.inline_parent = Some(label);
                }
                walk_stmt(self, statement);
                self.inline_parent = saved_parent;
                self.locate_new_diagnostics(before, *span);
                return;
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.visit_conditional(condition, then_branch, else_branch.as_deref());
                return;
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                self.visit_match(scrutinee, arms);
                return;
            },
            Stmt::Assignment {
                target,
                property,
                value,
                span,
                ..
            } => {
                let before = self.analysis.diagnostics.len();
                let label = self.assignment_label(target);
                if let Err(error) = &label {
                    self.analysis.diagnostics.push(error_diagnostic(
                        "array-index",
                        error,
                        target.first().map(TargetSegment::label_str),
                    ));
                }
                if let Ok(label) = label {
                    if property == "url" {
                        let kind = if self.actor_type(&label).is_some_and(|ty| ty == "Svg") {
                            "svg"
                        } else {
                            "image"
                        };
                        // Errors are recorded in analysis; no text closure for images.
                        let _ = self.asset(kind, value, &label);
                    } else if property == "transcript" {
                        self.transcript(value, &label);
                    } else if property == "source"
                        && self.actor_type(&label).is_some_and(|ty| ty == "Captions")
                    {
                        self.caption_source(value);
                    } else if is_text_property(property) {
                        self.text_value(&Property::new(property.clone(), value.clone()));
                    } else if property == "legend"
                        && self.actor_type(&label).is_some_and(|ty| {
                            catalog_lookup(ty)
                                .is_some_and(|info| is_legend_candidate(&info.capabilities))
                        })
                    {
                        self.legend_value(&Property::new(property.clone(), value.clone()), &label);
                    }
                }
                self.locate_new_diagnostics(before, *span);
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                ..
            } => {
                if let Some(values) = self.loop_values(iterable) {
                    for (index, value) in values.iter().enumerate() {
                        let saved = self.env.clone();
                        self.bind_loop(var, value.clone(), index);
                        if let Some(name) = index_var {
                            self.env.set(name, Value::Num(index as f64));
                        }
                        for child in body {
                            self.visit_stmt(child);
                        }
                        self.env = saved;
                    }
                    return;
                }
                tracing::debug!(
                    "unresolved resource-analysis loop; visiting its body conservatively"
                );
            },
            Stmt::Block { body, .. } => {
                let saved = self.env.clone();
                for child in body {
                    self.visit_stmt(child);
                }
                self.env = saved;
                return;
            },
            _ => {}, // The shared AST walker covers remaining statement bodies.
        }
        walk_stmt(self, statement);
    }

    fn visit_inline_item(&mut self, item: &'ast InlineItem) {
        if let InlineItem::ForLoop {
            var,
            index_var,
            iterable,
            body,
        } = item
        {
            match self.loop_values(iterable) {
                Some(values) => {
                    for (index, value) in values.iter().enumerate() {
                        let saved = self.env.clone();
                        self.bind_loop(var, value.clone(), index);
                        if let Some(name) = index_var {
                            self.env.set(name, Value::Num(index as f64));
                        }
                        for child in body {
                            self.visit_inline_item(child);
                        }
                        self.env = saved;
                    }
                },
                None => {
                    tracing::debug!(
                        "visiting unresolved inline resource-analysis loop body conservatively"
                    );
                    for child in body {
                        self.visit_inline_item(child);
                    }
                },
            }
            return;
        }
        let saved_parent = self.inline_parent.clone();
        match item {
            InlineItem::Anonymous { ty, props, .. } => self.actor(ty, "anonymous", props),
            InlineItem::Labeled {
                label,
                array_index,
                ty,
                props,
                ..
            } => match self.indexed_label(label, array_index.as_ref()) {
                Ok(label) => {
                    self.actor(ty, &label, props);
                    self.inline_parent = Some(label);
                },
                Err(error) => self.analysis.diagnostics.push(error_diagnostic(
                    "array-index",
                    &error,
                    Some(label),
                )),
            },
            _ => {}, // Shared traversal visits nested inline containers.
        }
        walk_inline_item(self, item);
        self.inline_parent = saved_parent;
    }
}

fn is_text_property(name: &str) -> bool {
    matches!(
        name,
        "text"
            | "content"
            | "math"
            | "code"
            | "font_family"
            | "font_weight"
            | "font_style"
            | "label"
            | "title"
            | "prefix"
            | "suffix"
    )
}

fn is_legend_candidate(capabilities: &animatix_core::caps::PrimitiveCapabilities) -> bool {
    capabilities.is_shape || capabilities.plot_geometry
}

fn prettify_legend_label(label: &str) -> String {
    label
        .split(['_', '-', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn push_unique<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}
