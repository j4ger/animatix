//! Timeline/composition rebuilds, background rebuild application, and scene sizing.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use animatix::composition::BuildTarget;
use animatix::timeline::{SceneDimensions, TimelineIndex};
use animatix_syntax::ast::{Expr, Stmt};
use animatix_syntax::source_index::SourceIndex;

use super::DocumentSession;
use super::load::diagnostics_from_module_error;
use crate::error::GuiError;

pub(crate) type ModifierIrCache =
    std::sync::Arc<animatix::timeline::modifier_runtime::ir::ModifierIrCache>;

impl DocumentSession {
    /// Apply document/workspace base directories to every active asset cache.
    fn apply_asset_base_dirs(&mut self) {
        let document_dir = self.file_path.parent();
        let workspace_root = self.workspace_root.as_deref();
        if let Some(timeline) = &mut self.timeline {
            timeline.set_asset_base_dirs(document_dir, workspace_root);
        }
        if let Some(composition) = &mut self.composition {
            composition.set_asset_base_dirs(document_dir, workspace_root);
        }
    }

    pub fn rebuild(&mut self) -> Result<(), GuiError> {
        let asset_cache = self.asset_cache();
        self.rebuild_with_asset_cache(asset_cache)
    }

    pub(crate) fn modifier_ir_caches(&self) -> Vec<ModifierIrCache> {
        if let Some(timeline) = &self.timeline {
            return timeline.modifier_ir_cache().into_iter().collect();
        }
        self.composition
            .iter()
            .flat_map(|composition| composition.scenes.values())
            .filter_map(|scene| scene.timeline.modifier_ir_cache())
            .collect()
    }

    /// Rebuild using an asset cache carried from a previous build.
    pub fn rebuild_with_asset_cache(
        &mut self,
        asset_cache: Option<std::sync::Arc<animatix::timeline::assets::AssetCache>>,
    ) -> Result<(), GuiError> {
        self.rebuild_with_caches(asset_cache, &self.modifier_ir_caches())
    }

    pub(crate) fn rebuild_with_caches(
        &mut self,
        asset_cache: Option<std::sync::Arc<animatix::timeline::assets::AssetCache>>,
        modifier_ir_caches: &[ModifierIrCache],
    ) -> Result<(), GuiError> {
        // Skip rebuild if source text hasn't changed since last successful build.
        // A failure marker forces a retry even when the timeline survived, so
        // last-known-good state never suppresses the next rebuild.
        let mut hasher = DefaultHasher::new();
        self.source_text.hash(&mut hasher);
        let source_hash = hasher.finish();
        if source_hash == self.last_source_hash
            && self.last_rebuild_error.is_none()
            && (self.timeline.is_some() || self.composition.is_some())
        {
            return Ok(());
        }

        let result = match self.load_program() {
            Ok(result) => result,
            Err(err) => {
                let err_string = err.to_string();
                self.last_rebuild_error = Some(err_string.clone());
                self.raw_statements = None;
                self.expanded_statements = None;
                self.source_index = None;
                self.namespaces = HashMap::new();
                // Preserve last-known-good component registry so the GUI
                // stays usable while the user is editing.
                self.timeline = None;
                self.composition = None;
                self.active_scene = None;
                self.timeline_index = TimelineIndex::default();
                self.keyframe_lines = Vec::new();
                self.diagnostics = diagnostics_from_module_error(&err, &self.file_path);
                self.duration_s = 0.1;
                self.scene_dimensions = SceneDimensions::default();
                return Err(GuiError::Build {
                    message: err_string,
                });
            },
        };

        let expanded = match result.expanded {
            Ok(expanded) => expanded,
            Err(error) => {
                let message = error.to_string();
                self.diagnostics = result.type_diagnostics;
                self.diagnostics.extend(error.diagnostics);
                self.last_rebuild_error = Some(message.clone());
                return Err(GuiError::Build { message });
            },
        };

        if let Some(error) = result.type_diagnostics.iter().find(|diagnostic| diagnostic.is_error())
        {
            let message = error.message.clone();
            self.diagnostics = result.type_diagnostics;
            self.last_rebuild_error = Some(message.clone());
            return Err(GuiError::Build { message });
        }

        // Build source index from raw (non-expanded) statements
        let source_index = SourceIndex::build(&result.raw_statements);

        let report = BuildTarget::build_with_options(
            &expanded.statements,
            &result.namespaces,
            Some(&self.file_path),
            animatix::timeline::BuildOptions {
                quality: Some(animatix::timeline::BuildQuality::Draft),
                function_context: Some(&result.function_context),
                asset_cache,
                modifier_ir_caches,
                extensions: Some(std::sync::Arc::clone(&self.extension_context)),
                ..Default::default()
            }
            .with_expanded(&expanded),
        );
        let mut all_diagnostics = result.type_diagnostics;
        all_diagnostics.extend(report.diagnostics);
        if let Some(error) = all_diagnostics.iter().find(|diagnostic| {
            diagnostic.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error
        }) {
            let message = error.message.clone();
            self.diagnostics = all_diagnostics;
            self.last_rebuild_error = Some(message.clone());
            return Err(GuiError::Build { message });
        }
        self.last_rebuild_error = None;
        self.duration_s = report.output.duration_s().max(0.1);
        self.scene_dimensions = document_scene_dimensions(&expanded.statements);
        self.raw_statements = Some(result.raw_statements);
        self.expanded_statements = Some(expanded.statements);
        self.source_index = Some(source_index);
        self.namespaces = result.namespaces;
        self.components = result.components;
        self.module_fns = result.module_fns;
        self.diagnostics = all_diagnostics;
        // Carry the plot path cache across rebuilds. It is keyed by parameter
        // hash, so entries for unchanged plots survive and only edited
        // parameters are re-evaluated.
        let old_plot_cache = self
            .timeline
            .as_ref()
            .map(|t| t.plot_path_cache.clone())
            .or_else(|| {
                self.composition.as_ref().and_then(|c| {
                    c.scenes.values().next().map(|s| s.timeline.plot_path_cache.clone())
                })
            })
            .unwrap_or_default();

        match report.output {
            BuildTarget::SingleScene(mut timeline) => {
                timeline.plot_path_cache = old_plot_cache;
                self.timeline = Some(timeline);
                self.composition = None;
                self.active_scene = None;
            },
            BuildTarget::MultiScene(mut composition) => {
                if self
                    .active_scene
                    .as_ref()
                    .is_none_or(|scene| !composition.scenes.contains_key(scene))
                {
                    self.active_scene = composition.declaration_order.first().cloned();
                }
                for scene in composition.scenes.values_mut() {
                    scene.timeline.plot_path_cache.clone_from(&old_plot_cache);
                }
                self.timeline = None;
                self.composition = Some(composition);
            },
        }

        // Build timeline index from source text (bi-directional sync)
        self.timeline_index = TimelineIndex::build(&self.source_text);
        self.keyframe_lines = self.timeline_index.keyframes.iter().map(|(_, line)| *line).collect();

        // Relative asset URLs need the document/workspace base directories.
        self.apply_asset_base_dirs();

        // Update source hash after successful rebuild
        self.last_source_hash = source_hash;

        Ok(())
    }

    /// Apply a successful background rebuild output to this session.
    /// Carries forward caches (plot_path_cache, modifier programs) from the
    /// current session since the background worker started from a fresh session.
    pub fn apply_rebuild_output(
        &mut self,
        output: crate::app::document::rebuild_output::RebuildOutput,
        source_hash: u64,
    ) {
        // Carry the plot path cache across rebuilds. It is keyed by parameter
        // hash, so entries for unchanged plots survive and only edited
        // parameters are re-evaluated.
        let old_plot_cache = self
            .timeline
            .as_ref()
            .map(|t| t.plot_path_cache.clone())
            .or_else(|| {
                self.composition.as_ref().and_then(|c| {
                    c.scenes.values().next().map(|s| s.timeline.plot_path_cache.clone())
                })
            })
            .unwrap_or_default();

        self.last_rebuild_error = None;
        self.raw_statements = Some(output.raw_statements);
        self.expanded_statements = Some(output.expanded_statements);
        self.namespaces = output.namespaces;
        self.components = output.components;
        self.module_fns = output.module_fns;
        self.source_index = Some(output.source_index);
        self.diagnostics = output.diagnostics;
        self.timeline_index = output.timeline_index;
        self.keyframe_lines = output.keyframe_lines;
        self.duration_s = output.duration_s;
        self.scene_dimensions = output.scene_dimensions;
        self.last_source_hash = source_hash;

        // Apply caches and active_scene depending on single- vs multi-scene.
        if let Some(mut timeline) = output.timeline {
            timeline.plot_path_cache = old_plot_cache;
            self.timeline = Some(timeline);
            self.composition = None;
            self.active_scene = None;
        } else if let Some(mut composition) = output.composition {
            if self
                .active_scene
                .as_ref()
                .is_none_or(|scene| !composition.scenes.contains_key(scene))
            {
                self.active_scene = composition.declaration_order.first().cloned();
            }
            for scene in composition.scenes.values_mut() {
                scene.timeline.plot_path_cache.clone_from(&old_plot_cache);
            }
            self.timeline = None;
            self.composition = Some(composition);
        } else {
            self.timeline = None;
            self.composition = None;
            self.active_scene = None;
        }
        self.apply_asset_base_dirs();
    }

    /// Apply a failed background rebuild output to this session.
    ///
    /// Keeps the last-known-good AST, timeline, and source index so inspector
    /// edits remain available while the source is temporarily invalid. Only the
    /// failure/error state and diagnostics are replaced.
    pub fn apply_rebuild_failure(
        &mut self,
        failure: &crate::app::document::rebuild_output::RebuildFailure,
    ) {
        self.last_rebuild_error = Some(failure.error.clone());
        self.diagnostics = failure.diagnostics.clone();
        if self.source_index.is_none() {
            self.source_index = failure.partial_source_index.clone();
        }
    }

    /// Returns the asset cache carried by the current timeline or composition.
    pub fn asset_cache(&self) -> Option<std::sync::Arc<animatix::timeline::assets::AssetCache>> {
        self.timeline.as_ref().map(|timeline| timeline.asset_cache_arc()).or_else(|| {
            self.composition.as_ref().and_then(|composition| {
                composition.scenes.values().next().map(|scene| scene.timeline.asset_cache_arc())
            })
        })
    }
}

pub(super) fn document_scene_dimensions(ast: &[Stmt]) -> SceneDimensions {
    ast.iter()
        .find_map(|stmt| match stmt {
            Stmt::Config { settings, .. } => settings.iter().find_map(|property| {
                if property.name != "resolution" {
                    return None;
                }

                let Expr::Tuple(items) = &property.value else {
                    return None;
                };
                if items.len() != 2 {
                    return None;
                }

                match (&items[0], &items[1]) {
                    (Expr::Num(width), Expr::Num(height)) if *width > 0.0 && *height > 0.0 => {
                        Some(SceneDimensions {
                            width: width.round() as u32,
                            height: height.round() as u32,
                        })
                    },
                    _ => None,
                }
            }),
            _ => None,
        })
        .unwrap_or_default()
}
