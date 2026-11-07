#![cfg(feature = "svg")]
//! Integration sanity tests for the SVG import pipeline from disk.

use std::path::PathBuf;

use animatix::timeline::import_svg;
use animatix_syntax::ast::Stmt;

fn test_svg_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/svg");
    path.push(name);
    path
}

#[test]
fn import_svg_from_disk_pipeline() {
    let path = test_svg_path("test_basic.svg");
    let stmts = import_svg(&path).expect("Failed to import SVG fixture from disk");

    // Verify scene configuration was parsed from the SVG root
    assert!(
        stmts.iter().any(|s| matches!(s, Stmt::Config { .. })),
        "Expected scene config from SVG viewBox"
    );

    // Verify actor declarations for the shapes present in the fixture
    let actor_types: Vec<&str> = stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::ActorDecl { ty, .. } => Some(ty.as_str()),
            _ => None,
        })
        .collect();

    for expected in ["Rect", "Ellipse", "Path", "Text"] {
        assert!(
            actor_types.contains(&expected),
            "Missing {expected} actor declaration in imported SVG"
        );
    }
}

#[test]
fn import_nonexistent_file_fails() {
    let path = PathBuf::from("/nonexistent/file.svg");
    assert!(import_svg(&path).is_err(), "Importing nonexistent file should fail");
}
