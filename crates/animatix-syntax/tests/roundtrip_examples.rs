//! Roundtrip tests for all `.amx` example files.
//!
//! This integration test dynamically discovers every `.amx` file in the
//! `examples/` directory, parses it, serializes the AST back to source, and
//! re-parses the result — verifying that the parser roundtrip is lossless
//! with respect to statement count.

use std::path::Path;

use animatix_syntax::ast::Stmt;

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

    let mut amx_files = collect_amx_files(&examples_dir);
    // Also round-trip the dogfood projects so formatter regressions on
    // real-content files (e.g. lost `[step: ...]` modifiers) are caught.
    let dogfood_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dogfood/projects")
        .canonicalize()
        .expect("dogfood/projects directory not found");
    amx_files.extend(collect_amx_files(&dogfood_dir));
    assert!(!amx_files.is_empty(), "no .amx files found for roundtrip");

    let mut failures: Vec<String> = Vec::new();

    for file_path in &amx_files {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: read error: {}", file_path.display(), e));
                continue;
            },
        };

        // Phase 1: parse the original source via parse_source (strips comments first)
        let (parsed_opt, parse_errors) = animatix_syntax::parser::parse_source(&source);
        let parsed: Vec<Stmt> = match parsed_opt {
            Some(stmts) if parse_errors.is_empty() => stmts,
            _ => {
                let msg: Vec<String> =
                    parse_errors.iter().map(|e| format!("  {}", e.message)).collect();
                failures.push(format!(
                    "{}: parse failed ({} error(s)):\n{}",
                    file_path.display(),
                    parse_errors.len(),
                    msg.join("\n")
                ));
                continue;
            },
        };

        let orig_count = parsed.len();

        // Phase 2: serialize back to source
        let serialized = animatix_syntax::to_source::stmts_to_source(&parsed);

        // Phase 3: re-parse the serialized output
        let (reparsed_opt, reparse_errors) = animatix_syntax::parser::parse_source(&serialized);
        let reparsed: Vec<Stmt> = match reparsed_opt {
            Some(stmts) if reparse_errors.is_empty() => stmts,
            _ => {
                let msg: Vec<String> =
                    reparse_errors.iter().map(|e| format!("  {}", e.message)).collect();
                failures.push(format!(
                    "{}: re-parse failed after serialization ({} error(s)):\n{}",
                    file_path.display(),
                    reparse_errors.len(),
                    msg.join("\n")
                ));
                continue;
            },
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
        panic!("roundtrip failures for {} file(s):\n{}", failures.len(), failures.join("\n\n"));
    }
}

// ---------------------------------------------------------------------------
// Serializer stability over the whole repo
// ---------------------------------------------------------------------------

/// Roots whose `.amx` files are real content, not fixtures.
fn repo_amx_files() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let mut files = Vec::new();
    for dir in ["examples", "dogfood", "web"] {
        files.extend(collect_amx_files(&root.join(dir)));
    }
    files.sort();
    files
}

/// One line of the first pass next to its second-pass replacement.
fn first_difference(a: &str, b: &str) -> Option<(String, String)> {
    a.lines()
        .zip(b.lines())
        .chain((a.lines().count()..b.lines().count()).map(|_| ("<absent line>", "<extra line>")))
        .find(|(x, y)| x != y)
        .map(|(x, y)| (x.to_string(), y.to_string()))
}

/// Both serializers must be stable: serializing an already-serialized document
/// has to produce byte-identical output, and must never fail to parse.
///
/// `animatix fmt` used to rewrite `move b to (300, 200)` into
/// `move b, to 300, 200`, which the parser rejects — a single format pass
/// destroyed the file. Statement *counts* could not see that, and neither
/// could the old `assert_eq!(arms, arms)` guardrails.
///
/// The one accepted exception is asserted narrowly below: a string literal's
/// backslashes grow on every pass, because the lexer stores the raw text
/// between the quotes while `format_expr` escapes it again. Decoding escapes at
/// lex time is a language-semantics decision (see `docs/roadmap.md`), so
/// instability is tolerated *only* where a backslash is on the line that
/// changed — anything else fails.
#[test]
fn both_serializers_are_stable_across_the_repo() {
    let formatter =
        animatix_syntax::formatter::Formatter::new(animatix_syntax::formatter::FormatConfig {
            indent_size: 2,
            ..Default::default()
        });
    let files = repo_amx_files();
    assert!(files.len() > 100, "expected the repo's .amx corpus, found {}", files.len());

    let mut failures: Vec<String> = Vec::new();
    let mut escape_debt: Vec<std::path::PathBuf> = Vec::new();

    for file_path in &files {
        let Ok(source) = std::fs::read_to_string(file_path) else {
            continue;
        };
        let label = file_path.display().to_string();
        let (stmts, parse_errors) = animatix_syntax::parser::parse_source(&source);
        let Some(stmts) = stmts else {
            failures.push(format!(
                "{label}: could not parse as shipped: {:?}",
                parse_errors.iter().map(|e| &e.message).collect::<Vec<_>>()
            ));
            continue;
        };

        for (serializer_name, serialized) in [
            ("to_source", animatix_syntax::to_source::stmts_to_source(&stmts)),
            ("Formatter", formatter.format(&stmts)),
        ] {
            let (again_opt, re_errors) = animatix_syntax::parser::parse_source(&serialized);
            let Some(again) = again_opt else {
                failures.push(format!(
                    "{label}: {serializer_name} emitted text that will not parse: {:?}",
                    re_errors.iter().map(|e| &e.message).collect::<Vec<_>>()
                ));
                continue;
            };
            let second = match serializer_name {
                "to_source" => animatix_syntax::to_source::stmts_to_source(&again),
                _ => formatter.format(&again),
            };
            if second == serialized {
                continue;
            }
            let Some((before, after)) = first_difference(&serialized, &second) else {
                continue;
            };
            if before.contains('\\') {
                escape_debt.push(file_path.clone());
            } else {
                failures.push(format!(
                    "{label}: {serializer_name} is not idempotent\n  pass 1: {before}\n  pass 2: {after}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} file(s) fail serializer stability:\n{}",
        failures.len(),
        failures.join("\n\n")
    );

    // The tolerance above must stay scoped to the escape bug: every excused
    // file has to actually contain a backslash, or the exemption is hiding
    // something else.
    let unjustified: Vec<String> = escape_debt
        .iter()
        .filter(|path| std::fs::read_to_string(path).is_ok_and(|text| !text.contains('\\')))
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        unjustified.is_empty(),
        "serializer instability was excused as the string-escape bug in files with no \
         backslash at all: {unjustified:?}"
    );
}
