//! Per-node vector-path sampling: the shape tier's shared paths, `BarChart`
//! data transitions, and dynamic procedural plots.
//!
//! Split out of `render_actor_node_impl`; the sampling order is unchanged.

use super::*;

impl Timeline {
    /// Sample the node's vector paths for `time_ms`.
    ///
    /// The common static-track case returns a refcount bump on the shape
    /// tier's shared `Arc`; a `BarChart` data transition or a dynamic
    /// procedural plot replaces it with this frame's rebuild.
    pub(super) fn evaluate_node_vector_paths(
        &self,
        node_id: EntityId,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
        frame_env: Option<&crate::timeline::env::Environment>,
    ) -> std::sync::Arc<Vec<VelloPath>> {
        // Shared Arc — the common static-track case returns a refcount
        // bump instead of a full path-list clone per node per frame.
        let mut vector_paths = self
            .world
            .shape_or_default(node_id)
            .evaluate_vector_paths(self.world.style(node_id), time_ms);
        // `BarChart` data transitions: once a `data = {…}` assignment has begun,
        // the declaration's paths are stale, so rebuild the bars from the dataset
        // in effect. Gated on the side channel being non-empty, so a chart whose
        // data never animates pays nothing.
        if !self.world.bar_data_transitions(node_id).is_empty() {
            if let Some(layout) = self.world.bar_layout(node_id) {
                let data = crate::timeline::plot::bar_data_at(
                    self.world.bar_data(node_id),
                    self.world.bar_data_transitions(node_id),
                    time_ms,
                );
                vector_paths = std::sync::Arc::new(layout.paths_for(&data).0);
            }
        }
        // Re-sample procedural plots at frame time so they can reference `t`.
        // Use the shared frame_env if available; fall back to creating one on-demand
        // (should only happen when frame_env was created at top level).
        if let Some(procedural_plot) = self.world.procedural_plot(node_id) {
            // Guard: only resample per-frame when the plot is dynamic (references `t`
            // or has animated params) or a func transition is currently active.
            // Static, non-transitioning plots keep the cached build-time vector_paths.
            let transitioning = self
                .world
                .func_transitions(node_id)
                .iter()
                .any(|t| time_ms >= t.start_ms && time_ms <= t.end_ms);
            // Also use per-frame sampling once all transitions are complete so that
            // the final `to` function (not the original declaration) is rendered.
            let has_completed_transitions =
                self.world.func_transitions(node_id).iter().any(|t| t.is_complete_at(time_ms));

            if procedural_plot.is_dynamic(&self.frame_written_vars)
                || transitioning
                || has_completed_transitions
            {
                let fallback_env;
                let env = if let Some(env) = frame_env {
                    env
                } else {
                    fallback_env =
                        self.build_frame_env_internal(time_ms, scene_dimensions, overrides);
                    &fallback_env
                };

                // Inject plot parameter values from keyframe tracks into a
                // scoped let-chain overlay on the frame environment, so that
                // `sample_procedural_plot_at` sees animated values without
                // deep-cloning the entire Environment hash map.
                let mut param_bindings = Vec::new();
                for name in &procedural_plot.param_names {
                    if let Some(param_track) = self.world.plot_param_track(node_id, name) {
                        let val = param_track.evaluate(time_ms);
                        let num_val = crate::timeline::Value::Num(val);
                        // Set the dotted key (e.g. "curve.freq") for explicit references
                        let mut key = String::new();
                        crate::timeline::env_keys::property_into(
                            &procedural_plot.actor_label,
                            name,
                            &mut key,
                        );
                        param_bindings.push((key, num_val.clone()));
                        // Set the bare name (e.g. "freq") for closure captures,
                        // but don't shadow closure sample arguments.
                        if !procedural_plot.func_args.contains(name) {
                            param_bindings.push((name.clone(), num_val));
                        }
                    }
                }
                let has_params = !param_bindings.is_empty();
                if has_params {
                    env.push_let_scope(param_bindings);
                }

                vector_paths =
                    std::sync::Arc::new(crate::timeline::plot::sample_procedural_plot_at(
                        procedural_plot,
                        env,
                        time_ms,
                        self.world.func_transitions(node_id),
                    ));

                if has_params {
                    env.pop_let_scope();
                }
                // Surface silent sample failures once per actor per frame: a
                // closure whose evaluation fails renders NaN gaps with no
                // other trace (the sampler memoizes per-sample errors, so the
                // path itself is the only signal). PathElement scan is cheap
                // relative to sampling; one diagnostic per frame per actor
                // cannot spam the panel.
                // The path assembler drops NaN points entirely (they only
                // reset the pen), so a failed closure produces an EMPTY
                // BezPath inside a present VelloPath — that emptiness is the
                // observable signal. (Per-point NaN scans never see it.)
                let has_nan_gap = vector_paths
                    .iter()
                    .any(|vp| vp.path.elements().is_empty() && vp.stroke.is_some());
                if has_nan_gap {
                    self.eval_caches.runtime_diagnostics.borrow_mut().push(
                        crate::diagnostics::Diagnostic::warning(
                            crate::diagnostics::DiagnosticCode::RenderFailure,
                            crate::diagnostics::DiagnosticPhase::Render,
                            format!(
                                "plot '{}' produced non-finite samples at t={time_ms}ms; \
                                 the func closure likely failed — rendering gaps",
                                procedural_plot.actor_label
                            ),
                        ),
                    );
                }
            }
        }
        vector_paths
    }
}
