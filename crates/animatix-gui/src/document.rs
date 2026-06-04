use crate::error::GuiError;
use animatix_syntax::ast::{Expr, Stmt};
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use animatix::composition::{BuildTarget, Composition};
use animatix_syntax::module::{ActionTemplate, ComponentEntry, ModuleError, ModuleGraph, Namespace};
use animatix_syntax::source_index::SourceIndex;
use animatix::timeline::{AnimationTrack, PropertyTrack, SceneDimensions, Timeline, TimelineIndex};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
        };

        if let Err(e) = document.rebuild() {
            tracing::warn!("Initial document rebuild failed: {}", e);
        }
        Ok(document)
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
        }
    }

    pub fn set_source_text(&mut self, source_text: String) {
        self.source_text = source_text;
        self.is_dirty = true;
    }

    /// Rebuild just the source index from the current source text.
    ///
    /// Used after external source changes (typing, hot-reload) when the AST is
    /// not already available. Inspector edits mutate the AST directly and build
    /// the index from the mutated AST without re-parsing.
    pub fn rebuild_source_index(&mut self) {
        if let Ok((raw_statements, _, _, _, _, _)) = self.load_program() {
            self.source_index = Some(SourceIndex::build(&raw_statements));
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
        let (raw_statements, expanded_statements, namespaces, type_diagnostics, components, module_actions) = match self.load_program() {
            Ok((raw_statements, expanded_statements, namespaces, type_diagnostics, components, module_actions)) => {
                (raw_statements, expanded_statements, namespaces, type_diagnostics, components, module_actions)
            }
            Err(err) => {
                let err_string = err.to_string();
                self.last_rebuild_error = Some(err_string.clone());
                self.raw_statements = None;
                self.expanded_statements = None;
                self.source_index = None;
                self.namespaces = HashMap::new();
                self.components = HashMap::new();
                self.module_actions = HashMap::new();
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
        let source_index = SourceIndex::build(&raw_statements);

        let report = BuildTarget::from_ast_with_quality(
            &expanded_statements,
            &namespaces,
            Some(&self.file_path),
            animatix::timeline::BuildQuality::Draft,
        );
        self.last_rebuild_error = None;
        self.duration_s = report.output.duration_s().max(0.1);
        self.scene_dimensions = document_scene_dimensions(&expanded_statements);
        self.raw_statements = Some(raw_statements);
        self.expanded_statements = Some(expanded_statements);
        self.source_index = Some(source_index);
        self.namespaces = namespaces;
        self.components = components;
        self.module_actions = module_actions;
        let mut all_diagnostics = type_diagnostics;
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

        match report.output {
            BuildTarget::SingleScene(mut timeline) => {
                timeline.plot_path_cache = old_plot_cache;
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
                }
                self.timeline = None;
                self.composition = Some(composition);
            }
        }

        // Build timeline index from source text (bi-directional sync)
        self.timeline_index = TimelineIndex::build(&self.source_text);
        self.keyframe_lines = self.timeline_index.keyframes.iter().map(|(_, line)| *line).collect();

        Ok(())
    }

    /// Load the program, returning (raw_statements, expanded_statements, namespaces).
    /// Raw statements are the parsed statements before component expansion.
    #[allow(clippy::type_complexity)]
    fn load_program(
        &self,
    ) -> Result<(Vec<Stmt>, Vec<Stmt>, HashMap<String, Namespace>, Vec<Diagnostic>, HashMap<String, ComponentEntry>, HashMap<String, ActionTemplate>), ModuleError>
    {
        let mut graph = ModuleGraph::new();
        let mut program = graph
            .load_program_with_source(&self.file_path, Some(&self.source_text))?;
        let type_diagnostics = program.typecheck();
        // expand_components borrows &self, so call it before moving fields out.
        let expanded_statements = program.expand_components();
        let raw_statements = program.statements;
        let namespaces = program.namespaces;
        let components = program.components;
        let module_actions = program.module_actions;
        Ok((raw_statements, expanded_statements, namespaces, type_diagnostics, components, module_actions))
    }

    pub fn raw_program_statements(&self) -> Option<&[Stmt]> {
        self.raw_statements.as_deref()
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
        if let Some(timeline) = self.timeline.as_ref() {
            return Some(timeline);
        }

        let composition = self.composition.as_ref()?;
        let active_scene = self
            .active_scene
            .as_deref()
            .and_then(|name| composition.scenes.get(name))
            .or_else(|| {
                composition
                    .declaration_order
                    .first()
                    .and_then(|name| composition.scenes.get(name))
            });

        active_scene.map(|scene| &scene.timeline)
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
        all_keyframes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
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
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
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
