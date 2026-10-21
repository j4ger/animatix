use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};

use crate::commands::storyboard::compute_auto_storyboard_times;
use crate::diagnostics::{
    backticked_label, diagnostic_to_json, module_error_diagnostics,
    resolve_build_diagnostic_positions, semantic_diagnostic_to_json,
};
use crate::extensions::{is_manifest_path, is_native_library_path, load_cli_extensions};

#[test]
fn check_and_lint_json_share_one_diagnostic_shape() {
    // Consumers parse both commands with one schema, so a diagnostic from
    // either family must carry the same keys (`code`, `severity`, `phase`).
    let build = diagnostic_to_json(&Diagnostic::error(
        DiagnosticCode::ParseError,
        DiagnosticPhase::Parse,
        "boom",
    ));
    for key in [
        "line", "col", "end_line", "end_col", "message", "code", "severity", "phase",
    ] {
        assert!(build.get(key).is_some(), "build diagnostic is missing `{key}`");
    }

    let semantic = semantic_diagnostic_to_json(&animatix_analyzer::Diagnostic {
        severity: animatix_analyzer::DiagnosticSeverity::Warning,
        line: 3,
        col: 4,
        end_line: 3,
        end_col: 9,
        message: "Unused actor: 'x'".to_string(),
        code: Some("unused-label".to_string()),
    });
    for key in [
        "line", "col", "end_line", "end_col", "message", "code", "severity", "phase",
    ] {
        assert!(semantic.get(key).is_some(), "semantic diagnostic is missing `{key}`");
    }
    assert_eq!(semantic["severity"], "warning");
    assert_eq!(semantic["phase"], "build");
    assert_eq!(semantic["line"], 3);
}

#[test]
fn module_parse_errors_become_structured_diagnostics() {
    // A module that fails to parse must produce one diagnostic per parse
    // error, tagged with the stable parse-error code — not a bare tracing
    // line, which `--format json` could not surface.
    let (ast, errors) =
        animatix_syntax::parser::parse_source("title: Text {\n  text: \"x\"\n#0s\n");
    assert!(ast.is_none() || !errors.is_empty(), "source must be malformed");
    let source = "title: Text {\n  text: \"x\"\n#0s\n";
    let err = animatix_syntax::module::ModuleError::ParseErrors(errors);
    let diagnostics = module_error_diagnostics(&err, source);
    assert!(!diagnostics.is_empty(), "parse failure must yield diagnostics");
    assert!(
        diagnostics.iter().all(|d| d.code == DiagnosticCode::ParseError && d.is_error()),
        "every parse diagnostic is a parse-error: {diagnostics:?}"
    );
}

#[test]
fn non_parse_module_errors_become_one_diagnostic() {
    let err = animatix_syntax::module::ModuleError::FileNotFound(PathBuf::from("/nope.amx"));
    let diagnostics = module_error_diagnostics(&err, "");
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].is_error());
    assert!(diagnostics[0].message.contains("/nope.amx"));
}

#[test]
fn plugin_paths_are_classified() {
    assert!(is_manifest_path(Path::new("demo.amx-plugin.toml")));
    assert!(!is_manifest_path(Path::new("demo.toml")));
    assert!(is_native_library_path(Path::new("libdemo.so")));
    assert!(is_native_library_path(Path::new("libdemo.dylib")));
    assert!(is_native_library_path(Path::new("demo.dll")));
    assert!(!is_native_library_path(Path::new("demo.txt")));
}

#[test]
fn relative_path_uses_common_ancestor() {
    let dir = std::env::temp_dir().join(format!(
        "animatix-cli-relative-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let from = dir.join("nested");
    let target = dir.join("libs").join("libdemo.so");
    std::fs::create_dir_all(&from).expect("create from dir");
    std::fs::create_dir_all(target.parent().expect("target parent")).expect("create target dir");
    std::fs::write(&target, b"demo").expect("write target");

    assert_eq!(
        animatix_plugin_tooling::relative_path(&from, &target),
        Some(PathBuf::from("../libs/libdemo.so"))
    );

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn load_cli_extensions_reads_manifest_without_native_library() {
    let dir = std::env::temp_dir().join(format!(
        "animatix-cli-plugin-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let manifest_path = dir.join("demo.amx-plugin.toml");
    std::fs::write(
        &manifest_path,
        "[[primitives]]\ntype_name = \"Gauge\"\n\n[[properties]]\nactor_type = \"Gauge\"\nname = \"level\"\ntype = \"Num\"\n",
    )
    .expect("write manifest");

    let extensions =
        load_cli_extensions(std::slice::from_ref(&manifest_path)).expect("load manifest");
    assert_eq!(extensions.manifests.len(), 1);
    assert_eq!(extensions.manifests[0].primitives[0].type_name, "Gauge");
    assert_eq!(
        extensions.merged_manifest().properties[0].name,
        "level",
        "manifest should be usable by the analyzer"
    );

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn backticked_label_extracts_actor_names() {
    assert_eq!(
        backticked_label("Actor `title` is declared before any keyframe"),
        Some("title".to_string())
    );
    // A label is a single token; a backticked phrase is not a label.
    assert_eq!(backticked_label("see `the docs here` for details"), None);
    assert_eq!(backticked_label("no backticks at all"), None);
    assert_eq!(backticked_label("empty `` label"), None);
}

#[test]
fn unpositioned_build_diagnostics_get_anchored() {
    // A `never-revealed` warning reaches the CLI with no location; the
    // analyzer resolves the quoted label to its declaration.
    let source = "title: Text, text: \"hi\"\n#0s\nfade-in title [1s]\n";
    let analyzer = animatix_analyzer::Analyzer::new(source);
    let mut diagnostics = vec![Diagnostic::warning(
        DiagnosticCode::NeverRevealed,
        DiagnosticPhase::Build,
        "Actor `title` is declared before any keyframe".to_string(),
    )];
    assert!(diagnostics[0].location.line.is_none(), "starts unpositioned");

    resolve_build_diagnostic_positions(&mut diagnostics, &analyzer);

    assert_eq!(diagnostics[0].location.line, Some(1), "anchored to the declaration");
    assert_eq!(diagnostics[0].location.column, Some(1));
}

#[test]
fn positioned_diagnostics_are_left_alone() {
    let source = "title: Text, text: \"hi\"\n";
    let analyzer = animatix_analyzer::Analyzer::new(source);
    let mut diagnostics = vec![
        Diagnostic::warning(
            DiagnosticCode::NeverRevealed,
            DiagnosticPhase::Build,
            "Actor `title` is declared before any keyframe".to_string(),
        )
        .with_location(9, 9, 0..1),
    ];

    resolve_build_diagnostic_positions(&mut diagnostics, &analyzer);

    assert_eq!(diagnostics[0].location.line, Some(9), "existing position is respected");
}

#[test]
fn auto_storyboard_times_sample_start_cues_and_rest() {
    let source = r#"
#0s
box: Rect, at: (0, 0), size: (10, 10)
#1s
fade-in box [500ms]
#3s
fade-out box [500ms]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty());
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast.unwrap(),
        &std::collections::HashMap::new(),
    );
    let target = BuildTarget::SingleScene(report.output);
    let times = compute_auto_storyboard_times(&target, 8);
    assert!(!times.is_empty());
    assert_eq!(times[0], 0.0, "must include start");
    assert!(times.contains(&1.0), "must include intermediate keyframe");
    assert!(times.last().copied().unwrap() >= 3.0, "must include end/rest frame");
}
