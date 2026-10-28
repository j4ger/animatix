//! Seed the build environment.
//!
//! Fills the timeline with everything the statement walk expects to find:
//! build quality and extension wiring, the standard library and pure `fn`s,
//! the colorscheme, namespace exports, `config` settings, the carry bag and
//! `--set` overrides. Also runs the speech pre-pass so the returned AST already
//! has speech-anchored keyframes lowered to times.

use animatix_text::transcript::SpeechTranscript;

use super::*;

/// What the seeding phase hands to the statement walk.
pub(super) struct SeededBuild {
    /// The input AST with speech-anchored keyframe times lowered.
    pub ast: Vec<Stmt>,
    /// Transcripts to attach once the walk has created the actor entities.
    pub transcripts: std::collections::HashMap<String, SpeechTranscript>,
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
    if let Some(cache) = options.asset_cache.clone() {
        timeline.asset_cache = cache;
        std::sync::Arc::make_mut(&mut timeline.asset_cache).invalidate_changed_assets();
    }
    load_standard_library(&mut timeline.env);
    if let Some(ctx) = timeline.extensions.as_ref() {
        ctx.install_functions(&mut timeline.env);
    }
    // Seed pure user functions (`fn f(...) -> T`) so expression calls
    // resolve against the build environment.
    for (name, params, body) in super::fn_eval::collect_pure_fns(ast) {
        tracing::info!("[fn-seed] pure fn '{name}'");
        timeline.env.set(
            &name,
            Value::UserFn {
                name: name.clone(),
                params: std::sync::Arc::new(params),
                body: std::sync::Arc::new(body),
            },
        );
    }
    timeline.apply_colorscheme(BuiltInColorscheme::DefaultDark.resolved());
    // Seed build-time environment with scene dimensions so `let` declarations
    // can reference `scene_width` / `scene_height`.
    let default_dims = SceneDimensions::default();
    timeline.env.set("scene_width", Value::Num(default_dims.width as f64));
    timeline.env.set("scene_height", Value::Num(default_dims.height as f64));

    timeline.load_colorscheme_declarations(ast, diagnostics);

    // Seed environment with namespace exports, recursing into nested aliases.
    for (alias, namespace) in namespaces {
        seed_namespace_exports(timeline, diagnostics, alias, namespace);
    }

    for stmt in ast {
        if let Stmt::Config { settings, .. } = stmt {
            timeline.apply_config_settings(settings, diagnostics);
        }
    }

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
                timeline.env.set(&define.name, value);
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

    // Pre-pass: collect speech transcripts from Audio/Captions declarations.
    // The map is a build-time buffer: it lowers speech-anchored keyframe
    // times below, then each transcript is attached to its actor's entity
    // after the declaration walk created it.
    let mut transcripts = std::collections::HashMap::new();
    super::speech::collect_transcripts_from_ast(
        ast,
        &timeline.asset_cache,
        &mut transcripts,
        diagnostics,
    );

    // Pre-pass: lower any speech-anchored keyframe times in the AST
    let ast = super::speech::lower_speech_keyframes(ast, &transcripts, diagnostics);

    SeededBuild { ast, transcripts }
}
