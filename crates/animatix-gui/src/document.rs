use animatix::ast::{Expr, Stmt};
use animatix::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use animatix::module::{ModuleGraph, Namespace};
use animatix::timeline::{AnimationTrack, PropertyTrack, SceneDimensions, Timeline};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct DocumentSession {
    pub file_path: PathBuf,
    pub source_text: String,
    pub expanded_statements: Option<Vec<Stmt>>,
    pub namespaces: HashMap<String, Namespace>,
    pub timeline: Option<Timeline>,
    pub diagnostics: Vec<Diagnostic>,
    pub last_rebuild_error: Option<String>,
    pub is_dirty: bool,
    pub duration_s: f64,
    pub scene_dimensions: SceneDimensions,
}

impl DocumentSession {
    pub fn load(file_path: PathBuf) -> Result<Self, String> {
        let source_text = fs::read_to_string(&file_path)
            .map_err(|err| format!("Failed to read {}: {err}", file_path.display()))?;

        let mut document = Self {
            file_path,
            source_text,
            expanded_statements: None,
            namespaces: HashMap::new(),
            timeline: None,
            diagnostics: Vec::new(),
            last_rebuild_error: None,
            is_dirty: false,
            duration_s: 5.0,
            scene_dimensions: SceneDimensions::default(),
        };

        let _ = document.rebuild();
        Ok(document)
    }

    pub fn from_error(file_path: PathBuf) -> Self {
        Self {
            file_path,
            source_text: String::new(),
            expanded_statements: None,
            namespaces: HashMap::new(),
            timeline: None,
            diagnostics: Vec::new(),
            last_rebuild_error: None,
            is_dirty: false,
            duration_s: 5.0,
            scene_dimensions: SceneDimensions::default(),
        }
    }

    pub fn set_source_text(&mut self, source_text: String) {
        self.source_text = source_text;
        self.is_dirty = true;
    }

    pub fn reload_from_disk(&mut self) -> Result<(), String> {
        self.source_text = fs::read_to_string(&self.file_path)
            .map_err(|err| format!("Failed to reload {}: {err}", self.file_path.display()))?;
        self.is_dirty = false;
        let _ = self.rebuild();
        Ok(())
    }

    pub fn save_to_disk(&mut self) -> Result<(), String> {
        fs::write(&self.file_path, &self.source_text)
            .map_err(|err| format!("Failed to save {}: {err}", self.file_path.display()))?;
        self.is_dirty = false;
        Ok(())
    }

    pub fn rebuild(&mut self) -> Result<(), String> {
        let (expanded_statements, namespaces) = match self.load_expanded_statements() {
            Ok((expanded_statements, namespaces)) => (expanded_statements, namespaces),
            Err(err) => {
                self.last_rebuild_error = Some(err.clone());
                self.expanded_statements = None;
                self.namespaces = HashMap::new();
                self.timeline = None;
                self.diagnostics = vec![
                    Diagnostic::error(
                        DiagnosticCode::SourceLoadFailure,
                        DiagnosticPhase::Parse,
                        format!("Failed to load or parse source: {err}"),
                    )
                    .with_path(self.file_path.clone()),
                ];
                self.duration_s = 0.1;
                self.scene_dimensions = SceneDimensions::default();
                return Err(err.to_string());
            }
        };
        let report = Timeline::build_with_diagnostics(&expanded_statements, &namespaces);
        self.last_rebuild_error = None;
        self.duration_s = timeline_duration_seconds(&report.output).max(0.1);
        self.scene_dimensions = document_scene_dimensions(&expanded_statements);
        self.expanded_statements = Some(expanded_statements);
        self.namespaces = namespaces;
        self.diagnostics = report.diagnostics;
        self.timeline = Some(report.output);
        Ok(())
    }

    fn load_expanded_statements(&self) -> Result<(Vec<Stmt>, HashMap<String, Namespace>), String> {
        let mut graph = ModuleGraph::new();
        let program = graph
            .load_program_with_source(&self.file_path, Some(&self.source_text))
            .map_err(|err| err.to_string())?;
        Ok((program.expand_components(), program.namespaces))
    }
}

fn document_scene_dimensions(ast: &[Stmt]) -> SceneDimensions {
    ast.iter()
        .find_map(|stmt| match stmt {
            Stmt::Config { settings } => settings.iter().find_map(|property| {
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

fn latest_keyframe_ms<T>(track: &PropertyTrack<T>) -> Option<u64> {
    track.keyframes.keys().next_back().copied()
}

fn track_max_ms(track: &AnimationTrack) -> u64 {
    let mut max_ms = 0u64;

    for time in [
        latest_keyframe_ms(&track.position),
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
        latest_keyframe_ms(&track.text_paths),
        latest_keyframe_ms(&track.vector_paths),
        latest_keyframe_ms(&track.image),
    ]
    .into_iter()
    .flatten()
    {
        max_ms = max_ms.max(time);
    }

    max_ms
}

pub fn timeline_duration_seconds(timeline: &Timeline) -> f64 {
    timeline
        .tracks
        .values()
        .map(track_max_ms)
        .max()
        .unwrap_or(0) as f64
        / 1000.0
}

pub fn timeline_keyframe_times_s(timeline: &Timeline) -> Vec<f64> {
    let mut times_ms = Vec::new();

    for track in timeline.tracks.values() {
        collect_track_keyframe_times(&mut times_ms, track);
    }

    times_ms.sort_unstable();
    times_ms.dedup();
    times_ms
        .into_iter()
        .map(|time_ms| time_ms as f64 / 1000.0)
        .collect()
}

fn collect_track_keyframe_times(times_ms: &mut Vec<u64>, track: &AnimationTrack) {
    times_ms.extend(track.position.keyframes.keys().copied());
    times_ms.extend(track.placement_mode.keyframes.keys().copied());
    times_ms.extend(track.position_binding.keyframes.keys().copied());
    times_ms.extend(track.size.keyframes.keys().copied());
    times_ms.extend(track.line_from.keyframes.keys().copied());
    times_ms.extend(track.line_to.keyframes.keys().copied());
    times_ms.extend(track.arc_angles.keyframes.keys().copied());
    times_ms.extend(track.color.keyframes.keys().copied());
    times_ms.extend(track.shape_type.keyframes.keys().copied());
    times_ms.extend(track.opacity.keyframes.keys().copied());
    times_ms.extend(track.stroke_width.keyframes.keys().copied());
    times_ms.extend(track.stroke_color.keyframes.keys().copied());
    times_ms.extend(track.stroke_progress.keyframes.keys().copied());
    times_ms.extend(track.fill_opacity.keyframes.keys().copied());
    times_ms.extend(track.text_paths.keyframes.keys().copied());
    times_ms.extend(track.vector_paths.keyframes.keys().copied());
    times_ms.extend(track.image.keyframes.keys().copied());
}

pub fn default_file_path() -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| Path::new("."));
    repo_root.join("examples/showcase.amx")
}

#[cfg(test)]
mod tests {
    use super::*;
    use animatix::ast::{Property, Time};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "animatix_gui_{}_{}_{}",
            name,
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn duration_scans_latest_track_keyframe() {
        let ast = vec![
            Stmt::Keyframe {
                time: Time::Seconds(0.0),
                body: vec![Stmt::ActorDecl {
                    is_pub: false,
                    label: "box".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: animatix::ast::Expr::Tuple(vec![
                            animatix::ast::Expr::Num(100.0),
                            animatix::ast::Expr::Num(100.0),
                        ]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                }],
                span: None,
            },
            Stmt::RelativeKeyframe {
                offset: Time::Seconds(2.0),
                body: vec![Stmt::Assignment {
                    target: vec!["box".to_string()],
                    property: "scale".to_string(),
                    value: animatix::ast::Expr::Num(0.5),
                    modifiers: vec![],
                }],
                span: None,
            },
        ];

        let timeline = Timeline::build(&ast);
        assert_eq!(timeline_duration_seconds(&timeline), 2.0);
    }

    #[test]
    fn scene_dimensions_use_config_resolution_when_present() {
        let ast = vec![Stmt::Config {
            settings: vec![Property {
                name: "resolution".to_string(),
                value: Expr::Tuple(vec![Expr::Num(1280.0), Expr::Num(720.0)]),
            }],
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
        let ast = vec![Stmt::Comment("no config".to_string())];

        assert_eq!(document_scene_dimensions(&ast), SceneDimensions::default());
    }

    #[test]
    fn timeline_keyframe_times_are_sorted_and_deduped() {
        let ast = vec![
            Stmt::Keyframe {
                time: Time::Seconds(0.0),
                body: vec![Stmt::ActorDecl {
                    is_pub: false,
                    label: "box".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
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
                }],
                span: None,
            },
        ];

        let timeline = Timeline::build(&ast);
        assert_eq!(
            timeline_keyframe_times_s(&timeline),
            vec![0.0, 1.0, 2.0, 3.5]
        );
    }

    #[test]
    fn rebuild_uses_expanded_program_pipeline_for_imported_components() {
        let dir = temp_project_dir("document_rebuild_components");
        let entry = dir.join("scene.amx");
        let library = dir.join("components.amx");

        write_file(
            &library,
            r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: blue
    title_text: Text { text: title, at: (0, -20) }
    badge: Circle, radius: 12, color: gold
    badge.color = red
}
"#,
        );

        write_file(
            &entry,
            r#"
config { resolution: (1280, 720) }
import "./components.amx"

card: MetricCard, title: "Latency"
"#,
        );

        let document = DocumentSession::load(entry).expect("document should rebuild");
        let timeline = document.timeline.as_ref().expect("timeline should exist");

        assert!(timeline.tracks.contains_key("card"));
        assert!(timeline.tracks.contains_key("card.title_text"));
        assert!(timeline.tracks.contains_key("card.badge"));
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
    fn rebuild_surfaces_duplicate_component_exports_and_clears_compiled_state() {
        let dir = temp_project_dir("document_rebuild_duplicate_components");
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
        );

        write_file(
            &second,
            r#"
pub component MetricCard(title: "Two") {
    title_text: Text { text: title }
}
"#,
        );

        write_file(
            &entry,
            r#"
import "./first.amx"
import "./second.amx"

card: MetricCard
"#,
        );

        let mut document = DocumentSession::from_error(entry.clone());
        document.source_text = fs::read_to_string(&entry).unwrap();

        let error = document
            .rebuild()
            .expect_err("duplicate exports should fail");

        assert!(error.contains("Duplicate component export 'MetricCard'"));
        assert!(document.expanded_statements.is_none());
        assert!(document.timeline.is_none());
        assert_eq!(document.diagnostics.len(), 1);
        let diagnostic = &document.diagnostics[0];
        assert_eq!(
            diagnostic.severity,
            animatix::diagnostics::DiagnosticSeverity::Error
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
        let dir = temp_project_dir("document_rebuild_parse_failure");
        let entry = dir.join("scene.amx");

        write_file(
            &entry,
            r#"
scene: Rect, size: (100, 100)
"#,
        );

        let mut document =
            DocumentSession::load(entry.clone()).expect("valid document should load");
        document.set_source_text("scene: Rect {".to_string());

        let error = document
            .rebuild()
            .expect_err("invalid source should fail rebuild");

        assert!(!error.is_empty());
        assert!(document.expanded_statements.is_none());
        assert!(document.timeline.is_none());
        assert_eq!(document.diagnostics.len(), 1);
        let diagnostic = &document.diagnostics[0];
        assert_eq!(
            diagnostic.severity,
            animatix::diagnostics::DiagnosticSeverity::Error
        );
        assert_eq!(diagnostic.phase, DiagnosticPhase::Parse);
        assert_eq!(diagnostic.code, DiagnosticCode::SourceLoadFailure);
        assert!(
            diagnostic
                .message
                .contains("Failed to load or parse source:")
        );
        assert_eq!(diagnostic.location.path.as_ref(), Some(&entry));
        assert!(document.last_rebuild_error.is_some());
        assert_eq!(document.duration_s, 0.1);
        assert_eq!(document.scene_dimensions, SceneDimensions::default());
    }

    #[test]
    fn load_keeps_invalid_document_editable_with_parse_diagnostic() {
        let dir = temp_project_dir("document_load_parse_failure");
        let entry = dir.join("scene.amx");

        write_file(&entry, "scene: Rect {");

        let document =
            DocumentSession::load(entry.clone()).expect("source should still load into session");

        assert_eq!(document.file_path, entry);
        assert_eq!(document.source_text, "scene: Rect {");
        assert!(document.expanded_statements.is_none());
        assert!(document.timeline.is_none());
        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].phase, DiagnosticPhase::Parse);
        assert_eq!(
            document.diagnostics[0].code,
            DiagnosticCode::SourceLoadFailure
        );
        assert!(document.last_rebuild_error.is_some());
    }
}
