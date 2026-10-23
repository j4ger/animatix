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

    /// Rebuild using an asset cache carried from a previous build.
    pub fn rebuild_with_asset_cache(
        &mut self,
        asset_cache: Option<std::sync::Arc<animatix::timeline::assets::AssetCache>>,
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
                // self.components = HashMap::new();
                // self.module_fns = HashMap::new();
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

        // Build source index from raw (non-expanded) statements
        let source_index = SourceIndex::build(&result.raw_statements);

        let report = BuildTarget::from_ast_with_quality_and_asset_cache_and_extension_context(
            &result.expanded_statements,
            &result.namespaces,
            Some(&self.file_path),
            animatix::timeline::BuildQuality::Draft,
            asset_cache,
            std::sync::Arc::clone(&self.extension_context),
        );
        self.last_rebuild_error = None;
        self.duration_s = report.output.duration_s().max(0.1);
        self.scene_dimensions = document_scene_dimensions(&result.expanded_statements);
        self.raw_statements = Some(result.raw_statements);
        self.expanded_statements = Some(result.expanded_statements);
        self.source_index = Some(source_index);
        self.namespaces = result.namespaces;
        self.components = result.components;
        self.module_fns = result.module_fns;
        let mut all_diagnostics = result.type_diagnostics;
        all_diagnostics.extend(report.diagnostics);
        self.diagnostics = all_diagnostics;
        // Phase 6.4: Preserve plot path cache across rebuilds.
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

        // Preserve lowered modifier IR across rebuilds when unchanged. Compare
        // the modifier hash to avoid re-lowering when only non-modifier parts
        // of the source changed.
        let old_modifier_data: Option<(u64, Vec<_>)> = self
            .timeline
            .as_ref()
            .map(|t| (t.modifier_hash, t.modifier_programs.clone()))
            .or_else(|| {
                self.composition.as_ref().and_then(|c| {
                    c.scenes.values().next().map(|s| {
                        let t = &s.timeline;
                        (t.modifier_hash, t.modifier_programs.clone())
                    })
                })
            });

        match report.output {
            BuildTarget::SingleScene(mut timeline) => {
                timeline.plot_path_cache = old_plot_cache;
                // Reuse lowered modifier programs if modifier AST is unchanged.
                if let Some((old_hash, old_ir)) = &old_modifier_data {
                    if timeline.modifier_hash == *old_hash && !old_ir.is_empty() {
                        timeline.modifier_programs = old_ir.clone();
                    }
                }
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
                // Reusing modifier IR is only safe when the old data belongs to
                // the same scene. Multi-scene files do not carry a per-scene
                // cache here, so re-lower each scene conservatively.
                let scene_count = composition.scenes.len();
                for scene in composition.scenes.values_mut() {
                    scene.timeline.plot_path_cache.clone_from(&old_plot_cache);
                    if scene_count == 1 {
                        if let Some((old_hash, old_ir)) = &old_modifier_data {
                            if scene.timeline.modifier_hash == *old_hash && !old_ir.is_empty() {
                                scene.timeline.modifier_programs = old_ir.clone();
                            }
                        }
                    }
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
        // Phase 6.4: Preserve plot path cache across rebuilds.
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

        // Preserve lowered modifier IR across rebuilds when unchanged.
        let old_modifier_data: Option<(u64, Vec<_>)> = self
            .timeline
            .as_ref()
            .map(|t| (t.modifier_hash, t.modifier_programs.clone()))
            .or_else(|| {
                self.composition.as_ref().and_then(|c| {
                    c.scenes.values().next().map(|s| {
                        let t = &s.timeline;
                        (t.modifier_hash, t.modifier_programs.clone())
                    })
                })
            });

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
            if let Some((old_hash, old_ir)) = &old_modifier_data {
                if timeline.modifier_hash == *old_hash && !old_ir.is_empty() {
                    timeline.modifier_programs = old_ir.clone();
                }
            }
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
            // Multi-scene output re-lowers modifier programs unless this
            // composition has a single scene with a matching hash.
            let scene_count = composition.scenes.len();
            for scene in composition.scenes.values_mut() {
                scene.timeline.plot_path_cache.clone_from(&old_plot_cache);
                if scene_count == 1 {
                    if let Some((old_hash, old_ir)) = &old_modifier_data {
                        if scene.timeline.modifier_hash == *old_hash && !old_ir.is_empty() {
                            scene.timeline.modifier_programs = old_ir.clone();
                        }
                    }
                }
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
