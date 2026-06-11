use crate::error::GuiError;
use animatix_syntax::ast::{Expr, Stmt};
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use animatix::composition::{BuildTarget, Composition};
use animatix_syntax::module::{ActionTemplate, ComponentEntry, ModuleError, ModuleGraph, Namespace};
use animatix_syntax::source_index::SourceIndex;
use animatix::timeline::{AnimationTrack, PropertyTrack, SceneDimensions, Timeline, TimelineIndex};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Result of loading and parsing a program.
pub struct LoadedProgramResult {
    pub raw_statements: Vec<Stmt>,
    pub expanded_statements: Vec<Stmt>,
    pub namespaces: HashMap<String, Namespace>,
    pub type_diagnostics: Vec<Diagnostic>,
    pub components: HashMap<String, ComponentEntry>,
    pub module_actions: HashMap<String, ActionTemplate>,
}

pub struct DocumentSession {
    pub file_path: PathBuf,
    pub source_text: String,
    pub raw_statements: Option<Vec<Stmt>>,
    pub expanded_statements: Option<Vec<Stmt>>,
    pub namespaces: HashMap<String, Namespace>,
    pub source_index: Option<SourceIndex>,
    pub timeline: Option<Timeline>,
    pub composition: Option<Composition>,
    pub active_scene: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub last_rebuild_error: Option<String>,
    pub is_dirty: bool,
    pub duration_s: f64,
    pub scene_dimensions: SceneDimensions,
    /// Bi-directional timeline index mapping source lines to times and vice versa.
    pub timeline_index: TimelineIndex,
    /// Set of 0-indexed line numbers that contain keyframe declarations.
    /// Derived from timeline_index for editor decorations.
    pub keyframe_lines: Vec<usize>,
    /// Component registry from the last successful program load.
    pub components: HashMap<String, ComponentEntry>,
    /// Module-scoped actions from the last successful program load.
    pub module_actions: HashMap<String, ActionTemplate>,
    /// Hash of source_text at last successful rebuild — skip rebuild if unchanged.
    last_source_hash: u64,
    /// Hash of component registry at last successful rebuild — skip expansion if unchanged.
    last_component_hash: u64,
    /// Cached expanded statements — reused when components haven't changed.
    cached_expanded: Option<Vec<Stmt>>,
    /// Cached module graph — preserves parsed imports across rebuilds.
    cached_module_graph: Option<ModuleGraph>,
    /// When true, use the tree-sitter parser for incremental parsing.
    use_tree_sitter: bool,
}

impl DocumentSession {
    pub fn load(file_path: PathBuf) -> Result<Self, GuiError> {
        let source_text = fs::read_to_string(&file_path)
            .map_err(|err| GuiError::Io { path: file_path.clone(), source: err })?;

        let mut document = Self {
            file_path,
            source_text,
            raw_statements: None,
            expanded_statements: None,
            namespaces: HashMap::new(),
            source_index: None,
            timeline: None,
            composition: None,
            active_scene: None,
            diagnostics: Vec::new(),
            last_rebuild_error: None,
            is_dirty: false,
            duration_s: 5.0,
            scene_dimensions: SceneDimensions::default(),
            timeline_index: TimelineIndex::default(),
            keyframe_lines: Vec::new(),
            components: HashMap::new(),
            module_actions: HashMap::new(),
            last_source_hash: 0,
            last_component_hash: 0,
            cached_expanded: None,
            cached_module_graph: None,
            use_tree_sitter: false,
        };

        if let Err(e) = document.rebuild() {
            tracing::warn!("Initial document rebuild failed: {}", e);
        }
        Ok(document)
    }

    /// Create a session from in-memory source text without loading from disk.
    /// This is used by the background rebuild worker.
    pub fn from_source(file_path: PathBuf, source_text: String) -> Result<Self, GuiError> {
        Ok(Self {
            file_path,
            source_text,
            raw_statements: None,
            expanded_statements: None,
            namespaces: HashMap::new(),
            source_index: None,
            timeline: None,
            composition: None,
            active_scene: None,
            diagnostics: Vec::new(),
            last_rebuild_error: None,
            is_dirty: false,
            duration_s: 5.0,
            scene_dimensions: SceneDimensions::default(),
            timeline_index: TimelineIndex::default(),
            keyframe_lines: Vec::new(),
            components: HashMap::new(),
            module_actions: HashMap::new(),
            last_source_hash: 0,
            last_component_hash: 0,
            cached_expanded: None,
            cached_module_graph: None,
            use_tree_sitter: false,
        })
    }

    pub fn from_error(file_path: PathBuf) -> Self {
        Self {
            file_path,
            source_text: String::new(),
            raw_statements: None,
            expanded_statements: None,
            namespaces: HashMap::new(),
            source_index: None,
            timeline: None,
            composition: None,
            active_scene: None,
            diagnostics: Vec::new(),
            last_rebuild_error: None,
            is_dirty: false,
            duration_s: 5.0,
            scene_dimensions: SceneDimensions::default(),
            timeline_index: TimelineIndex::default(),
            keyframe_lines: Vec::new(),
            components: HashMap::new(),
            module_actions: HashMap::new(),
            last_source_hash: 0,
            last_component_hash: 0,
            cached_expanded: None,
            cached_module_graph: None,
            use_tree_sitter: false,
        }
    }

    /// Enable or disable the tree-sitter parser for incremental parsing.
    /// When enabled, the next rebuild will use tree-sitter instead of chumsky.
    pub fn set_use_tree_sitter(&mut self, enabled: bool) {
        if self.use_tree_sitter != enabled {
            self.use_tree_sitter = enabled;
            // Invalidate cached module graph since the parser changed
            self.cached_module_graph = None;
        }
    }

    /// Returns true if the tree-sitter parser is enabled.
    pub fn uses_tree_sitter(&self) -> bool {
        self.use_tree_sitter
    }

    pub fn set_source_text(&mut self, source_text: String) {
        self.source_text = source_text;
        self.is_dirty = true;
        self.cached_expanded = None;
    }

    /// Rebuild just the source index from the current source text.
    ///
    /// Used after external source changes (typing, hot-reload) when the AST is
    /// not already available. Inspector edits mutate the AST directly and build
    /// the index from the mutated AST without re-parsing.
    pub fn rebuild_source_index(&mut self) {
        if let Ok(result) = self.load_program() {
            self.source_index = Some(SourceIndex::build(&result.raw_statements));
        }
    }

    pub fn reload_from_disk(&mut self) -> Result<(), GuiError> {
        let path = self.file_path.clone();
        self.source_text = fs::read_to_string(&path)
            .map_err(|err| GuiError::Io { path, source: err })?;
        self.is_dirty = false;
        if let Err(e) = self.rebuild() {
            tracing::warn!("Document reload rebuild failed: {}", e);
        }
        Ok(())
    }

    pub fn save_to_disk(&mut self) -> Result<(), GuiError> {
        let path = self.file_path.clone();
        fs::write(&path, &self.source_text)
            .map_err(|err| GuiError::Io { path, source: err })?;
        self.is_dirty = false;
        Ok(())
    }

    pub fn rebuild(&mut self) -> Result<(), GuiError> {
        // Skip rebuild if source text hasn't changed since last successful build
        let mut hasher = DefaultHasher::new();
        self.source_text.hash(&mut hasher);
        let source_hash = hasher.finish();
        if source_hash == self.last_source_hash && (self.timeline.is_some() || self.composition.is_some()) {
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
                // self.module_actions = HashMap::new();
                self.timeline = None;
                self.composition = None;
                self.active_scene = None;
                self.timeline_index = TimelineIndex::default();
                self.keyframe_lines = Vec::new();
                self.diagnostics = diagnostics_from_module_error(&err, &self.file_path);
                self.duration_s = 0.1;
                self.scene_dimensions = SceneDimensions::default();
                return Err(GuiError::Build { message: err_string });
            }
        };

        // Build source index from raw (non-expanded) statements
        let source_index = SourceIndex::build(&result.raw_statements);

        let report = BuildTarget::from_ast_with_quality(
            &result.expanded_statements,
            &result.namespaces,
            Some(&self.file_path),
            animatix::timeline::BuildQuality::Draft,
        );
        self.last_rebuild_error = None;
        self.duration_s = report.output.duration_s().max(0.1);
        self.scene_dimensions = document_scene_dimensions(&result.expanded_statements);
        self.raw_statements = Some(result.raw_statements);
        self.expanded_statements = Some(result.expanded_statements);
        self.source_index = Some(source_index);
        self.namespaces = result.namespaces;
        self.components = result.components;
        self.module_actions = result.module_actions;
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
                    c.scenes
                        .values()
                        .next()
                        .map(|s| s.timeline.plot_path_cache.clone())
                })
            })
            .unwrap_or_default();

        // Preserve modifier IR/bytecode programs across rebuilds when unchanged.
        // Compare modifier hash to avoid recompilation when only non-modifier
        // parts of the source changed.
        let old_modifier_data: Option<(u64, Vec<_>, Vec<_>)> = self
            .timeline
            .as_ref()
            .map(|t| (t.modifier_hash, t.modifier_programs.clone(), t.modifier_bytecode_programs.clone()))
            .or_else(|| {
                self.composition.as_ref().and_then(|c| {
                    c.scenes.values().next().map(|s| {
                        let t = &s.timeline;
                        (t.modifier_hash, t.modifier_programs.clone(), t.modifier_bytecode_programs.clone())
                    })
                })
            });

        match report.output {
            BuildTarget::SingleScene(mut timeline) => {
                timeline.plot_path_cache = old_plot_cache;
                // Reuse compiled modifier programs if modifier AST is unchanged.
                if let Some((old_hash, old_ir, old_bc)) = &old_modifier_data {
                    if timeline.modifier_hash == *old_hash && !old_ir.is_empty() {
                        timeline.modifier_programs = old_ir.clone();
                        timeline.modifier_bytecode_programs = old_bc.clone();
                    }
                }
                self.timeline = Some(timeline);
                self.composition = None;
                self.active_scene = None;
            }
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
                    // Reuse compiled modifier programs if modifier AST is unchanged.
                    if let Some((old_hash, old_ir, old_bc)) = &old_modifier_data {
                        if scene.timeline.modifier_hash == *old_hash && !old_ir.is_empty() {
                            scene.timeline.modifier_programs = old_ir.clone();
                            scene.timeline.modifier_bytecode_programs = old_bc.clone();
                        }
                    }
                }
                self.timeline = None;
                self.composition = Some(composition);
            }
        }

        // Build timeline index from source text (bi-directional sync)
        self.timeline_index = TimelineIndex::build(&self.source_text);
        self.keyframe_lines = self.timeline_index.keyframes.iter().map(|(_, line)| *line).collect();

        // Update source hash after successful rebuild
        self.last_source_hash = source_hash;

        Ok(())
    }

    /// Apply a successful background rebuild output to this session.
    /// Carries forward caches (plot_path_cache, modifier programs) from the
    /// current session since the background worker started from a fresh session.
    pub fn apply_rebuild_output(&mut self, output: crate::app::document::rebuild_output::RebuildOutput, source_hash: u64) {
        // Phase 6.4: Preserve plot path cache across rebuilds.
        let old_plot_cache = self
            .timeline
            .as_ref()
            .map(|t| t.plot_path_cache.clone())
            .or_else(|| {
                self.composition.as_ref().and_then(|c| {
                    c.scenes
                        .values()
                        .next()
                        .map(|s| s.timeline.plot_path_cache.clone())
                })
            })
            .unwrap_or_default();

        // Preserve modifier IR/bytecode programs across rebuilds when unchanged.
        let old_modifier_data: Option<(u64, Vec<_>, Vec<_>)> = self
            .timeline
            .as_ref()
            .map(|t| {
                (
                    t.modifier_hash,
                    t.modifier_programs.clone(),
                    t.modifier_bytecode_programs.clone(),
                )
            })
            .or_else(|| {
                self.composition.as_ref().and_then(|c| {
                    c.scenes.values().next().map(|s| {
                        let t = &s.timeline;
                        (
                            t.modifier_hash,
                            t.modifier_programs.clone(),
                            t.modifier_bytecode_programs.clone(),
                        )
                    })
                })
            });

        self.last_rebuild_error = None;
        self.raw_statements = Some(output.raw_statements);
        self.expanded_statements = Some(output.expanded_statements);
        self.namespaces = output.namespaces;
        self.components = output.components;
        self.module_actions = output.module_actions;
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
            if let Some((old_hash, old_ir, old_bc)) = &old_modifier_data {
                if timeline.modifier_hash == *old_hash && !old_ir.is_empty() {
                    timeline.modifier_programs = old_ir.clone();
                    timeline.modifier_bytecode_programs = old_bc.clone();
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
            for scene in composition.scenes.values_mut() {
                scene.timeline.plot_path_cache.clone_from(&old_plot_cache);
                if let Some((old_hash, old_ir, old_bc)) = &old_modifier_data {
                    if scene.timeline.modifier_hash == *old_hash && !old_ir.is_empty() {
                        scene.timeline.modifier_programs = old_ir.clone();
                        scene.timeline.modifier_bytecode_programs = old_bc.clone();
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
    }

    /// Apply a failed background rebuild output to this session.
    pub fn apply_rebuild_failure(&mut self, failure: &crate::app::document::rebuild_output::RebuildFailure) {
        self.last_rebuild_error = Some(failure.error.clone());
        // Mirror the error arm of rebuild(): clear built state
        self.raw_statements = None;
        self.expanded_statements = None;
        self.timeline = None;
        self.composition = None;
        self.active_scene = None;
        self.diagnostics = failure.diagnostics.clone();
        self.source_index = failure.partial_source_index.clone();
        self.timeline_index = TimelineIndex::default();
        self.keyframe_lines = Vec::new();
        self.duration_s = 0.1;
        self.scene_dimensions = SceneDimensions::default();
    }

    /// Load the program, returning a structured result.
    /// Raw statements are the parsed statements before component expansion.
    /// Uses cached ModuleGraph to avoid re-reading unchanged imports.
    /// Skips component expansion when the component registry hasn't changed.
    fn load_program(&mut self) -> Result<LoadedProgramResult, ModuleError> {
        let mut graph = if self.use_tree_sitter {
            self.cached_module_graph.take().unwrap_or_else(ModuleGraph::new_with_tree_sitter)
        } else {
            self.cached_module_graph.take().unwrap_or_default()
        };
        let mut program = graph
            .load_program_with_source(&self.file_path, Some(&self.source_text))?;
        let type_diagnostics = program.typecheck();

        // Hash the component registry to detect changes.
        let component_hash = {
            let mut hasher = DefaultHasher::new();
            let mut names: Vec<&String> = program.components.keys().collect();
            names.sort();
            for name in names {
                name.hash(&mut hasher);
                let entry = &program.components[name];
                format!("{:?}", entry.definition).hash(&mut hasher);
            }
            hasher.finish()
        };

        // Skip expansion if components haven't changed and we have cached results.
        let expanded_statements = if component_hash == self.last_component_hash
            && self.cached_expanded.is_some()
        {
            self.cached_expanded.clone().unwrap()
        } else {
            let expanded = program.expand_components();
            self.cached_expanded = Some(expanded.clone());
            self.last_component_hash = component_hash;
            expanded
        };

        // Cache the graph for next rebuild (preserves parsed imports)
        self.cached_module_graph = Some(graph);
        Ok(LoadedProgramResult {
            raw_statements: program.statements,
            expanded_statements,
            namespaces: program.namespaces,
            type_diagnostics,
            components: program.components,
            module_actions: program.module_actions,
        })
    }

    pub fn raw_program_statements(&self) -> Option<&[Stmt]> {
        self.raw_statements.as_deref()
    }

    /// Collect all audio segments from the timeline or composition, resolving
    /// relative source paths relative to the document's directory.
    pub fn all_audio_segments(&self) -> Vec<animatix::timeline::AudioSegment> {
        use animatix::timeline::AudioSegment;
        use std::path::Path;

        let doc_dir = self.file_path.parent().unwrap_or(Path::new(""));

        let segments: Vec<AudioSegment> = if let Some(composition) = &self.composition {
            composition
                .scenes
                .values()
                .flat_map(|s| s.timeline.audio_segments().to_vec())
                .collect()
        } else if let Some(timeline) = &self.timeline {
            timeline.audio_segments().to_vec()
        } else {
            Vec::new()
        };

        // Resolve relative source paths relative to the document directory
        segments
            .into_iter()
            .map(|mut seg| {
                let path = Path::new(&seg.source);
                if path.is_relative() {
                    if let Some(resolved) = doc_dir.join(path).canonicalize().ok() {
                        seg.source = resolved.to_string_lossy().to_string();
                    }
                }
                seg
            })
            .collect()
    }
}

impl DocumentSession {
    /// Find the 0-indexed source line of the keyframe whose absolute time
    /// is closest to and ≤ `time_s`. Returns `None` if no keyframe exists.
    pub fn find_keyframe_line_at(&self, time_s: f64) -> Option<usize> {
        self.active_timeline()?;
        self.timeline_index.line_for_time((time_s * 1000.0) as u64)
    }

    /// Find the absolute time of the keyframe immediately before `time_s`.
    pub fn prev_keyframe_time(&self, time_s: f64) -> f64 {
        let keyframes = timeline_keyframe_times_s(self.active_timeline(), self.composition.as_ref(), self.active_scene.as_deref());
        if !keyframes.is_empty() {
            return keyframes
                .into_iter()
                .rev()
                .find(|t| *t <= time_s)
                .unwrap_or(0.0);
        }

        self.timeline_index
            .prev_keyframe_time((time_s * 1000.0) as u64)
            .map(|ms| ms as f64 / 1000.0)
            .unwrap_or(0.0)
    }

    /// Rescan source text for keyframes and update cached maps.
    /// Call this after direct source text modifications (e.g. keyframe insertion)
    /// so the editor decorations stay in sync before the next rebuild.
    pub fn rescan_keyframe_lines(&mut self) {
        self.timeline_index = TimelineIndex::build(&self.source_text);
        self.keyframe_lines = self.timeline_index.keyframes.iter().map(|(_, line)| *line).collect();
    }

    pub fn is_composition(&self) -> bool {
        self.composition.is_some()
    }

    pub fn active_timeline(&self) -> Option<&Timeline> {
        self.active_timeline_ref().map(|r| r.timeline)
    }

    /// Returns an `ActiveTimelineRef` with full scene context.
    /// For single-scene documents, returns a reference directly.
    /// For compositions, resolves the active scene (or falls back to
    /// declaration order / entry scene).
    pub fn active_timeline_ref(&self) -> Option<crate::app::document::active_timeline::ActiveTimelineRef<'_>> {
        use crate::app::document::active_timeline::{ActiveSceneId, ActiveTimelineRef};

        if let Some(timeline) = self.timeline.as_ref() {
            return Some(ActiveTimelineRef {
                id: ActiveSceneId::SingleScene,
                timeline,
                composition: None,
                scene_name: None,
                duration_s: self.duration_s,
                dimensions: self.scene_dimensions,
            });
        }

        let composition = self.composition.as_ref()?;
        let (scene_name, scene) = self
            .active_scene
            .as_deref()
            .and_then(|name| composition.scenes.get(name).map(|s| (name, s)))
            .or_else(|| {
                composition
                    .declaration_order
                    .first()
                    .and_then(|name| composition.scenes.get(name).map(|s| (name.as_str(), s)))
            })

            .or_else(|| {
                composition.scenes.iter().next().map(|(name, s)| (name.as_str(), s))
            })?;

        Some(ActiveTimelineRef {
            id: ActiveSceneId::Scene(scene_name.to_string()),
            timeline: &scene.timeline,
            composition: Some(composition),
            scene_name: Some(scene_name),
            duration_s: scene.duration_s.max(0.1),
            dimensions: self.scene_dimensions,
        })
    }

    /// Returns a mutable `ActiveTimelineMut` for editing the active scene.
    /// For single-scene documents, returns the sole timeline mutably.
    /// For compositions, resolves the active scene similarly.
    pub fn active_timeline_mut(&mut self) -> Option<crate::app::document::active_timeline::ActiveTimelineMut<'_>> {
        use crate::app::document::active_timeline::{ActiveSceneId, ActiveTimelineMut};

        if let Some(timeline) = self.timeline.as_mut() {
            return Some(ActiveTimelineMut {
                id: ActiveSceneId::SingleScene,
                timeline,
                scene_name: None,
            });
        }

        let composition = self.composition.as_mut()?;
        let scene_name = self
            .active_scene
            .clone()
            .or_else(|| {
                composition
                    .declaration_order
                    .first()
                    .cloned()
            })

            .or_else(|| {
                composition.scenes.keys().next().cloned()
            })?;

        if !composition.scenes.contains_key(&scene_name) {
            return None;
        }
        let scene = composition.scenes.get_mut(&scene_name)?;
        Some(ActiveTimelineMut {
            id: ActiveSceneId::Scene(scene_name.clone()),
            timeline: &mut scene.timeline,
            scene_name: Some(scene_name),
        })
    }

    /// Resolve an export target for the given scope.
    /// For single-scene documents, always returns the timeline.
    /// For compositions, returns the whole composition or active scene timeline.
    pub fn export_target(&self, scope: crate::app::document::export_target::ExportScope)
        -> Option<crate::app::document::export_target::ExportTargetRef<'_>>
    {
        use crate::app::document::export_target::{ExportScope, ExportTargetRef};

        match scope {
            ExportScope::ActiveScene | ExportScope::Scene(_) => {
                let tr = self.active_timeline_ref()?;
                Some(ExportTargetRef::Timeline {
                    timeline: tr.timeline,
                    duration_s: tr.duration_s,
                    dimensions: tr.dimensions,
                })
            }
            ExportScope::WholeComposition => {
                let composition = self.composition.as_ref()?;
                Some(ExportTargetRef::Composition {
                    composition,
                    duration_s: composition.global_duration_s.max(0.1),
                    dimensions: self.scene_dimensions,
                })
            }
        }
    }

    pub fn scene_names(&self) -> &[String] {
        self.composition
            .as_ref()
            .map(|composition| composition.declaration_order.as_slice())
            .unwrap_or(&[])
    }

    pub fn import_aliases(&self) -> Vec<String> {
        let mut aliases = self.namespaces.keys().cloned().collect::<Vec<_>>();
        aliases.sort();
        aliases
    }
}

fn document_scene_dimensions(ast: &[Stmt]) -> SceneDimensions {
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
                    }
                    _ => None,
                }
            }),
            _ => None,
        })
        .unwrap_or_default()
}

fn latest_keyframe_ms<T>(track: &Option<PropertyTrack<T>>) -> Option<u64> {
    track.as_ref().and_then(|t| t.keyframes.keys().next_back().copied())
}

fn track_max_ms(track: &AnimationTrack) -> u64 {
    let mut max_ms = 0u64;

    for time in [
        latest_keyframe_ms(&track.position),
        latest_keyframe_ms(&track.motion_offset),
        latest_keyframe_ms(&track.rotation),
        latest_keyframe_ms(&track.scale),
        latest_keyframe_ms(&track.placement_mode),
        latest_keyframe_ms(&track.position_binding),
        latest_keyframe_ms(&track.size),
        latest_keyframe_ms(&track.line_from),
        latest_keyframe_ms(&track.line_to),
        latest_keyframe_ms(&track.arc_angles),
        latest_keyframe_ms(&track.color),
        latest_keyframe_ms(&track.shape_type),
        latest_keyframe_ms(&track.opacity),
        latest_keyframe_ms(&track.stroke_width),
        latest_keyframe_ms(&track.stroke_color),
        latest_keyframe_ms(&track.stroke_progress),
        latest_keyframe_ms(&track.fill_opacity),
        latest_keyframe_ms(&track.morph_options),
        latest_keyframe_ms(&track.text_content),
        latest_keyframe_ms(&track.text_paths),
        latest_keyframe_ms(&track.vector_paths),
        latest_keyframe_ms(&track.image),
        latest_keyframe_ms(&track.points),
    ]
    .into_iter()
    .flatten()
    {
        max_ms = max_ms.max(time);
    }

    max_ms
}

pub fn timeline_duration_seconds(
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
) -> f64 {
    if let Some(timeline) = timeline {
        timeline
            .tracks()
            .values()
            .map(track_max_ms)
            .max()
            .unwrap_or(0) as f64
            / 1000.0
    } else {
        composition.map(|c| c.global_duration_s).unwrap_or(0.0)
    }
}

pub fn timeline_keyframe_times_s(
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    _active_scene: Option<&str>,
) -> Vec<f64> {
    if let Some(timeline) = timeline {
        timeline.keyframe_times_s()
    } else if let Some(composition) = composition {
        // Collect keyframes from all scenes, converting local times to global times
        let mut all_keyframes: Vec<f64> = Vec::new();
        for (scene_name, scene) in &composition.scenes {
            let start_s = composition.scene_start_times.get(scene_name).copied().unwrap_or(0.0);
            for local_time in scene.timeline.keyframe_times_s() {
                all_keyframes.push(start_s + local_time);
            }
        }
        all_keyframes.sort_by(|a, b| a.total_cmp(b));
        all_keyframes.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        all_keyframes
    } else {
        Vec::new()
    }
}

pub fn default_file_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("untitled.amx")
}

/// Convert a `ModuleError` into a vector of `Diagnostic`s.
/// Parse errors are preserved as individual diagnostics with location info
/// via `ParseError::to_diagnostic()`.
/// Other module errors become a single `SourceLoadFailure` diagnostic.
fn diagnostics_from_module_error(err: &ModuleError, file_path: &Path) -> Vec<Diagnostic> {
    match err {
        ModuleError::ParseErrors(parse_errors) => parse_errors
            .iter()
            .map(|e| e.to_diagnostic().with_path(file_path))
            .collect(),
        _ => vec![Diagnostic::error(
            DiagnosticCode::SourceLoadFailure,
            DiagnosticPhase::Parse,
            err.to_string(),
        )
        .with_path(file_path)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use animatix_syntax::ast::{Property, Time};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir(name: &str) -> Result<PathBuf, GuiError> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "animatix_gui_{}_{}_{}",
            name,
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).map_err(|err| GuiError::Io { path: dir.clone(), source: err })?;
        Ok(dir)
    }

    fn write_file(path: &Path, contents: &str) -> Result<(), GuiError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| GuiError::Io { path: parent.to_path_buf(), source: err })?;
        }
        fs::write(path, contents).map_err(|err| GuiError::Io { path: path.to_path_buf(), source: err })?;
        Ok(())
    }

    #[test]
    fn duration_scans_latest_track_keyframe() {
        let ast = vec![
            Stmt::Keyframe {
                time: Time::Seconds(0.0),
                body: vec![Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "box".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: animatix_syntax::ast::Expr::Tuple(vec![
                            animatix_syntax::ast::Expr::Num(100.0),
                            animatix_syntax::ast::Expr::Num(100.0),
                        ]),
                        value_span: None,
                    trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                }],
                span: None,
            },
            Stmt::RelativeKeyframe {
                offset: Time::Seconds(2.0),
                body: vec![Stmt::Assignment {
                    target: vec!["box".to_string()],
                    property: "scale".to_string(),
                    value: animatix_syntax::ast::Expr::Num(0.5),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
                span: None,
            },
        ];

        let timeline = Timeline::build(&ast);
        assert_eq!(timeline_duration_seconds(Some(&timeline), None), 2.0);
    }

    #[test]
    fn scene_dimensions_use_config_resolution_when_present() {
        let ast = vec![Stmt::Config {
            settings: vec![Property {
                name: "resolution".to_string(),
                value: Expr::Tuple(vec![Expr::Num(1280.0), Expr::Num(720.0)]),
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
        }];

        assert_eq!(
            document_scene_dimensions(&ast),
            SceneDimensions {
                width: 1280,
                height: 720,
            }
        );
    }

    #[test]
    fn scene_dimensions_fall_back_to_default_when_missing() {
        let ast = vec![Stmt::Comment("no config".to_string(), None)];

        assert_eq!(document_scene_dimensions(&ast), SceneDimensions::default());
    }

    #[test]
    fn timeline_keyframe_times_none_timeline_none_composition() {
        let times = timeline_keyframe_times_s(None, None, None);
        assert!(times.is_empty(), "expected empty vec, got {times:?}");
    }

    #[test]
    fn timeline_keyframe_times_empty_timeline() {
        let ast: Vec<Stmt> = vec![];
        let timeline = Timeline::build(&ast);
        let times = timeline_keyframe_times_s(Some(&timeline), None, None);
        assert!(times.is_empty(), "expected empty vec, got {times:?}");
    }

    #[test]
    fn timeline_keyframe_times_are_sorted_and_deduped() {
        let ast = vec![
            Stmt::Keyframe {
                time: Time::Seconds(0.0),
                body: vec![Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "box".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                        value_span: None,
                    trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                }],
                span: None,
            },
            Stmt::RelativeKeyframe {
                offset: Time::Seconds(1.0),
                body: vec![Stmt::Assignment {
                    target: vec!["box".to_string()],
                    property: "scale".to_string(),
                    value: Expr::Num(0.5),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
                span: None,
            },
            Stmt::RelativeKeyframe {
                offset: Time::Seconds(1.0),
                body: vec![Stmt::Assignment {
                    target: vec!["box".to_string()],
                    property: "stroke_width".to_string(),
                    value: Expr::Num(4.0),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
                span: None,
            },
            Stmt::RelativeKeyframe {
                offset: Time::Seconds(1.5),
                body: vec![Stmt::Assignment {
                    target: vec!["box".to_string()],
                    property: "scale".to_string(),
                    value: Expr::Num(1.0),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
                span: None,
            },
        ];

        let timeline = Timeline::build(&ast);
        let times = timeline_keyframe_times_s(Some(&timeline), None, None);
        // Verify the user-visible keyframe times exist (in ms resolution)
        assert!(times.contains(&0.0), "missing 0.0, got {times:?}");
        assert!(times.contains(&1.0), "missing 1.0, got {times:?}");
        assert!(times.contains(&2.0), "missing 2.0, got {times:?}");
        assert!(times.contains(&3.5), "missing 3.5, got {times:?}");
        // Verify they're sorted and deduped
        let mut sorted = times.clone();
        sorted.sort_by(|a, b| f64::total_cmp(a, b));
        sorted.dedup();
        assert_eq!(times, sorted, "times are not sorted and deduped");
    }

    #[test]
    fn rebuild_uses_expanded_program_pipeline_for_imported_components() {
        let dir = temp_project_dir("document_rebuild_components").unwrap();
        let entry = dir.join("scene.amx");
        let library = dir.join("components.amx");

        write_file(
            &library,
            r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: blue
    title_text: Text { text: title, at: (0, -20) }
    badge: Ellipse, radius: 12, color: gold
    badge.color = red
}
"#,
        ).unwrap();

        write_file(
            &entry,
            r#"
config { resolution: (1280, 720) }
import "./components.amx"

card: MetricCard, title: "Latency"
"#,
        ).unwrap();

        let document = DocumentSession::load(entry).expect("document should rebuild");
        let timeline = document.timeline.as_ref().expect("timeline should exist");

        assert!(timeline.has_actor("card"));
        assert!(timeline.has_actor("card.title_text"));
        assert!(timeline.has_actor("card.badge"));
        assert!(document.last_rebuild_error.is_none());
        assert_eq!(
            document.scene_dimensions,
            SceneDimensions {
                width: 1280,
                height: 720,
            }
        );

        let expanded = document
            .expanded_statements
            .as_ref()
            .expect("expanded statements should be stored");
        let expanded_debug = format!("{expanded:#?}");
        assert!(expanded_debug.contains("card.title_text"));
        assert!(!expanded_debug.contains("MetricCard"));
    }

    #[test]
    fn rebuild_supports_multi_scene_compositions() {
        let dir = temp_project_dir("document_rebuild_composition").unwrap();
        let entry = dir.join("scene.amx");

        write_file(
            &entry,
            r#"
# Intro
#0s
title: Text, text: "Welcome"

# Diagram
#0s
graph: Rect, size: (400, 400)
"#,
        ).unwrap();

        let document = DocumentSession::load(entry).expect("document should rebuild");
        assert!(document.is_composition());
        assert!(document.timeline.is_none());
        assert!(document.composition.is_some());
        assert_eq!(document.active_scene.as_deref(), Some("Intro"));
        assert_eq!(document.scene_names(), vec!["Intro".to_string(), "Diagram".to_string()]);
        assert!(document.active_timeline().is_some());
        assert!(document.duration_s > 0.0);
    }

    #[test]
    fn rebuild_surfaces_duplicate_component_exports_and_clears_compiled_state() {
        let dir = temp_project_dir("document_rebuild_duplicate_components").unwrap();
        let entry = dir.join("scene.amx");
        let first = dir.join("first.amx");
        let second = dir.join("second.amx");

        write_file(
            &first,
            r#"
pub component MetricCard(title: "One") {
    title_text: Text { text: title }
}
"#,
        ).unwrap();

        write_file(
            &second,
            r#"
pub component MetricCard(title: "Two") {
    title_text: Text { text: title }
}
"#,
        ).unwrap();

        write_file(
            &entry,
            r#"
import "./first.amx"
import "./second.amx"

card: MetricCard
"#,
        ).unwrap();

        let mut document = DocumentSession::from_error(entry.clone());
        document.source_text = fs::read_to_string(&entry)
            .unwrap_or_else(|_| panic!("failed to read test file: {:?}", &entry));

        let error = document
            .rebuild()
            .expect_err("duplicate exports should fail");

        assert!(error.to_string().contains("Duplicate component export 'MetricCard'"));
        assert!(document.expanded_statements.is_none());
        assert!(document.timeline.is_none());
        assert_eq!(document.diagnostics.len(), 1);
        let diagnostic = &document.diagnostics[0];
        assert_eq!(
            diagnostic.severity,
            animatix_syntax::diagnostics::DiagnosticSeverity::Error
        );
        assert_eq!(diagnostic.phase, DiagnosticPhase::Parse);
        assert_eq!(diagnostic.code, DiagnosticCode::SourceLoadFailure);
        assert!(
            diagnostic
                .message
                .contains("Duplicate component export 'MetricCard'")
        );
        assert_eq!(diagnostic.location.path.as_ref(), Some(&entry));
        assert!(
            document
                .last_rebuild_error
                .as_ref()
                .is_some_and(|message| message.contains("Duplicate component export 'MetricCard'"))
        );
        assert_eq!(document.duration_s, 0.1);
        assert_eq!(document.scene_dimensions, SceneDimensions::default());
        assert_eq!(document.file_path, entry);
    }

    #[test]
    fn rebuild_records_parse_failure_diagnostic_for_invalid_source() {
        let dir = temp_project_dir("document_rebuild_parse_failure").unwrap();
        let entry = dir.join("scene.amx");

        write_file(
            &entry,
            r#"
scene: Rect, size: (100, 100)
"#,
        ).unwrap();

        let mut document =
            DocumentSession::load(entry.clone()).expect("valid document should load");
        document.set_source_text("scene: Rect {".to_string());

        let error = document
            .rebuild()
            .expect_err("invalid source should fail rebuild");

        assert!(!error.to_string().is_empty());
        assert!(document.expanded_statements.is_none());
        assert!(document.timeline.is_none());
        assert_eq!(document.diagnostics.len(), 1);
        let diagnostic = &document.diagnostics[0];
        assert_eq!(
            diagnostic.severity,
            animatix_syntax::diagnostics::DiagnosticSeverity::Error
        );
        assert_eq!(diagnostic.phase, DiagnosticPhase::Parse);
        assert_eq!(diagnostic.code, DiagnosticCode::ParseError);
        assert!(
            !diagnostic.message.is_empty(),
            "parse error message should not be empty"
        );
        assert!(diagnostic.location.line.is_some());
        assert!(diagnostic.location.column.is_some());
        assert_eq!(diagnostic.location.path.as_ref(), Some(&entry));
        assert!(document.last_rebuild_error.is_some());
        assert_eq!(document.duration_s, 0.1);
        assert_eq!(document.scene_dimensions, SceneDimensions::default());
    }

    #[test]
    fn load_keeps_invalid_document_editable_with_parse_diagnostic() {
        let dir = temp_project_dir("document_load_parse_failure").unwrap();
        let entry = dir.join("scene.amx");

        write_file(&entry, "scene: Rect {").unwrap();

        let document =
            DocumentSession::load(entry.clone()).expect("source should still load into session");

        assert_eq!(document.file_path, entry);
        assert_eq!(document.source_text, "scene: Rect {");
        assert!(document.expanded_statements.is_none());
        assert!(document.timeline.is_none());
        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].phase, DiagnosticPhase::Parse);
        assert_eq!(document.diagnostics[0].code, DiagnosticCode::ParseError);
        assert!(document.diagnostics[0].location.line.is_some());
        assert!(document.diagnostics[0].location.column.is_some());
        assert!(document.last_rebuild_error.is_some());
    }
}
