//! Post-walk validation passes.
//!
//! Each pass only reads the lowered timeline (except the scene-statistics bake)
//! and emits warnings/errors for states that would otherwise fail silently at
//! playback: actors nothing ever reveals, a declared duration shorter than the
//! content, a loop seam that jumps, unregistered primitives, and `always`
//! writes that cannot land where the author expects.

use super::*;

/// Reject configured runtime inputs used in expressions that are consumed
/// while lowering scene structure. Hot inputs can change frame-time values,
/// but they cannot add/remove actors, choose a build-time branch, or resize the
/// authored canvas after compilation.
pub(super) fn validate_runtime_input_uses(
    ast: &[Stmt],
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if timeline.runtime_input_names.is_empty() {
        return;
    }
    let configured: std::collections::HashSet<String> =
        timeline.runtime_input_names.iter().cloned().collect();
    let mut invalid: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    scan_runtime_input_stmts(ast, &configured, &mut invalid);
    for (name, context) in &invalid {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::InvalidConfigValue,
                DiagnosticPhase::Build,
                format!(
                    "Runtime input '{name}' is used by {context}, which is fixed at build time; hot inputs can only drive frame-time values such as `always` expressions and plot functions."
                ),
            )
            .with_subject(name),
        );
    }
    timeline.runtime_input_names.retain(|name| !invalid.contains_key(name));
}

/// Reject names that resolve to a non-numeric build value or actor label.
/// Standard-library functions are already present during config lowering;
/// this post-walk check catches values and labels declared later in the file.
pub(super) fn validate_runtime_input_values(
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if timeline.runtime_input_names.is_empty() {
        return;
    }
    let mut invalid = std::collections::BTreeMap::new();
    for name in &timeline.runtime_input_names {
        if timeline.world.get_id(name).is_some() {
            invalid.insert(name.clone(), "actor label".to_string());
            continue;
        }
        if let Some(value) = timeline.env.get_ref(name)
            && !matches!(value, Value::Num(_))
        {
            invalid.insert(name.clone(), format!("build-time value {value:?}"));
            continue;
        }
        if let Some(track) = timeline.variable_tracks.get(name)
            && let Some(value) =
                track.keyframes.values().find(|value| !matches!(value, Value::Num(_)))
        {
            invalid.insert(name.clone(), format!("non-numeric variable track value {value:?}"));
        }
    }
    for (name, source) in &invalid {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::InvalidConfigValue,
                DiagnosticPhase::Build,
                format!(
                    "Runtime input '{name}' conflicts with {source}; runtime inputs can only override numeric variables."
                ),
            )
            .with_subject(name),
        );
    }
    timeline.runtime_input_names.retain(|name| !invalid.contains_key(name));
    super::walk::refresh_frame_written_vars(timeline);
    timeline.invalidate_frame_cache();
}

fn scan_runtime_input_expr(
    expr: &Expr,
    configured: &std::collections::HashSet<String>,
    context: &str,
    invalid: &mut std::collections::BTreeMap<String, String>,
) {
    fn visit(
        expr: &Expr,
        configured: &std::collections::HashSet<String>,
        shadowed: &std::collections::HashSet<String>,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        match expr {
            Expr::Ident(name) => {
                if configured.contains(name) && !shadowed.contains(name) {
                    out.insert(name.clone());
                }
            },
            Expr::Path(parts) => {
                if parts
                    .first()
                    .is_some_and(|name| configured.contains(name) && !shadowed.contains(name))
                {
                    out.insert(parts[0].clone());
                }
            },
            Expr::Index(container, index) => {
                visit(container, configured, shadowed, out);
                visit(index, configured, shadowed, out);
            },
            Expr::Tuple(items) | Expr::List(items) => {
                for item in items {
                    visit(item, configured, shadowed, out);
                }
            },
            Expr::Binary(left, _, right) => {
                visit(left, configured, shadowed, out);
                visit(right, configured, shadowed, out);
            },
            Expr::Unary(_, value) => visit(value, configured, shadowed, out),
            Expr::Call(callee, args) => {
                visit(callee, configured, shadowed, out);
                for arg in args {
                    visit(&arg.value, configured, shadowed, out);
                }
            },
            Expr::Field(receiver, _) => visit(receiver, configured, shadowed, out),
            Expr::Timeline(plan) | Expr::Sequence(plan) => {
                let active = configured.difference(shadowed).cloned().collect();
                let mut invalid = std::collections::BTreeMap::new();
                scan_runtime_input_stmts(&plan.statements, &active, &mut invalid);
                out.extend(invalid.into_keys());
            },
            Expr::Stagger { modifiers, plan } => {
                for modifier in modifiers {
                    visit(&modifier.value, configured, shadowed, out);
                }
                let active = configured.difference(shadowed).cloned().collect();
                let mut invalid = std::collections::BTreeMap::new();
                scan_runtime_input_stmts(&plan.statements, &active, &mut invalid);
                out.extend(invalid.into_keys());
            },
            // Closures are stored now and their bodies run only when invoked.
            // That is the intended dynamic use of a runtime input.
            Expr::Closure(_, _) => {},
            Expr::LetChain(bindings, tail) => {
                let mut local_names = shadowed.clone();
                for (name, value) in bindings {
                    visit(value, configured, &local_names, out);
                    // The declaration shadows the same name only in following
                    // bindings and the tail of this lexical chain.
                    local_names.insert(name.clone());
                }
                visit(tail, configured, &local_names, out);
            },
            Expr::Conditional(condition, then_value, else_value) => {
                visit(condition, configured, shadowed, out);
                visit(then_value, configured, shadowed, out);
                visit(else_value, configured, shadowed, out);
            },
            Expr::Match(scrutinee, arms) => {
                visit(scrutinee, configured, shadowed, out);
                for (_, arm) in arms {
                    visit(arm, configured, shadowed, out);
                }
            },
            Expr::Construct(_, props) => {
                for prop in props {
                    visit(&prop.value, configured, shadowed, out);
                }
            },
            Expr::Num(_)
            | Expr::Percent(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::Null
            | Expr::Duration(_)
            | Expr::Unit
            | Expr::QualifiedPath(_) => {},
        }
    }

    let mut used = std::collections::BTreeSet::new();
    visit(expr, configured, &std::collections::HashSet::new(), &mut used);
    for name in used {
        invalid.entry(name).or_insert_with(|| context.to_string());
    }
}

fn scan_runtime_input_props(
    props: &[crate::ast::Property],
    context: &str,
    configured: &std::collections::HashSet<String>,
    invalid: &mut std::collections::BTreeMap<String, String>,
) {
    for prop in props {
        scan_runtime_input_expr(
            &prop.value,
            configured,
            &format!("property `{context}.{}`", prop.name),
            invalid,
        );
    }
}

fn scan_runtime_input_modifiers(
    modifiers: &[crate::ast::Modifier],
    owner: &str,
    configured: &std::collections::HashSet<String>,
    invalid: &mut std::collections::BTreeMap<String, String>,
) {
    for modifier in modifiers {
        let name = modifier.name.as_deref().unwrap_or("duration");
        scan_runtime_input_expr(
            &modifier.value,
            configured,
            &format!("{owner} modifier `{name}`"),
            invalid,
        );
    }
}

fn scan_runtime_input_inline_items(
    items: &[crate::ast::InlineItem],
    context: &str,
    configured: &std::collections::HashSet<String>,
    invalid: &mut std::collections::BTreeMap<String, String>,
) {
    for item in items {
        match item {
            crate::ast::InlineItem::Anonymous {
                ty,
                props,
                modifiers,
                children,
            } => {
                let label = format!("{context}.{ty}");
                scan_runtime_input_props(props, &label, configured, invalid);
                scan_runtime_input_modifiers(modifiers, &label, configured, invalid);
                scan_runtime_input_inline_items(children, &label, configured, invalid);
            },
            crate::ast::InlineItem::Labeled {
                label,
                array_index,
                props,
                modifiers,
                children,
                ..
            } => {
                if let Some(index) = array_index {
                    scan_runtime_input_expr(index, configured, "an actor array index", invalid);
                }
                let label = format!("{context}.{label}");
                scan_runtime_input_props(props, &label, configured, invalid);
                scan_runtime_input_modifiers(modifiers, &label, configured, invalid);
                scan_runtime_input_inline_items(children, &label, configured, invalid);
            },
            crate::ast::InlineItem::ForLoop {
                var,
                index_var,
                iterable,
                body,
            } => {
                scan_runtime_input_expr(
                    iterable,
                    configured,
                    "a container `for` iterable",
                    invalid,
                );
                let mut body_inputs = configured.clone();
                match var {
                    crate::ast::LoopPattern::Single(name) => {
                        body_inputs.remove(name);
                    },
                    crate::ast::LoopPattern::Tuple(names) => {
                        for name in names {
                            body_inputs.remove(name);
                        }
                    },
                }
                if let Some(index_var) = index_var {
                    body_inputs.remove(index_var);
                }
                scan_runtime_input_inline_items(body, context, &body_inputs, invalid);
            },
            crate::ast::InlineItem::SlotFill { items, .. } => {
                scan_runtime_input_inline_items(items, context, configured, invalid);
            },
            crate::ast::InlineItem::Expr(expr, _) => {
                scan_runtime_input_expr(expr, configured, "a container timeline", invalid)
            },
            crate::ast::InlineItem::SlotMarker => {},
        }
    }
}

fn scan_runtime_input_stmts(
    statements: &[Stmt],
    configured: &std::collections::HashSet<String>,
    invalid: &mut std::collections::BTreeMap<String, String>,
) {
    for stmt in statements {
        match stmt {
            Stmt::LetDecl { name, value, .. } => scan_runtime_input_expr(
                value,
                configured,
                &format!("build-time `let {name}`"),
                invalid,
            ),
            Stmt::ActorDecl {
                label,
                array_index,
                props,
                modifiers,
                children,
                ..
            } => {
                if let Some(index) = array_index {
                    scan_runtime_input_expr(index, configured, "an actor array index", invalid);
                }
                scan_runtime_input_props(props, label, configured, invalid);
                scan_runtime_input_modifiers(
                    modifiers,
                    &format!("actor `{label}`"),
                    configured,
                    invalid,
                );
                scan_runtime_input_inline_items(children, label, configured, invalid);
            },
            Stmt::Keyframe { time, body, .. }
            | Stmt::RelativeKeyframe {
                offset: time, body, ..
            } => {
                if let crate::ast::Time::Expression(expr) = time {
                    scan_runtime_input_expr(expr, configured, "a time marker", invalid);
                }
                scan_runtime_input_stmts(body, configured, invalid);
            },
            Stmt::Sequence { body, .. } => scan_runtime_input_stmts(body, configured, invalid),
            Stmt::Stagger {
                modifiers, body, ..
            } => {
                scan_runtime_input_modifiers(modifiers, "a `stagger` block", configured, invalid);
                scan_runtime_input_stmts(body, configured, invalid);
            },
            Stmt::Block { body, .. } => scan_runtime_input_stmts(body, configured, invalid),
            Stmt::Assignment {
                value,
                property,
                modifiers,
                ..
            } => {
                scan_runtime_input_expr(
                    value,
                    configured,
                    &format!("keyframe assignment `{property}`"),
                    invalid,
                );
                scan_runtime_input_modifiers(
                    modifiers,
                    &format!("keyframe assignment `{property}`"),
                    configured,
                    invalid,
                );
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                scan_runtime_input_expr(
                    condition,
                    configured,
                    "a build-time `if` condition",
                    invalid,
                );
                scan_runtime_input_stmts(then_branch, configured, invalid);
                if let Some(branch) = else_branch {
                    scan_runtime_input_stmts(branch, configured, invalid);
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                scan_runtime_input_expr(
                    scrutinee,
                    configured,
                    "a build-time `match` value",
                    invalid,
                );
                for (_, body) in arms {
                    scan_runtime_input_stmts(body, configured, invalid);
                }
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                modifiers,
                ..
            } => {
                scan_runtime_input_expr(
                    iterable,
                    configured,
                    "a build-time `for` iterable",
                    invalid,
                );
                scan_runtime_input_modifiers(
                    modifiers,
                    "a build-time `for` loop",
                    configured,
                    invalid,
                );
                let mut body_inputs = configured.clone();
                match var {
                    crate::ast::LoopPattern::Single(name) => {
                        body_inputs.remove(name);
                    },
                    crate::ast::LoopPattern::Tuple(names) => {
                        for name in names {
                            body_inputs.remove(name);
                        }
                    },
                }
                if let Some(index_var) = index_var {
                    body_inputs.remove(index_var);
                }
                scan_runtime_input_stmts(body, &body_inputs, invalid);
            },
            Stmt::Config { settings, .. } => {
                for setting in settings.iter().filter(|setting| setting.name != "runtime_inputs") {
                    scan_runtime_input_expr(
                        &setting.value,
                        configured,
                        &format!("config `{}`", setting.name),
                        invalid,
                    );
                }
            },
            Stmt::Scene { config, body, .. } => {
                scan_runtime_input_props(config, "scene config", configured, invalid);
                scan_runtime_input_stmts(body, configured, invalid);
            },
            // These bodies are lowered to frame-time programs or invoked when
            // a runtime expression calls them; they may safely read inputs.
            Stmt::Always { .. } | Stmt::ReactiveBinding { .. } | Stmt::FnDecl { .. } => {},
            Stmt::Expr(value, _) => scan_runtime_input_expr(
                value,
                configured,
                "a build-time timeline expression",
                invalid,
            ),
            Stmt::TypeAlias { .. }
            | Stmt::Import { .. }
            | Stmt::ComponentDef(..)
            | Stmt::Return { .. }
            | Stmt::Play { .. }
            | Stmt::Step { .. }
            | Stmt::Comment(..) => {},
        }
    }
}

/// Warn about actors that were seeded hidden and never revealed.
///
/// Tracks still flagged `hidden_by_default` at the end of the build were seeded
/// hidden and no entrance action (fade-in, wipe-in, ...) ever lifted them —
/// they will never be visible. The flag is maintained by
/// `lift_hidden_by_default`, which every entrance action routes through.
pub(super) fn validate_reveals(timeline: &Timeline, diagnostics: &mut Vec<Diagnostic>) {
    // Generated sub-actors (graph ticks, bar labels) render through
    // their parent primitive, so an ancestor's reveal counts as their
    // own. `parent_of` is derived from the children lists rather than
    // trusting the stored `parent` back-reference.
    // An ancestor counts as revealing only when its opacity genuinely
    // ramps 0 → positive (an entrance window). A declaration-time
    // constant keyframe — visible-by-default containers carry one — is
    // not a reveal: counting it suppressed warnings for hidden Graph
    // children, whose own seeded 0 multiplies the ancestor's opacity to
    // zero regardless (probe 010).
    let parent_of = timeline.parent_of();
    let opacity_reveals = |pt: &crate::timeline::PropertyTrack<f32>| -> bool {
        let mut saw_zero_at: Option<u64> = None;
        for (t, (v, _)) in pt.keyframes.iter() {
            if *v <= 0.0 {
                saw_zero_at.get_or_insert(*t);
            } else if saw_zero_at.is_some_and(|t0| *t > t0) {
                return true;
            }
        }
        false
    };
    let revealed_via_ancestor = |label: &str| -> bool {
        let mut current = parent_of.get(label).copied();
        while let Some(parent) = current {
            if let Some(pt) = timeline
                .world
                .get_id(parent)
                .and_then(|id| timeline.world.style(id).opacity.as_ref())
                && opacity_reveals(pt)
            {
                return true;
            }
            current = parent_of.get(parent).copied();
        }
        false
    };
    // Generated sub-actors (graph tick labels, bar labels) render through
    // their parent primitive and are not individually targetable, so they
    // follow the parent's entrance by construction and never warn.
    let is_generated_sub = |label: &str| -> bool {
        label.contains("_tick_x_") || label.contains("_tick_y_") || label.contains("_bar_label_")
    };
    for (label, id) in timeline.world.tracks.iter() {
        if !timeline.world.meta.get(id).is_some_and(|m| m.hidden_by_default) {
            continue;
        }
        if is_generated_sub(label)
            || revealed_via_ancestor(label)
            || timeline.world.style(id).opacity.as_ref().is_some_and(opacity_reveals)
        {
            continue;
        }
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::NeverRevealed,
                DiagnosticPhase::Build,
                format!(
                    "Actor `{label}` is declared before any keyframe (implicit `opacity: 0.0`) and \
                     is never revealed by an entrance action or opacity assignment, so it will \
                     remain invisible throughout playback. Add an entrance action (e.g. \
                     `fade_in(target: {label}, duration: 300ms)`), declare it inside a keyframe block, or set `opacity: 1.0` \
                     explicitly on its declaration."
                ),
            )
            .with_subject(label)
            .with_ast_span(timeline.world.decl_span(id).copied()),
        );
    }
}

/// Bake scene statistics, then check the declared duration and the loop seam.
///
/// A declared duration overrides the keyframe-inferred length, so anything
/// authored past it is never played. That truncation is the documented
/// contract (`docs/spec.md`, "overrides keyframe-inferred duration") and the
/// composition path has always done it per scene, but doing it silently would
/// strand authored animation with no feedback — say so instead.
pub(super) fn validate_duration_and_seam(
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(declared_s) = timeline.declared_duration_s {
        let content_s = timeline.duration_seconds();
        if content_s > declared_s {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::DurationShorterThanContent,
                    DiagnosticPhase::Build,
                    format!(
                        "`config {{ duration: {declared_s}s }}` is shorter than this \
                         timeline's last keyframe at {content_s:.2}s. A declared duration \
                         overrides the inferred one, so everything after {declared_s}s is \
                         never played — raise the duration or move the animation earlier."
                    ),
                )
                .with_subject("duration"),
            );
        }
    }

    // Bake scene statistics before the loop seam check and before env_base is
    // frozen.
    if timeline.stats_used {
        crate::timeline::scene_stats::bake_scene_stats(timeline);
    }

    // Loop seam check. A scene that declares `config { seamless_loop: true }`
    // is replayed end-to-start, so any keyframed value that differs between
    // the first and last frame shows up as a jump every cycle — the single
    // most common way a "looping" scene fails to loop.
    //
    // v1 scope, deliberately: only *keyframed* values are sampled. What an
    // `always` block computes, and a plot `func` transition, are frame-time
    // functions rather than tracks, so the build cannot see them and does
    // not lint them (a sine of the frame time wraps by construction when the
    // period divides the scene; only the author can know that).
    if !timeline.seamless_loop {
        return;
    }
    let end_ms = (timeline.duration_seconds() * 1000.0).round() as u64;
    if end_ms == 0 {
        return;
    }
    let mut broken: Vec<String> = Vec::new();
    for (label, id) in timeline.world.tracks.iter() {
        for idx in crate::timeline::property_registry::allowed_property_indices(
            &timeline.world.caps(id),
            timeline.world.actor_type(id),
        ) {
            let schema = &crate::timeline::property_registry::PROPERTY_REGISTRY[idx];
            let field =
                schema.field_for_actor(&timeline.world.caps(id), timeline.world.actor_type(id));
            if !timeline.world.property_has_keyframes(id, field) {
                continue;
            }
            let first = timeline.world.read_property_value(id, field, 0);
            let last = timeline.world.read_property_value(id, field, end_ms);
            let wraps = match (&first, &last) {
                (Some(a), Some(b)) => crate::timeline::values_wrap(a, b),
                // One side never sampled: the property is constant
                // across the scene, which wraps trivially.
                _ => true,
            };
            if !wraps {
                broken.push(format!("`{label}.{}`", schema.name));
            }
        }
    }
    // The camera is not a track, so the actor walk above cannot see
    // it — and a scene that ends pushed in and restarts unzoomed
    // jumps at the seam exactly like any other un-wrapped value.
    if timeline.camera.is_authored() {
        for (axis, first, last) in timeline.camera.seam_pairs(0, end_ms) {
            if !crate::timeline::values_wrap(&first, &last) {
                broken.push(format!("`{axis}`"));
            }
        }
    }
    // Baked scene statistics are data, so check that they wrap cleanly.
    if timeline.stats_used {
        for key in [
            "scene.stats.motion",
            "scene.stats.ink",
            "scene.stats.focus_x",
            "scene.stats.focus_y",
            "scene.stats.spread_x",
            "scene.stats.spread_y",
            "scene.stats.cast",
        ] {
            if let Some(val) = timeline.env.get(key) {
                let first = crate::timeline::eval_shared::eval_builtin_fn(
                    "curve_at",
                    &[val.clone(), Value::Num(0.0)],
                );
                let last = crate::timeline::eval_shared::eval_builtin_fn(
                    "curve_at",
                    &[val.clone(), Value::Num(timeline.duration_seconds())],
                );
                if let (Ok(f), Ok(l)) = (first, last)
                    && (f.as_num() - l.as_num()).abs() > 1e-3
                {
                    broken.push(format!("`{key}`"));
                }
            }
        }
    }
    if broken.is_empty() {
        return;
    }
    let shown = if broken.len() > 6 {
        format!("{} ({} in total)", broken[..6].join(", "), broken.len())
    } else {
        broken.join(", ")
    };
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::LoopNotSeamless,
            DiagnosticPhase::Build,
            format!(
                "`config {{ seamless_loop: true }}` but these keyframed values \
                 differ between the first frame and the last ({shown}), so the \
                 replay jumps at the seam. Give each one the same value at both \
                 ends, or drop the declaration."
            ),
        )
        .with_subject("seamless_loop"),
    );
}

/// Check primitive registration and `always` writes that cannot take effect.
///
/// Every track's primitive identity must resolve: identity is required when the
/// entity is spawned, so a miss means the declared primitive is not registered
/// (typically an extension whose plugin is not loaded). Warn once here rather
/// than silently rendering nothing every frame.
///
/// The `always` half warns for a per-frame write that the modifier runtime has
/// nowhere to put (`solo` is assignable but deliberately not animatable) and
/// for a write over a property that also has keyframes.
pub(super) fn validate_actors(timeline: &mut Timeline, diagnostics: &mut Vec<Diagnostic>) {
    for (label, id) in timeline.world.tracks.iter() {
        let actor_type = timeline.world.actor_type(id);
        if timeline.primitive_registry.find(actor_type).is_none() {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownActorType,
                    DiagnosticPhase::Build,
                    format!(
                        "Actor `{label}` is type `{actor_type}`, which is not a registered \
                         primitive; it will not render. Load the extension that provides it."
                    ),
                )
                .with_subject(label),
            );
        }
    }

    // Include writes in conditional and loop bodies as well as direct statements.
    animatix_syntax::walk::walk_stmts(&timeline.modifiers, &mut |stmt| {
        if let crate::ast::Stmt::Assignment {
            target,
            property,
            span,
            value_span,
            ..
        } = stmt
        {
            let actor_key = crate::timeline::assignment_target_key(target);
            let is_indexed = target.iter().any(|segment| segment.as_static_str().is_none());
            let id = if is_indexed {
                None
            } else {
                timeline.world.get_id(&actor_key)
            };
            let schema = crate::timeline::property_registry::lookup_property(property);
            let unsupported = schema.and_then(|schema| {
                if let Some(id) = id {
                    let actor_type = timeline.world.actor_type(id);
                    return (!schema.supports_frame_override(actor_type)).then_some(actor_type);
                }
                if is_indexed && !schema.supports_frame_override("") {
                    return indexed_write_actor_type(&timeline.world, target);
                }
                None
            });
            if let Some(actor_type) = unsupported {
                let authored_target = animatix_syntax::format_core::format_target_segments(
                    target,
                    animatix_syntax::format_core::format_expr,
                );
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::AlwaysWriteNotAnimatable,
                        DiagnosticPhase::Build,
                        format!(
                            "Always block writes `{property}` on `{authored_target}` ({actor_type}), \
                             which does not support a per-frame override. Use a declaration or \
                             a keyframe operation where the property supports it."
                        ),
                    )
                    .with_subject(property)
                    .with_ast_span(*span)
                    .with_byte_span(*value_span),
                );
            } else if let Some(id) = id
                && timeline.world.is_property_animated(id, property)
            {
                // Only a property that actually *moves* is worth warning
                // about: the build seeds a constant keyframe for most
                // declared properties, and a frame-time write over a
                // value that never changes silently overrides nothing.
                diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::AlwaysOverridesKeyframes,
                            DiagnosticPhase::Build,
                            format!(
                                "Always block writes to `{}` on actor `{}`, which also has keyframe animation. The always value will silently override keyframes every frame.",
                                property, actor_key
                            ),
                        )
                        .with_subject(property)
                        .with_ast_span(*span),
                    );
            }
        }
    });
}

fn indexed_write_actor_type<'a>(
    world: &'a crate::timeline::World,
    target: &[crate::ast::TargetSegment],
) -> Option<&'a str> {
    use crate::ast::TargetSegment;

    world.tracks.iter().find_map(|(label, id)| {
        let mut parts = label.split('.');
        let matches_target = target.iter().all(|segment| match (segment, parts.next()) {
            (TargetSegment::Static(expected), Some(part)) => expected == part,
            (TargetSegment::Indexed { base, .. }, Some(part)) => {
                crate::ast::is_array_member_label(part) == Some(base.as_str())
            },
            (_, None) => false,
        });
        (matches_target && parts.next().is_none()).then(|| world.actor_type(id))
    })
}

#[cfg(test)]
mod runtime_input_structure_tests {
    use super::*;
    use crate::ast::{CallArgument, Expr, LoopPattern, Modifier, Stmt};

    fn input_modifier(name: &str) -> Modifier {
        Modifier {
            name: Some("duration".to_string()),
            value: Expr::Ident(name.to_string()),
        }
    }

    #[test]
    fn runtime_inputs_in_time_scopes_and_timing_modifiers_are_rejected() {
        let input_names = [
            "speed",
            "clip",
            "action_duration",
            "loop_step",
            "stagger_gap",
        ];
        let statements = vec![
            Stmt::Expr(
                Expr::Call(
                    Box::new(Expr::Ident("stretch".into())),
                    vec![
                        CallArgument::named(
                            "animation",
                            Expr::Timeline(crate::ast::PlanBody::new(Vec::new())),
                        ),
                        CallArgument::named("factor", Expr::Ident("speed".into())),
                    ],
                ),
                None,
            ),
            Stmt::Expr(
                Expr::Call(
                    Box::new(Expr::Ident("fit_duration".into())),
                    vec![
                        CallArgument::named(
                            "animation",
                            Expr::Timeline(crate::ast::PlanBody::new(Vec::new())),
                        ),
                        CallArgument::named("duration", Expr::Ident("clip".into())),
                    ],
                ),
                None,
            ),
            Stmt::Expr(
                Expr::Call(
                    Box::new(Expr::Ident("move".into())),
                    vec![
                        CallArgument::named("target", Expr::Ident("box".into())),
                        CallArgument::named("duration", Expr::Ident("action_duration".into())),
                    ],
                ),
                None,
            ),
            Stmt::ForLoop {
                var: LoopPattern::Single("item".to_string()),
                index_var: None,
                iterable: Expr::List(Vec::new()),
                body: Vec::new(),
                modifiers: vec![input_modifier("loop_step")],
                span: None,
            },
            Stmt::Stagger {
                modifiers: vec![input_modifier("stagger_gap")],
                body: Vec::new(),
                span: None,
            },
        ];

        let mut timeline = crate::timeline::Timeline::new();
        timeline.runtime_input_names = input_names.iter().map(|name| (*name).to_string()).collect();
        let mut diagnostics = Vec::new();

        validate_runtime_input_uses(&statements, &mut timeline, &mut diagnostics);

        assert!(timeline.runtime_input_names.is_empty());
        assert_eq!(diagnostics.len(), input_names.len());
        for name in input_names {
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.location.subject.as_deref() == Some(name)
                        && diagnostic.message.contains("fixed at build time")
                }),
                "expected a build-time diagnostic for runtime input `{name}`"
            );
        }
    }
}
