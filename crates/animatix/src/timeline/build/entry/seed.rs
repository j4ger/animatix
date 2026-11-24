//! Seed the build environment.
//!
//! Fills the timeline with everything the statement walk expects to find:
//! build quality and extension wiring, the standard library and pure `fn`s,
//! the colorscheme, namespace exports, `config` settings, the carry bag and
//! `--set` overrides. Also runs the speech pre-pass so the returned AST already
//! has speech-anchored keyframes lowered to times.

use super::*;

/// What the seeding phase hands to the statement walk.
pub(super) struct SeededBuild {
    /// The input AST with speech-anchored keyframe times lowered.
    pub ast: Vec<Stmt>,
}

/// Seed the build environment and return the AST the statement walk consumes.
pub(super) fn seed_environment(
    timeline: &mut Timeline,
    ast: &[Stmt],
    namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
    options: &BuildOptions<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> SeededBuild {
    // Pre-scan the program for actor labels referenced by expressions so
    // build_eval_env injects only those actors' properties (see
    // build::referenced_roots for the safety argument).
    let scan = super::referenced_roots::scan_references(ast);
    timeline.referenced_roots = Some(scan.roots);
    timeline.stats_used = scan.stats_used;
    timeline.is_animating_used = scan.is_animating_used;
    timeline.wildcard_actors = scan.wildcard_actors;
    timeline.referenced_properties = scan.referenced_properties;
    timeline.build_quality = options.quality.unwrap_or(crate::timeline::BuildQuality::Production);
    timeline.extensions = options.extensions.clone();
    if let Some(registry) = options.selected_primitive_registry() {
        timeline.primitive_registry = registry;
    }
    // Behavior dispatch and the primitive registry are two views of the same
    // extension set: the render walk asks for a BehaviorDef, while declaration
    // building and property dispatch ask the PrimitiveRegistry. Keep the
    // behavior table in sync whenever a build installs a registry.
    timeline.sync_extension_behaviors();
    if let Some(cache) = options.asset_cache.clone() {
        timeline.asset_cache = cache;
        std::sync::Arc::make_mut(&mut timeline.asset_cache).invalidate_changed_assets();
    }
    if let Some(source_path) = options.function_scope {
        timeline
            .env
            .set_source_path(Some(std::sync::Arc::new(source_path.to_path_buf())));
        std::sync::Arc::make_mut(&mut timeline.asset_cache).set_document_dir(source_path.parent());
    }
    load_standard_library(&mut timeline.env);
    if let Some(ctx) = timeline.extensions.as_ref() {
        ctx.install_functions(&mut timeline.env);
    }
    // Seed ordinary user functions so expression calls resolve against the
    // build environment regardless of their return type annotation.
    let local_functions = super::fn_eval::collect_user_fns(ast);
    let needs_fallback_context = !local_functions.is_empty()
        || options.instance_methods.is_some_and(|methods| !methods.is_empty())
        || options
            .scene_methods
            .is_some_and(|scenes| scenes.values().any(|methods| !methods.is_empty()))
        || namespaces.values().any(namespace_has_functions);
    let fallback_context = (options.function_context.is_none() && needs_fallback_context)
        .then(|| super::fn_eval::context_from_ast_at(ast, namespaces, options.function_scope));
    let function_context = options
        .function_context
        .map(|context| std::sync::Arc::new(context.clone()))
        .or_else(|| fallback_context.as_ref().map(|(context, _)| context.clone()));
    timeline.env.set_function_context(function_context.clone());
    if let Some(instance_methods) = options.instance_methods {
        crate::timeline::build::install_input_methods(
            &mut timeline.env,
            instance_methods,
            function_context.as_deref(),
        );
    }
    let function_scope = options
        .function_scope
        .or_else(|| fallback_context.as_ref().map(|(_, path)| path.as_path()));
    let mut seeded_functions = std::collections::HashSet::new();
    if let (Some(context), Some(scope_path)) = (&function_context, function_scope) {
        if let Some(module) = context.modules.get(scope_path) {
            for (name, template) in &module.visible_functions {
                timeline.env.set(
                    name,
                    super::fn_eval::user_function_value(name, template, Some(context.clone())),
                );
                seeded_functions.insert(name.clone());
            }
            for (alias, namespace) in &module.namespaces {
                seed_namespace_functions(
                    &mut timeline.env,
                    &format!("{alias}::"),
                    namespace,
                    context,
                    &mut seeded_functions,
                );
            }
        }
    } else if let Some(context) = &function_context {
        for (name, template) in context.visible.iter() {
            timeline.env.set(
                name,
                super::fn_eval::user_function_value(name, template, Some(context.clone())),
            );
            seeded_functions.insert(name.clone());
        }
    }
    if let Some(context) = &function_context {
        for (alias, namespace) in namespaces {
            seed_namespace_functions(
                &mut timeline.env,
                &format!("{alias}::"),
                namespace,
                context,
                &mut seeded_functions,
            );
        }
    }
    for (name, params, return_type, body) in local_functions {
        if seeded_functions.contains(&name) {
            continue;
        }
        let definition_path = function_scope.map(std::path::Path::to_path_buf);
        timeline.env.set(
            &name,
            Value::UserFn {
                name: name.clone(),
                params: std::sync::Arc::new(params),
                return_type,
                body: std::sync::Arc::new(body),
                function_context: function_context.clone(),
                definition_path,
                component_scope: None,
            },
        );
        seeded_functions.insert(name);
    }
    seed_host_actor_references(&mut timeline.env, ast);
    timeline.apply_colorscheme(BuiltInColorscheme::DefaultDark.resolved());
    // Seed build-time environment with scene dimensions so `let` declarations
    // can reference `scene_width` / `scene_height`.
    timeline.canvas_size = animatix_syntax::config_keys::configured_canvas_size(ast, diagnostics);
    let canvas = timeline.canvas_dimensions();
    seed_canvas_globals(&mut timeline.env, canvas.width, canvas.height);

    timeline.load_colorscheme_declarations(ast, diagnostics);

    timeline.seed_role_values();

    // Seed environment with namespace exports, recursing into nested aliases.
    for (alias, namespace) in namespaces {
        seed_namespace_exports(timeline, diagnostics, alias, namespace);
    }

    for stmt in ast {
        if let Stmt::Config { settings, .. } = stmt {
            timeline.apply_config_settings(settings, diagnostics);
        }
    }
    super::project_fonts::apply(timeline, ast, diagnostics);

    timeline.seed_style_values();

    // Inject carry bag (if any) BEFORE statement processing so that
    // carried actors are visible to re-declarations and assignments.
    if let Some(carry) = options.carry {
        timeline.inject_carry_bag(
            carry.bag,
            carry.source_timeline,
            carry.source_duration_ms,
            carry.dims,
            diagnostics,
        );
    }

    seed_cli_defines(timeline, options, diagnostics);

    // Pre-pass only when a transcript declaration exists; it is needed to
    // lower speech-anchored keyframe times. Actual transcript components are
    // attached later by the actor declaration path using its resolved label.
    let mut transcripts = std::collections::HashMap::new();
    if super::speech::needs_speech_transcript_prepass(ast) {
        let mut transcript_env = timeline.env.clone();
        let overridden = timeline.cli_defines.clone();
        let beat_ms = timeline.beat_duration_ms();
        super::speech::collect_transcripts_from_ast(
            ast,
            std::sync::Arc::make_mut(&mut timeline.asset_cache),
            &mut transcripts,
            &mut transcript_env,
            &overridden,
            beat_ms,
        );
    }

    // Pre-pass: lower any speech-anchored keyframe times in the AST
    let ast = super::speech::lower_speech_keyframes(
        ast,
        &transcripts,
        timeline.build_quality,
        options.draft_cue_times,
        diagnostics,
    );

    SeededBuild { ast }
}

fn seed_canvas_globals(env: &mut Environment, width: u32, height: u32) {
    env.set_host_global("scene_width", Value::Num(f64::from(width)));
    env.set_host_global("scene_height", Value::Num(f64::from(height)));
}

fn namespace_has_functions(namespace: &crate::module::Namespace) -> bool {
    !namespace.function_exports.is_empty()
        || namespace.namespaces.values().any(namespace_has_functions)
}

/// Apply command-line bindings after namespace exports and carry inputs exist.
fn seed_cli_defines(
    timeline: &mut Timeline,
    options: &BuildOptions<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // `--set NAME=VALUE` overrides. Seeded here — after the colorscheme,
    // namespace exports and carry bag, so an override can reference them —
    // and shadow every authored `let` of the same name (see `process_body`),
    // so a template's own declaration reads as its default rather than
    // blocking the override.
    let cli_defines = super::defines::parse_defines(options.defines, diagnostics);
    for define in &cli_defines {
        timeline.cli_defines.insert(define.name.clone());
        match crate::timeline::evaluate_expr(&define.value, &timeline.env) {
            Ok(value) => {
                timeline.env.set_host_global(&define.name, value);
            },
            Err(err) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!(
                            "`--set {}`: the value is not evaluable at build time ({err:?}).",
                            define.name
                        ),
                    )
                    .with_subject(define.name.as_str()),
                );
            },
        }
    }
}

fn seed_namespace_functions(
    env: &mut Environment,
    prefix: &str,
    namespace: &crate::module::Namespace,
    context: &std::sync::Arc<crate::module::FunctionContext>,
    seeded: &mut std::collections::HashSet<String>,
) {
    for (name, template) in &namespace.function_exports {
        let qualified = format!("{prefix}{name}");
        env.set(
            &qualified,
            super::fn_eval::user_function_value(name, template, Some(context.clone())),
        );
        seeded.insert(qualified);
    }
    for (alias, nested) in &namespace.namespaces {
        seed_namespace_functions(env, &format!("{prefix}{alias}::"), nested, context, seeded);
    }
}

fn seed_host_actor_references(env: &mut Environment, ast: &[Stmt]) {
    fn inline_labels(
        items: &[crate::ast::InlineItem],
        parent: &str,
        labels: &mut Vec<String>,
        aliases: &mut std::collections::HashMap<String, Option<String>>,
    ) {
        for item in items {
            match item {
                crate::ast::InlineItem::Labeled {
                    label,
                    array_index,
                    children,
                    ..
                } => {
                    let path = crate::timeline::build::process::qualify_child(label, parent);
                    labels.push(path.clone());
                    if array_index.is_some() {
                        aliases
                            .entry(label.clone())
                            .and_modify(|existing| {
                                if existing.as_deref() != Some(path.as_str()) {
                                    *existing = None;
                                }
                            })
                            .or_insert_with(|| Some(path.clone()));
                    }
                    inline_labels(children, &path, labels, aliases);
                },
                crate::ast::InlineItem::SlotMarker => {},
                crate::ast::InlineItem::SlotFill { items, .. }
                | crate::ast::InlineItem::ForLoop { body: items, .. }
                | crate::ast::InlineItem::Anonymous {
                    children: items, ..
                } => {
                    inline_labels(items, parent, labels, aliases);
                },
                crate::ast::InlineItem::Expr(..) => {},
            }
        }
    }

    fn walk(
        statements: &[Stmt],
        labels: &mut Vec<String>,
        aliases: &mut std::collections::HashMap<String, Option<String>>,
    ) {
        for statement in statements {
            match statement {
                Stmt::ActorDecl {
                    label,
                    array_index,
                    children,
                    ..
                } => {
                    labels.push(label.clone());
                    if array_index.is_some() {
                        aliases
                            .entry(label.clone())
                            .and_modify(|existing| {
                                if existing.as_deref() != Some(label.as_str()) {
                                    *existing = None;
                                }
                            })
                            .or_insert_with(|| Some(label.clone()));
                    }
                    inline_labels(children, label, labels, aliases);
                },
                Stmt::Keyframe { body, .. }
                | Stmt::RelativeKeyframe { body, .. }
                | Stmt::Always { body, .. }
                | Stmt::ForLoop { body, .. }
                | Stmt::Scene { body, .. }
                | Stmt::Block { body, .. } => walk(body, labels, aliases),
                Stmt::Conditional {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    walk(then_branch, labels, aliases);
                    if let Some(else_branch) = else_branch {
                        walk(else_branch, labels, aliases);
                    }
                },
                Stmt::Match { arms, .. } => {
                    for (_, body) in arms {
                        walk(body, labels, aliases);
                    }
                },
                // Sequence/stagger expression bodies are private plan scopes.
                Stmt::Sequence { .. }
                | Stmt::Stagger { .. }
                | Stmt::FnDecl { .. }
                | Stmt::Expr(crate::ast::Expr::Timeline(_), ..)
                | Stmt::Expr(crate::ast::Expr::Sequence(_), ..)
                | Stmt::Expr(crate::ast::Expr::Stagger { .. }, ..) => {},
                _ => {},
            }
        }
    }

    let mut labels = Vec::new();
    let mut aliases = std::collections::HashMap::new();
    walk(ast, &mut labels, &mut aliases);
    labels.sort();
    labels.dedup();
    for label in &labels {
        env.set(
            label,
            Value::ActorRef(crate::timeline::plan_value::ActorRef::caller(label.clone())),
        );
    }
    for (alias, target) in aliases {
        let Some(target) = target else {
            continue;
        };
        if !labels.contains(&alias) && env.get(&alias).is_none() {
            env.set(&alias, Value::ActorRef(crate::timeline::plan_value::ActorRef::caller(target)));
        }
    }
}
