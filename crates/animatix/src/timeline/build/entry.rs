//! Build entry points: AST-to-Timeline lowering pass infrastructure.
//!
//! Public entry points: `build()`, `build_with_font_context()`,
//! `build_with_diagnostics()`, `build_with_diagnostics_and_font_context()`.

use super::*;
use tracing::instrument;

impl Timeline {
    pub(super) fn register_container_metadata_and_apply_layout(
        &mut self,
        label: &str,
        container_ty: &str,
        time_ms: u64,
        gap: f32,
        padding: f32,
        align: Option<&str>,
        cols: Option<usize>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let child_order = self
            .tracks
            .get(label)
            .map(|t| t.children.clone())
            .unwrap_or_default();
        // layout_children is computed on demand via Timeline::layout_children_for
        let _layout_children = self.build_layout_children(label, container_ty, &child_order, diagnostics);

        self.container_metadata.insert(
            label.to_string(),
            ContainerMetadata {
                layout_type: LayoutType::from_container_ty(container_ty),
                gap,
                padding,
                align: align.unwrap_or("center").to_string(),
                cols,
                child_order,
            },
        );

        self.apply_container_layout(label, time_ms as f64, diagnostics);
    }

    /// Build a `Timeline` from an AST with the default font context.
    pub fn build(ast: &[Stmt]) -> Self {
        Self::build_with_font_context(ast, crate::renderer::text::FontContext::new())
    }

    /// Build a `Timeline` from an AST with a custom font context.
    pub fn build_with_font_context(
        ast: &[Stmt],
        font_context: crate::renderer::text::FontContext,
    ) -> Self {
        Self::build_with_diagnostics_and_font_context(ast, &std::collections::HashMap::new(), font_context)
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
            crate::renderer::text::FontContext::new(),
        )
    }

    /// Build a `Timeline` from an AST with full control over diagnostics and
    /// font context.
    #[instrument(skip(ast, namespaces), fields(ast_statements = ast.len()))]
    pub fn build_with_diagnostics_and_font_context(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        font_context: crate::renderer::text::FontContext,
    ) -> BuildReport<Self> {
        let mut timeline = Self::new_with_font_context(font_context);
        load_standard_library(&mut timeline.env);
        timeline.apply_colorscheme(BuiltInColorscheme::DefaultDark.resolved());
        let mut current_build_time_ms = 0.0;
        let mut diagnostics = Vec::new();

        timeline.load_colorscheme_declarations(ast, &mut diagnostics);

        // Seed environment with namespace exports
        for (alias, namespace) in namespaces {
            for (name, expr) in &namespace.exports {
                let key = format!("{}.{}", alias, name);
                // Evaluate the export expression in the current env
                match evaluate_expr(expr, &timeline.env) {
                    Ok(value) => {
                        timeline.env.set(&key, value);
                    }
                    Err(e) => {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::ModuleExportEvalError,
                                DiagnosticPhase::Build,
                                format!(
                                    "Failed to evaluate export '{}.{}': {}; using default.",
                                    alias, name, e
                                ),
                            )
                            .with_subject(&key),
                        );
                    }
                }
            }
        }

        for stmt in ast {
            if let Stmt::Config { settings, .. } = stmt {
                timeline.apply_config_settings(settings, &mut diagnostics);
            }
        }

        for stmt in ast {
            match stmt {
                Stmt::Config { .. } => {}
                Stmt::Keyframe { time, body, .. } => {
                    current_build_time_ms = time_to_ms(time);
                    timeline.process_body(current_build_time_ms, body, None, &mut diagnostics);
                }
                Stmt::RelativeKeyframe { offset, body, .. } => {
                    current_build_time_ms += time_to_ms(offset);
                    timeline.process_body(current_build_time_ms, body, None, &mut diagnostics);
                }
                Stmt::ActorDecl { .. }
                | Stmt::Assignment { .. }
                | Stmt::Sequence { .. }
                | Stmt::Stagger { .. }
                | Stmt::LetDecl { .. } => {
                    timeline.process_body(
                        current_build_time_ms,
                        std::slice::from_ref(stmt),
                        None,
                        &mut diagnostics,
                    );
                }
                _ => {}
            }
        }

        // Compile always-body statements into IR for faster frame-time evaluation.
        // Skip compilation when no modifiers exist to avoid empty programs.
        if !timeline.modifiers.is_empty() {
            match crate::timeline::modifier_runtime::ir::lower_modifier_body(&timeline.modifiers) {
                Ok(program) => {
                    timeline.modifier_programs.push(program);
                    // Compile IR to bytecode for even faster execution
                    match crate::timeline::modifier_runtime::vm::compile_modifier_bytecode(
                        timeline.modifier_programs.last().expect("IR program just pushed above"),
                    ) {
                        Ok(bytecode) => {
                            timeline.modifier_bytecode_programs.push(bytecode);
                        }
                        Err(e) => {
                            diagnostics.push(Diagnostic::warning(
                                DiagnosticCode::ModifierCompilationError,
                                DiagnosticPhase::Build,
                                format!(
                                    "Bytecode compilation failed: {}. Using IR fallback.",
                                    e
                                ),
                            ));
                        }
                    }
                }
                Err(e) => {
                    // Fall back to AST interpretation for this batch
                    diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::ModifierCompilationError,
                    DiagnosticPhase::Build,
                    format!(
                        "Always-block optimization failed: {}. Animation will still work, but may be slower.",
                        e
                    ),
                ));
                }
            }
        }

        // P2.22: Freeze the base environment into an Arc for cheap sharing.
        // After build, env is stable; frame_eval_env will reference this Arc
        // instead of copying all entries.
        timeline.env_base = std::sync::Arc::new(
            std::mem::take(&mut timeline.env.overrides)
        );
        // Restore env with the frozen base layer
        timeline.env.base = Some(std::sync::Arc::clone(&timeline.env_base));

        BuildReport::new(timeline, diagnostics)
    }
}