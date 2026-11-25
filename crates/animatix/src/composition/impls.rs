//! Inherent implementations for the composition data model: queries, the
//! canonical build entry point, and the test-only build shortcuts.

use super::*;

impl Composition {
    /// Scheduled scene names in playback order, including scenes with equal start times.
    pub fn playback_order(&self) -> &[String] {
        &self.playback_order
    }

    /// Build a [`CompositionSummary`] from this composition's scene graph.
    pub fn summary(&self) -> CompositionSummary {
        let scenes = self
            .playback_order
            .iter()
            .filter_map(|name| {
                let scene = self.scenes.get(name)?;
                let start = self.scene_start_times.get(name)?;
                Some((name.clone(), *start, scene.duration_s, scene.explicit_duration_s))
            })
            .collect();
        let edges = self
            .playback_order
            .iter()
            .filter_map(|name| {
                let edge = self.edges.get(name)?;
                Some((
                    name.clone(),
                    edge.to_scene.clone(),
                    edge.transition.id.clone(),
                    edge.transition.duration_ms,
                ))
            })
            .collect();
        CompositionSummary {
            scenes,
            edges,
            total_duration_s: self.global_duration_s,
        }
    }

    /// Flatten scheduled scenes' audio onto the composition clock, including gain automation.
    pub fn audio_segments(&self) -> Vec<crate::timeline::AudioSegment> {
        self.declaration_order
            .iter()
            .filter_map(|name| Some((self.scenes.get(name)?, *self.scene_start_times.get(name)?)))
            .flat_map(|(scene, offset)| {
                scene.timeline.audio_segments().iter().cloned().map(move |mut segment| {
                    segment.start_time_s += offset;
                    if let Some(envelope) = &mut segment.volume_envelope {
                        for (time, _) in envelope {
                            *time += offset;
                        }
                    }
                    segment
                })
            })
            .collect()
    }

    /// Interactive pause points across the entire composition in global milliseconds.
    pub fn pause_points(&self) -> &[u64] {
        &self.pause_points
    }

    /// The next pause point strictly after `current_time_ms`.
    pub fn step_next(&self, current_time_ms: u64) -> Option<u64> {
        self.pause_points.iter().copied().find(|&t| t > current_time_ms)
    }

    /// The previous pause point strictly before `current_time_ms`.
    pub fn step_prev(&self, current_time_ms: u64) -> Option<u64> {
        self.pause_points.iter().copied().rfind(|&t| t < current_time_ms)
    }
}

impl BuildTarget {
    /// Returns the authored document canvas size, if configured.
    pub fn configured_canvas_size(&self) -> Option<(u32, u32)> {
        match self {
            Self::SingleScene(timeline) => timeline.configured_canvas_size(),
            Self::MultiScene(composition) => composition
                .scenes
                .values()
                .next()
                .and_then(|scene| scene.timeline.configured_canvas_size()),
        }
    }

    /// Returns the dimensions of the document's logical canvas.
    pub fn canvas_dimensions(&self) -> crate::timeline::SceneDimensions {
        self.configured_canvas_size().map_or_else(Default::default, |(width, height)| {
            crate::timeline::SceneDimensions { width, height }
        })
    }

    /// Resolve authored subtitle cues onto the final document clock.
    pub fn subtitle_cues(
        &self,
        track: Option<&str>,
    ) -> Vec<animatix_text::transcript::SubtitleCue> {
        match self {
            Self::SingleScene(timeline) => {
                let end = timeline.declared_duration_seconds().unwrap_or_else(|| {
                    timeline.duration_seconds().max(timeline.caption_end_seconds())
                });
                timeline.subtitle_cues(end, track)
            },
            Self::MultiScene(composition) => {
                let mut cues = Vec::new();
                for name in composition.playback_order() {
                    let scene = &composition.scenes[name];
                    let offset = composition.scene_start_times[name];
                    let limit = scene.duration_s.min(composition.global_duration_s - offset);
                    cues.extend(scene.timeline.subtitle_cues(limit, track).into_iter().map(
                        |mut cue| {
                            cue.start_s += offset;
                            cue.end_s += offset;
                            cue
                        },
                    ));
                }
                cues.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
                cues
            },
        }
    }

    /// The mutation epoch of the whole document, for a driver keying rendered
    /// frames by content (`timeline::frame_signature`).
    ///
    /// Summed across scenes rather than maxed: epochs only ever increase, so a
    /// sum strictly increases when any scene is edited, while a max would keep
    /// reporting the same number after an edit to a scene whose epoch is not the
    /// largest — a multi-scene document whose dedup never noticed the change.
    pub fn content_epoch(&self) -> u64 {
        match self {
            Self::SingleScene(timeline) => timeline.content_epoch(),
            Self::MultiScene(composition) => {
                composition.scenes.values().map(|scene| scene.timeline.content_epoch()).sum()
            },
        }
    }

    /// Build the appropriate target from parsed AST statements with explicit
    /// options.
    ///
    /// Detects `Stmt::Scene` markers to choose single- or multi-scene mode.
    /// This is the one build implementation: every other entry point either
    /// builds [`BuildOptions`] for it or reports a composition-only result.
    ///
    /// The asset cache uses `source_path`'s directory unless the caller
    /// provided a different document base. Every scene inherits it through
    /// `options`.
    /// Single-scene programs get the `PersistTargetNotCarried` warning because a
    /// lone timeline has no successor to carry into.
    pub fn build_with_options(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
        options: BuildOptions<'_>,
    ) -> BuildReport<Self> {
        let asset_cache = options
            .asset_cache
            .clone()
            .or_else(|| {
                source_path.and_then(|path| path.parent()).map(|dir| {
                    let mut cache = crate::timeline::assets::AssetCache::new();
                    cache.set_base_dirs(Some(dir), None);
                    std::sync::Arc::new(cache)
                })
            })
            .map(|mut cache| {
                if let Some(dir) = source_path.and_then(std::path::Path::parent) {
                    let dir = if dir.as_os_str().is_empty() {
                        std::path::Path::new(".")
                    } else {
                        dir
                    };
                    let cache = std::sync::Arc::make_mut(&mut cache);
                    if cache.document_dir().is_none() {
                        cache.set_document_dir(Some(dir));
                    }
                    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
                    cache.set_project_dir(&dir);
                }
                cache
            });
        let skip_persist_target_warning = options.skip_persist_target_warning;
        let options = BuildOptions {
            asset_cache,
            ..options
        };

        let has_scenes = statements.iter().any(|s| matches!(s, Stmt::Scene { .. }));
        let mut report = if has_scenes {
            if !options.defines.is_empty() {
                tracing::warn!(
                    "--set overrides are not applied to a multi-scene program; the composition builds each scene from its own source"
                );
            }
            let report = Composition::build_with_options(statements, namespaces, options);
            BuildReport {
                output: BuildTarget::MultiScene(report.output),
                diagnostics: report.diagnostics,
            }
        } else {
            let report = Timeline::build_with_options(statements, namespaces, options);
            let mut diagnostics = report.diagnostics;
            // Warn about persistent actors in a truly single-scene file (no
            // successor), unless the caller speaks for persistence itself —
            // the web player does, so it opts out (`skip_persist_target_warning`).
            if !skip_persist_target_warning {
                for (label, flag) in report.output.persistence_flags_iter() {
                    if flag {
                        diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::PersistTargetNotCarried,
                                DiagnosticPhase::Build,
                                format!(
                                    "Actor '{}' is persisted but there is no successor scene to carry into.",
                                    label,
                                ),
                            )
                            .with_subject(label),
                        );
                    }
                }
            }
            BuildReport {
                output: BuildTarget::SingleScene(report.output),
                diagnostics,
            }
        };
        match &mut report.output {
            BuildTarget::SingleScene(timeline) => {
                timeline.validate_cue_range(
                    timeline.playback_duration_seconds(),
                    &mut report.diagnostics,
                );
            },
            BuildTarget::MultiScene(composition) => {
                for scene in composition.scenes.values_mut() {
                    scene.timeline.validate_cue_range(scene.duration_s, &mut report.diagnostics);
                }
            },
        }
        if let Some(path) = source_path {
            for diag in &mut report.diagnostics {
                if diag.location.path.is_none() {
                    diag.location.path = Some(path.to_path_buf());
                }
            }
            report.output.set_asset_source_base(path);
        }
        report
    }

    /// Build from AST with an extension context and `--set NAME=VALUE`
    /// overrides applied to the build environment.
    ///
    /// Convenience for the CLI, whose documents are always built under an
    /// extension context and may carry `--set` overrides. Overrides reach a
    /// single-scene program; a multi-scene composition reports that they do not
    /// reach it yet rather than applying them to some scenes only.
    pub fn from_ast_with_context_and_defines(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
        source_path: Option<&std::path::Path>,
        context: std::sync::Arc<crate::extension_context::ExtensionContext>,
        defines: &[(String, String)],
    ) -> BuildReport<Self> {
        Self::build_with_options(
            statements,
            namespaces,
            source_path,
            BuildOptions::default().with_context(context).with_defines(defines),
        )
    }

    /// Returns the total duration in seconds, regardless of target type.
    ///
    /// This is the length the document *plays* for, so a single scene honours
    /// its declared `config { duration: N }` through
    /// [`Timeline::playback_duration_seconds`] — the GUI preview range and every
    /// export frame count come from here.
    pub fn duration_s(&self) -> f64 {
        match self {
            BuildTarget::SingleScene(timeline) => timeline.playback_duration_seconds(),
            BuildTarget::MultiScene(composition) => composition.global_duration_s,
        }
    }

    /// Returns all interactive pause points in global milliseconds.
    pub fn pause_points(&self) -> &[u64] {
        match self {
            BuildTarget::SingleScene(timeline) => timeline.pause_points(),
            BuildTarget::MultiScene(composition) => composition.pause_points(),
        }
    }

    /// The next pause point strictly after `current_time_ms`.
    pub fn step_next(&self, current_time_ms: u64) -> Option<u64> {
        match self {
            BuildTarget::SingleScene(timeline) => timeline.step_next(current_time_ms),
            BuildTarget::MultiScene(composition) => composition.step_next(current_time_ms),
        }
    }

    /// The previous pause point strictly before `current_time_ms`.
    pub fn step_prev(&self, current_time_ms: u64) -> Option<u64> {
        match self {
            BuildTarget::SingleScene(timeline) => timeline.step_prev(current_time_ms),
            BuildTarget::MultiScene(composition) => composition.step_prev(current_time_ms),
        }
    }

    fn set_asset_source_base(&mut self, source_path: &std::path::Path) {
        let document_dir = source_path.parent();
        match self {
            BuildTarget::SingleScene(timeline) => {
                std::sync::Arc::make_mut(&mut timeline.asset_cache)
                    .set_base_dirs(document_dir, None);
            },
            BuildTarget::MultiScene(composition) => {
                for scene in composition.scenes.values_mut() {
                    std::sync::Arc::make_mut(&mut scene.timeline.asset_cache)
                        .set_base_dirs(document_dir, None);
                }
            },
        }
    }
}

impl Composition {
    /// Returns true if the composition has at least one scene.
    pub fn has_scenes(&self) -> bool {
        !self.scenes.is_empty()
    }

    /// Configure document/workspace base directories for relative asset URLs.
    pub fn set_asset_base_dirs(
        &mut self,
        document_dir: Option<&std::path::Path>,
        workspace_root: Option<&std::path::Path>,
    ) {
        for scene in self.scenes.values_mut() {
            scene.timeline.set_asset_base_dirs(document_dir, workspace_root);
        }
    }
}

/// Test-only shortcuts that keep the composition tests on the canonical build
/// path without repeating the option set at every call site.
#[cfg(test)]
impl Composition {
    pub(super) fn build_test(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
    ) -> BuildReport<Self> {
        Self::build_with_options(statements, namespaces, BuildOptions::default())
    }
}

#[cfg(test)]
impl BuildTarget {
    pub(super) fn build_test(
        statements: &[Stmt],
        namespaces: &std::collections::HashMap<String, Namespace>,
    ) -> BuildReport<Self> {
        Self::build_with_options(statements, namespaces, None, BuildOptions::default())
    }
}
