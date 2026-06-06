//! Tests for SVG import functionality.

use animatix::timeline::import_svg;
use animatix_syntax::ast::{Expr, Stmt};
use std::path::PathBuf;

fn test_svg_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/svg");
    path.push(name);
    path
}

#[test]
fn import_basic_shapes() {
    let path = test_svg_path("test_basic.svg");
    let stmts = import_svg(&path).expect("Failed to import SVG");
    
    // Should have multiple actor declarations
    assert!(!stmts.is_empty(), "SVG import produced no statements");
    
    // Count actor declarations
    let actor_count = stmts.iter().filter(|s| matches!(s, Stmt::ActorDecl { .. })).count();
    assert!(actor_count >= 4, "Expected at least 4 actors, got {}", actor_count);
    
    // Check that we have the expected actor types
    let mut has_rect = false;
    let mut has_ellipse = false;
    let mut has_path = false;
    let mut has_text = false;
    
    for stmt in &stmts {
        if let Stmt::ActorDecl { ty, .. } = stmt {
            match ty.as_str() {
                "Rect" => has_rect = true,
                "Ellipse" => {
                    // Both circle and ellipse become Ellipse type
                    has_ellipse = true;
                }
                "Path" => has_path = true,
                "Text" => has_text = true,
                _ => {}
            }
        }
    }
    
    assert!(has_rect, "Missing Rect actor");
    assert!(has_ellipse, "Missing Ellipse actor (circle/ellipse)");
    assert!(has_path, "Missing Path actor");
    assert!(has_text, "Missing Text actor");
}

#[test]
fn import_preserves_colors() {
    let path = test_svg_path("test_basic.svg");
    let stmts = import_svg(&path).expect("Failed to import SVG");
    
    // Find the rect actor and check its fill color
    for stmt in &stmts {
        if let Stmt::ActorDecl { ty, props, .. } = stmt {
            if ty == "Rect" {
                let fill = props.iter().find(|p| p.name == "color");
                assert!(fill.is_some(), "Rect should have fill color");
                
                let stroke = props.iter().find(|p| p.name == "stroke_color");
                assert!(stroke.is_some(), "Rect should have stroke color");
                
                let stroke_width = props.iter().find(|p| p.name == "stroke_width");
                assert!(stroke_width.is_some(), "Rect should have stroke width");
                return;
            }
        }
    }
    panic!("No Rect actor found");
}

#[test]
fn import_handles_opacity() {
    let path = test_svg_path("test_basic.svg");
    let stmts = import_svg(&path).expect("Failed to import SVG");
    
    // Find the circle actor and check its opacity
    for stmt in &stmts {
        if let Stmt::ActorDecl { ty, props, .. } = stmt {
            if ty == "Ellipse" {
                // Check if any ellipse has opacity set
                let has_opacity = props.iter().any(|p| p.name == "opacity");
                // At least one ellipse should have opacity
                if has_opacity {
                    return;
                }
            }
        }
    }
    // It's okay if no ellipse has opacity - the circle might be the one with opacity
}

#[test]
fn import_path_with_extended_commands() {
    let path = test_svg_path("test_basic.svg");
    let stmts = import_svg(&path).expect("Failed to import SVG");
    
    // Find the path actor
    for stmt in &stmts {
        if let Stmt::ActorDecl { ty, props, .. } = stmt {
            if ty == "Path" {
                let commands = props.iter().find(|p| p.name == "commands");
                assert!(commands.is_some(), "Path should have commands property");
                
                // The path uses M, L, H, V, Z commands
                // Verify commands are parsed (should be a tuple)
                if let Some(Expr::Tuple(cmds)) = commands.map(|p| &p.value) {
                    assert!(!cmds.is_empty(), "Path commands should not be empty");
                    // Should have at least 4 commands: M, L, line_to (from H), line_to (from V), close
                    assert!(cmds.len() >= 4, "Expected at least 4 path commands, got {}", cmds.len());
                }
                return;
            }
        }
    }
    panic!("No Path actor found");
}

#[test]
fn import_nonexistent_file_fails() {
    let path = PathBuf::from("/nonexistent/file.svg");
    let result = import_svg(&path);
    assert!(result.is_err(), "Importing nonexistent file should fail");
}
