//! Per-node behavior evaluation: the primitive `evaluate` dispatch and its
//! render-failure diagnostic.
//!
//! Split out of `render_actor_node_impl`; the evaluation order is unchanged.

use super::*;

impl Timeline {
    /// Evaluate the node's behavior into draw commands.
    ///
    /// `Some(commands)` draws the commands and records a hit region; `None`
    /// means "no drawable content" — nothing is drawn and no hit region or
    /// precise bounds are recorded for it.
    pub(super) fn evaluate_behavior_commands(
        &self,
        node_id: EntityId,
        node_label: &str,
        time_ms: u64,
        local_transform: kurbo::Affine,
        opacity: f32,
        scene_dimensions: SceneDimensions,
        node_overrides: Option<&std::collections::HashMap<String, Value>>,
        vector_paths: &[VelloPath],
    ) -> Option<Vec<crate::primitives::RenderCommand>> {
        // ── Behavior evaluation ──
        // The behavior's evaluate() is the direct render path.
        // `Some(commands)` draws the commands and records a hit region;
        // `None` means "no drawable content" — nothing is drawn and no
        // hit region or precise bounds are recorded for it.
        let primitive_dispatch = self.behavior_for_node(node_id, time_ms).and_then(|def| {
            if def.caps.is_shape {
                self.world.begin_shape_commands(node_id);
            }
            let ctx = crate::primitives::EvaluateCtx {
                id: node_id,
                label: node_label,
                time_ms,
                local_transform,
                opacity,
                scene_dimensions,
                background_color: self.eval_caches.background_color.get(),
                overrides: node_overrides,
                vector_paths,
                asset_cache: &self.asset_cache,
                target_resolver: Some(self),
                legend: self.world.legend(node_id),
                world: Some(&self.world),
                highlight: self.world.highlight(node_id),
                style: self.world.style(node_id),
                shape: self.world.shape(node_id),
                geometry: self.world.geometry(node_id),
                text: self.world.text(node_id),
                property_plan: self.world.property_plan(node_id),
                caps: def.caps,
                payload: None,
            };
            let mut text_ctx = crate::primitives::TextCompileCtx {
                text_compiler: &mut self.text_compiler.borrow_mut(),
                font_context: self.font_context.as_ref(),
            };
            match def.evaluate(&ctx, Some(&mut text_ctx)) {
                Ok(commands) => commands,
                Err(e) => {
                    self.eval_caches.runtime_diagnostics.borrow_mut().push(
                        crate::diagnostics::Diagnostic::error(
                            crate::diagnostics::DiagnosticCode::RenderFailure,
                            crate::diagnostics::DiagnosticPhase::Render,
                            format!("failed to evaluate '{node_label}' at t={time_ms}ms: {e}"),
                        ),
                    );
                    None
                },
            }
        });
        primitive_dispatch
    }
}
