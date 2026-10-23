use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use animatix::timeline::{SceneDimensions, Timeline};
use animatix_syntax::ast::{Expr, Property, Stmt, Time};
use animatix_syntax::diagnostics::{DiagnosticCode, DiagnosticPhase};

use super::DocumentSession;
use super::load::extension_manifest_for_path;
use super::rebuild::document_scene_dimensions;
use super::save::{
    atomic_write, clear_recovery, recovery_is_newer, recovery_is_newer_than, recovery_path,
    write_recovery,
};
use super::timing::{timeline_duration_seconds, timeline_keyframe_times_s};
use crate::error::GuiError;

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
    fs::create_dir_all(&dir).map_err(|err| GuiError::Io {
        path: dir.clone(),
        source: err,
    })?;
    Ok(dir)
}

fn write_file(path: &Path, contents: &str) -> Result<(), GuiError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| GuiError::Io {
            path: parent.to_path_buf(),
            source: err,
        })?;
    }
    fs::write(path, contents).map_err(|err| GuiError::Io {
        path: path.to_path_buf(),
        source: err,
    })?;
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
                array_index: None,
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
                target: vec![animatix_syntax::ast::TargetSegment::Static(
                    "box".to_string(),
                )],
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
    // Empty AST still produces a default background_color keyframe at t=0
    assert_eq!(times, vec![0.0], "expected [0.0] (default bg keyframe), got {times:?}");
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
                array_index: None,
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
                target: vec![animatix_syntax::ast::TargetSegment::Static(
                    "box".to_string(),
                )],
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
                target: vec![animatix_syntax::ast::TargetSegment::Static(
                    "box".to_string(),
                )],
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
                target: vec![animatix_syntax::ast::TargetSegment::Static(
                    "box".to_string(),
                )],
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
    sorted.sort_by(f64::total_cmp);
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
    )
    .unwrap();

    write_file(
        &entry,
        r#"
config { resolution: (1280, 720) }
import "./components.amx"

card: MetricCard, title: "Latency"
"#,
    )
    .unwrap();

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
    )
    .unwrap();

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
    )
    .unwrap();

    write_file(
        &second,
        r#"
pub component MetricCard(title: "Two") {
title_text: Text { text: title }
}
"#,
    )
    .unwrap();

    write_file(
        &entry,
        r#"
import "./first.amx"
import "./second.amx"

card: MetricCard
"#,
    )
    .unwrap();

    let mut document = DocumentSession::from_error(entry.clone());
    document.source_text = fs::read_to_string(&entry)
        .unwrap_or_else(|_| panic!("failed to read test file: {:?}", &entry));

    let error = document.rebuild().expect_err("duplicate exports should fail");

    assert!(error.to_string().contains("Duplicate component export 'MetricCard'"));
    assert!(document.expanded_statements.is_none());
    assert!(document.timeline.is_none());
    assert_eq!(document.diagnostics.len(), 1);
    let diagnostic = &document.diagnostics[0];
    assert_eq!(diagnostic.severity, animatix_syntax::diagnostics::DiagnosticSeverity::Error);
    assert_eq!(diagnostic.phase, DiagnosticPhase::Parse);
    assert_eq!(diagnostic.code, DiagnosticCode::SourceLoadFailure);
    assert!(diagnostic.message.contains("Duplicate component export 'MetricCard'"));
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
    )
    .unwrap();

    let mut document = DocumentSession::load(entry.clone()).expect("valid document should load");
    document.set_source_text("scene: Rect {".to_string());

    let error = document.rebuild().expect_err("invalid source should fail rebuild");

    assert!(!error.to_string().is_empty());
    assert!(document.expanded_statements.is_none());
    assert!(document.timeline.is_none());
    assert_eq!(document.diagnostics.len(), 1);
    let diagnostic = &document.diagnostics[0];
    assert_eq!(diagnostic.severity, animatix_syntax::diagnostics::DiagnosticSeverity::Error);
    assert_eq!(diagnostic.phase, DiagnosticPhase::Parse);
    assert_eq!(diagnostic.code, DiagnosticCode::ParseError);
    assert!(!diagnostic.message.is_empty(), "parse error message should not be empty");
    assert!(diagnostic.location.line.is_some());
    assert!(diagnostic.location.column.is_some());
    assert_eq!(diagnostic.location.path.as_ref(), Some(&entry));
    assert!(document.last_rebuild_error.is_some());
    assert_eq!(document.duration_s, 0.1);
    assert_eq!(document.scene_dimensions, SceneDimensions::default());
}

#[test]
fn atomic_write_replaces_existing_file() {
    let dir = temp_project_dir("document_atomic_write").unwrap();
    let entry = dir.join("scene.amx");
    std::fs::write(&entry, "old").unwrap();

    atomic_write(&entry, "new").unwrap();

    assert_eq!(std::fs::read_to_string(&entry).unwrap(), "new");
    let temp_path = dir.join(format!(".scene.amx.{}.tmp", std::process::id()));
    assert!(!temp_path.exists());
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

#[test]
fn apply_rebuild_failure_keeps_last_good_ast_and_timeline() {
    let dir = temp_project_dir("document_apply_rebuild_failure").unwrap();
    let entry = dir.join("scene.amx");

    write_file(&entry, "#0s\nbox: Rect, size: (100, 100)\n").unwrap();

    let mut document = DocumentSession::load(entry.clone()).expect("valid document should load");
    assert!(document.raw_statements.is_some());
    assert!(document.timeline.is_some());

    document.apply_rebuild_failure(&crate::app::document::rebuild_output::RebuildFailure {
        error: "rejected source".to_string(),
        diagnostics: vec![animatix_syntax::diagnostics::Diagnostic::error(
            animatix_syntax::diagnostics::DiagnosticCode::ParseError,
            animatix_syntax::diagnostics::DiagnosticPhase::Parse,
            "rejected source",
        )],
        partial_source_index: None,
    });

    assert_eq!(document.last_rebuild_error.as_deref(), Some("rejected source"));
    assert_eq!(document.diagnostics.len(), 1);
    assert!(document.raw_statements.is_some(), "raw AST should survive failure");
    assert!(document.timeline.is_some(), "last-known-good timeline should survive failure");
    assert!(document.source_index.is_some(), "last-known-good source index should survive");
}

#[test]
fn failed_rebuild_marker_forces_next_rebuild_retry() {
    let dir = temp_project_dir("document_failure_retry").unwrap();
    let entry = dir.join("scene.amx");

    write_file(&entry, "#0s\nbox: Rect, size: (100, 100)\n").unwrap();

    let mut document = DocumentSession::load(entry.clone()).expect("valid document should load");
    document.last_rebuild_error = Some("stale failure".to_string());

    document.rebuild().expect("rebuild should retry despite the failure marker");
    assert!(document.last_rebuild_error.is_none());
    assert!(document.timeline.is_some());
}

#[test]
fn document_session_loads_extension_manifest_from_directory() {
    let dir = temp_project_dir("document_extension_manifest").unwrap();
    let entry = dir.join("scene.amx");

    write_file(&entry, "#0s\n").unwrap();
    write_file(&dir.join("demo.amx-plugin.toml"), "[[primitives]]\ntype_name = \"Gauge\"\n")
        .unwrap();

    let manifest = extension_manifest_for_path(&entry);
    assert_eq!(manifest.primitives.len(), 1);
    assert_eq!(manifest.primitives[0].type_name, "Gauge");

    let document = DocumentSession::from_source(entry, "#0s\n".to_string()).unwrap();
    assert_eq!(document.extension_manifest.primitives.len(), 1);
}

#[test]
fn recovery_path_is_a_sibling_sidecar() {
    assert_eq!(
        recovery_path(Path::new("/tmp/demo/scene.amx")),
        PathBuf::from("/tmp/demo/scene.amx.autosave")
    );
    // A path with no file name still yields a usable sibling.
    assert_eq!(recovery_path(Path::new("/")), PathBuf::from("/animatix.amx.autosave"));
}

#[test]
fn recovery_is_newer_than_compares_mtimes_strictly() {
    let base = UNIX_EPOCH + Duration::from_secs(1_000);
    let newer = base + Duration::from_secs(1);

    assert!(recovery_is_newer_than(newer, base));
    assert!(!recovery_is_newer_than(base, base), "equal mtimes are not newer");
    assert!(!recovery_is_newer_than(base, newer));
}

#[test]
fn recovery_detection_reads_filesystem_mtimes() {
    let dir = temp_project_dir("recovery_detection").unwrap();
    let source = dir.join("scene.amx");
    write_file(&source, "#0s\n").unwrap();

    // Backdate the source so the recovery write is unambiguously newer.
    let old = SystemTime::now() - Duration::from_secs(600);
    fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(old)
        .unwrap();

    assert!(!recovery_is_newer(&source), "no sidecar yet");
    assert!(!clear_recovery(&source), "clearing a missing sidecar is a no-op");

    write_recovery(&source, "#0s\nbox: Rect, size: (100, 100)\n").unwrap();
    assert!(recovery_is_newer(&source));
    assert_eq!(
        fs::read_to_string(recovery_path(&source)).unwrap(),
        "#0s\nbox: Rect, size: (100, 100)\n"
    );

    assert!(clear_recovery(&source));
    assert!(!recovery_path(&source).exists());
    assert!(!recovery_is_newer(&source));
}

#[test]
fn document_session_and_snapshot_expose_pause_points() {
    let source = "#0s\nb: Rect, size: (100, 100)\n#step\n\n#1s\nb.opacity = 0.5\n#pause\n";
    let mut session =
        DocumentSession::from_source(PathBuf::from("test.amx"), source.to_string()).unwrap();
    session.rebuild().unwrap();

    let pts = session.pause_points_s();
    assert_eq!(pts.len(), 2);
    assert_eq!(pts, vec![0.0, 1.0]);
}
