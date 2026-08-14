//! Build entry points: AST-to-Timeline lowering pass infrastructure.
//!
//! Public entry points: `build()`, `build_with_font_context()`,
//! `build_with_diagnostics()`, `build_with_diagnostics_and_font_context()`.

use tracing::instrument;

use super::*;
use crate::timeline::layout::ChildExtent;

impl Timeline {
    pub(crate) fn register_container_metadata_and_apply_layout(
        &mut self,
        label: &str,
        container_ty: &str,
        time_ms: u64,
        gap: [f32; 2],
        padding: [f32; 4],
        align: Option<&str>,
        cols: Option<usize>,
        diagnostics: &mut Vec<Diagnostic>,
        vertical_align: Option<&str>,
    ) {
        let child_order = self.tracks.get(label).map(|t| t.children.clone()).unwrap_or_default();
        // layout_children is computed on demand via Timeline::layout_children_for
        let _layout_children =
            self.build_layout_children(label, container_ty, &child_order, diagnostics);

        let metadata = ContainerMetadata {
            layout_type: LayoutType::from_container_ty(container_ty),
            gap,
            padding,
            align: align.unwrap_or("center").to_string(),
            vertical_align: vertical_align.unwrap_or("center").to_string(),
            cols,
            child_order: child_order.clone(),
        };

        self.container_metadata.insert(label.to_string(), metadata.clone());

        // ─── Two-pass width propagation ───
        // Pass 1: Apply layout to get container size from Taffy.
        // This gives us the container's total width/height determined by children.
        self.apply_container_layout(label, time_ms as f64, diagnostics);

        // Compute container size for width propagation.
        // The container_size comes from the Taffy layout we just computed.
        let child_extents: Vec<ChildExtent> = child_order
            .iter()
            .filter_map(|cl| {
                let track = self.tracks.get(cl)?;
                // Use layout_size_last() to get the most recently keyframed value,
                // consistent with apply_container_layout().
                let half_size = track.layout_size_last()?;
                Some(ChildExtent {
                    label: cl.clone(),
                    half_size,
                    placement_mode: track
                        .geometry
                        .placement_mode
                        .last(crate::timeline::PlacementMode::LayoutManaged),
                })
            })
            .collect();

        let container_size =
            crate::timeline::layout::compute_container_size(&child_extents, &metadata);

        tracing::debug!(
            "Width propagation: {} '{}' container_size={:?}, {} children",
            container_ty,
            label,
            container_size,
            child_extents.len()
        );

        // Propagate container width to text children
        self.propagate_text_child_widths(label, container_size, time_ms, diagnostics);

        // Pass 2: Re-apply layout with updated (wrapped) child sizes
        // This ensures positions account for the new wrapped text dimensions.
        self.apply_container_layout(label, time_ms as f64, diagnostics);
    }

    /// Build a `Timeline` from an AST with the default font context and
    /// production quality.
    pub fn build(ast: &[Stmt]) -> Self {
        Self::build_with_font_context(
            ast,
            std::sync::Arc::new(crate::renderer::text::FontContext::new()),
        )
    }

    /// Build a `Timeline` from an AST with a custom font context.
    pub fn build_with_font_context(
        ast: &[Stmt],
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    ) -> Self {
        Self::build_with_diagnostics_and_font_context(
            ast,
            &std::collections::HashMap::new(),
            font_context,
            super::BuildQuality::Production,
        )
        .output
    }

    /// Build a `Timeline` from an AST, collecting diagnostics and using the
    /// default font context.
    #[instrument(skip(ast, namespaces), fields(ast_statements = ast.len()))]
    pub fn build_with_diagnostics(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
    ) -> BuildReport<Self> {
        Self::build_with_diagnostics_and_font_context(
            ast,
            namespaces,
            std::sync::Arc::new(crate::renderer::text::FontContext::new()),
            super::BuildQuality::Production,
        )
    }

    /// Build a `Timeline` from an AST with full control over diagnostics,
    /// font context, and build quality.
    #[instrument(skip(ast, namespaces, font_context), fields(ast_statements = ast.len()))]
    pub fn build_with_diagnostics_and_font_context(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
    ) -> BuildReport<Self> {
        Self::build_impl(ast, namespaces, font_context, build_quality, None)
    }

    /// Build a `Timeline` with a pre-seeded carry bag injected before
    /// statement processing.
    ///
    /// Used by the multi-scene composition engine to thread actor state
    /// from a predecessor scene into the current scene.
    ///
    /// `carry` — optional carry bag from the predecessor scene.  When `None`
    /// this is identical to [`build_with_diagnostics_and_font_context`].
    ///
    /// `source_timeline` — the predecessor timeline, used to resolve
    /// layout-managed world positions (Phase 3 re-rooting).
    ///
    /// `source_duration_ms` — duration of the predecessor scene in ms.
    ///
    /// `dims` — scene pixel dimensions `[width, height]`.
    pub fn build_with_carry(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
        carry: Option<&crate::timeline::persistence::CarryBag>,
        source_timeline: Option<&Timeline>,
        source_duration_ms: u64,
        dims: [f64; 2],
    ) -> BuildReport<Self> {
        let carry_params =
            carry.zip(source_timeline).map(|(c, s)| (c, s, source_duration_ms, dims));
        Self::build_impl(ast, namespaces, font_context, build_quality, carry_params)
    }

    /// Internal build implementation shared by all public build entry points.
    fn build_impl(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
        build_quality: super::BuildQuality,
        carry: Option<(&crate::timeline::persistence::CarryBag, &Timeline, u64, [f64; 2])>,
    ) -> BuildReport<Self> {
        // Clear expression evaluation cache at the start of each build.
        crate::timeline::utils::clear_eval_cache();

        let mut timeline = Self::new_with_font_context(font_context);
        timeline.build_quality = build_quality;
        load_standard_library(&mut timeline.env);
        timeline.apply_colorscheme(BuiltInColorscheme::DefaultDark.resolved());
        // Seed build-time environment with scene dimensions so `let` declarations
        // can reference `scene_width` / `scene_height`.
        let default_dims = super::SceneDimensions::default();
        timeline.env.set("scene_width", super::Value::Num(default_dims.width as f64));
        timeline.env.set("scene_height", super::Value::Num(default_dims.height as f64));
        let mut current_build_time_ms = 0.0;
        let mut diagnostics = Vec::new();

        timeline.load_colorscheme_declarations(ast, &mut diagnostics);

        // Seed environment with namespace exports, recursing into nested aliases.
        for (alias, namespace) in namespaces {
            seed_namespace_exports(&mut timeline, &mut diagnostics, alias, namespace);
        }

        for stmt in ast {
            if let Stmt::Config { settings, .. } = stmt {
                timeline.apply_config_settings(settings, &mut diagnostics);
            }
        }

        // Inject carry bag (if any) BEFORE statement processing so that
        // carried actors are visible to re-declarations and assignments.
        if let Some((carry, source_tl, source_dur_ms, dims)) = carry {
            timeline.inject_carry_bag(carry, source_tl, source_dur_ms, dims, &mut diagnostics);
        }

        let mut has_seen_keyframe = false;
        for stmt in ast {
            match stmt {
                Stmt::Config { .. } => {},
                Stmt::Keyframe { time, body, .. } => {
                    has_seen_keyframe = true;
                    timeline.default_opacity = 1.0;
                    current_build_time_ms = time_to_ms(time);
                    timeline.process_body(current_build_time_ms, body, None, &mut diagnostics);
                },
                Stmt::RelativeKeyframe { offset, body, .. } => {
                    has_seen_keyframe = true;
                    timeline.default_opacity = 1.0;
                    current_build_time_ms += time_to_ms(offset);
                    timeline.process_body(current_build_time_ms, body, None, &mut diagnostics);
                },
                Stmt::ActorDecl { .. }
                | Stmt::Assignment { .. }
                | Stmt::Sequence { .. }
                | Stmt::Stagger { .. }
                | Stmt::LetDecl { .. }
                | Stmt::Always { .. }
                | Stmt::ForLoop { .. } => {
                    let saved_opacity = timeline.default_opacity;
                    if !has_seen_keyframe {
                        timeline.default_opacity = 0.0;
                    }
                    timeline.process_body(
                        current_build_time_ms,
                        std::slice::from_ref(stmt),
                        None,
                        &mut diagnostics,
                    );
                    timeline.default_opacity = saved_opacity;
                },
                _ => {}, // Config/import/type aliases are handled by earlier phases
            }
        }

        // Compute modifier hash for cross-rebuild IR caching.
        //
        // We hash the Debug representation because the AST types cannot derive
        // `Hash` (several variants hold `f64`/`f32`, which do not implement
        // `Hash`). This is safe in practice: the hash is only compared within a
        // single process/session and is never persisted, Debug output is a
        // faithful rendering of every field, and a changed hash merely causes
        // an unnecessary re-lower rather than incorrect cache reuse.
        {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for stmt in &timeline.modifiers {
                format!("{:?}", stmt).hash(&mut hasher);
            }
            timeline.modifier_hash = hasher.finish();
        }

        // Lower always-body statements into IR for frame-time interpretation.
        // Skip lowering when no modifiers exist to avoid empty programs.
        if !timeline.modifiers.is_empty() {
            match crate::timeline::modifier_runtime::ir::lower_modifier_body(&timeline.modifiers) {
                Ok(program) => {
                    timeline.modifier_programs.push(program);
                },
                Err(e) => {
                    diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::ModifierCompilationError,
                        DiagnosticPhase::Build,
                        format!(
                            "Always-block lowering failed: {e}. Modifier execution will be skipped."
                        ),
                    ));
                },
            }
        }

        // Check for always-blocks overriding keyframed properties.
        for stmt in &timeline.modifiers {
            if let crate::ast::Stmt::Assignment {
                target,
                property,
                span,
                ..
            } = stmt
            {
                let actor_key = crate::timeline::assignment_target_key(target);
                if let Some(track) = timeline.tracks.get(&actor_key) {
                    if track.has_keyframes_for(property) {
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
            }
        }

        // P2.22: Freeze the base environment into an Arc for cheap sharing.
        // After build, env is stable; build_frame_env will reference this Arc
        // instead of copying all entries.
        timeline.env_base = std::sync::Arc::new(std::mem::take(&mut timeline.env.overrides));
        // Restore env with the frozen base layer
        timeline.env.base = Some(std::sync::Arc::clone(&timeline.env_base));

        // Validate Callout `target` references after all actors are built.
        // This is a post-build pass so forward declarations are visible.
        for (label, track) in &timeline.tracks {
            if track.kind == crate::timeline::ActorKindId::Callout {
                use crate::timeline::TrackAccessor;
                let target = track.geometry.callout_target.get(0, String::new());
                if !target.is_empty() && !timeline.tracks.contains_key(&target) {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::CalloutTargetNotFound,
                            DiagnosticPhase::Build,
                            format!(
                                "Callout '{}' has target '{}', but no actor with that label exists.",
                                label, target
                            ),
                        )
                        .with_subject(label),
                    );
                }
            }
        }

        // Populate Legend entries after every actor is built so generated and
        // forward-declared actors participate in the same scan.
        let legend_labels = timeline
            .tracks
            .iter()
            .filter(|(_, track)| track.kind == crate::timeline::ActorKindId::Legend)
            .map(|(label, _)| label.clone())
            .collect::<Vec<_>>();
        if !legend_labels.is_empty() {
            let entries = crate::timeline::legend::scan_legend_entries(
                &timeline.tracks,
                &timeline.root_nodes,
            );
            for label in legend_labels {
                if let Some(track) = timeline.tracks.get_mut(&label) {
                    track.legend.entries = entries.clone();
                }
            }
        }

        BuildReport::new(timeline, diagnostics)
    }
}

fn seed_namespace_exports(
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    prefix: &str,
    namespace: &crate::module::Namespace,
) {
    for (name, expr) in &namespace.exports {
        let key = format!("{prefix}.{name}");
        match evaluate_expr(expr, &timeline.env) {
            Ok(value) => {
                timeline.env.set(&key, value);
            },
            Err(e) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::ModuleExportEvalError,
                        DiagnosticPhase::Build,
                        format!("Failed to evaluate export '{key}': {e}; using default."),
                    )
                    .with_subject(&key),
                );
            },
        }
    }

    for (alias, nested) in &namespace.namespaces {
        seed_namespace_exports(timeline, diagnostics, &format!("{prefix}.{alias}"), nested);
    }
}
