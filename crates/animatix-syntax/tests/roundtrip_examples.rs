//! Roundtrip tests for all `.amx` example files.
//!
//! This integration test dynamically discovers every `.amx` file in the
//! `examples/` directory, parses it, serializes the AST back to source, and
//! re-parses the result — verifying that the parser roundtrip is lossless
//! with respect to statement count.

use animatix_syntax::ast::Stmt;
use animatix_syntax::chumsky::Parser;
use std::path::Path;

/// Recursively collect all `.amx` files under `dir`.
fn collect_amx_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_amx_files(&path));
            } else if path.extension().is_some_and(|e| e == "amx") {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn roundtrip_all_example_files() {
    // CARGO_MANIFEST_DIR = crates/animatix-syntax/ — go up two levels to workspace root.
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .canonicalize()
        .expect("examples/ directory not found — is the workspace structure intact?");

    assert!(
        examples_dir.is_dir(),
        "examples/ directory does not exist at: {}",
        examples_dir.display()
    );

    let amx_files = collect_amx_files(&examples_dir);
    assert!(
        !amx_files.is_empty(),
        "no .amx files found in {}",
        examples_dir.display()
    );

    let mut failures: Vec<String> = Vec::new();

    for file_path in &amx_files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: read error: {}", file_path.display(), e));
                continue;
            }
        };

        // Phase 1: parse the original source using chumsky's into_output_errors
        let (parsed_opt, parse_errors) =
            animatix_syntax::parser::parser_simple()
                .parse(&source)
                .into_output_errors();
        let parsed: Vec<Stmt> = match parsed_opt {
            Some(stmts) if parse_errors.is_empty() => stmts,
            _ => {
                let msg: Vec<String> = parse_errors
                    .iter()
                    .map(|e| format!("  {}", e))
                    .collect();
                failures.push(format!(
                    "{}: parse failed ({} error(s)):\n{}",
                    file_path.display(),
                    parse_errors.len(),
                    msg.join("\n")
                ));
                continue;
            }
        };

        let orig_count = parsed.len();

        // Phase 2: serialize back to source
        let serialized = animatix_syntax::to_source::stmts_to_source(&parsed);

        // Phase 3: re-parse the serialized output
        let (reparsed_opt, reparse_errors) =
            animatix_syntax::parser::parser_simple()
                .parse(&serialized)
                .into_output_errors();
        let reparsed: Vec<Stmt> = match reparsed_opt {
            Some(stmts) if reparse_errors.is_empty() => stmts,
            _ => {
                let msg: Vec<String> = reparse_errors
                    .iter()
                    .map(|e| format!("  {}", e))
                    .collect();
                failures.push(format!(
                    "{}: re-parse failed after serialization ({} error(s)):\n{}",
                    file_path.display(),
                    reparse_errors.len(),
                    msg.join("\n")
                ));
                continue;
            }
        };

        // Phase 4: compare statement counts
        if reparsed.len() != orig_count {
            failures.push(format!(
                "{}: statement count mismatch: original={}, after roundtrip={}",
                file_path.display(),
                orig_count,
                reparsed.len()
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "roundtrip failures for {} file(s):\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}