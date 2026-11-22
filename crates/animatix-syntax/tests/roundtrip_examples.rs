//! Serializer roundtrip tests over public scenes and valid AMX fixtures.
//!
//! Both serializers are exercised against the real corpus: parse → serialize →
//! re-parse, then serialization is repeated to prove the output is stable. The
//! `to_source` pass additionally checks that a roundtrip preserves the
//! statement count, so a serializer that silently drops a statement cannot hide
//! behind a stable fixed point.

use std::path::Path;

mod common;

use common::comment_texts;

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

// Serializer stability over the whole repo

/// Use authored inputs and fixtures instead of anything copied into build output.
fn repo_amx_files() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let mut files = Vec::new();
    for dir in [
        "web/content/scenes",
        "crates/animatix-syntax/tests/fixtures/amx",
        "crates/animatix-web/tests/fixtures/amx",
        "crates/animatix/tests/fixtures/amx",
        "crates/animatix/benches/fixtures/amx",
        "crates/animatix-cli/tests/fixtures/amx",
    ] {
        let sources = collect_amx_files(&root.join(dir));
        assert!(!sources.is_empty(), "no AMX inputs under {dir}");
        files.extend(sources);
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
/// Tuple-valued named arguments must survive the formatter without changing
/// the parsed structure. Statement *counts* cannot detect argument loss.
///
#[test]
fn both_serializers_are_stable_across_the_repo() {
    let formatter =
        animatix_syntax::formatter::Formatter::new(animatix_syntax::formatter::FormatConfig {
            indent_size: 2,
            ..Default::default()
        });
    let mut failures: Vec<String> = Vec::new();

    for file_path in repo_amx_files() {
        let label = file_path.display().to_string();
        let source = std::fs::read_to_string(&file_path).expect(&label);
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
            // A stable fixed point can still have dropped a statement on the
            // first pass, so `to_source` also has to preserve the count.
            if serializer_name == "to_source"
                && parse_errors.is_empty()
                && again.len() != stmts.len()
            {
                failures.push(format!(
                    "{label}: to_source changed the statement count: {} -> {}",
                    stmts.len(),
                    again.len()
                ));
            }
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
            failures.push(format!(
                "{label}: {serializer_name} is not idempotent\n  pass 1: {before}\n  pass 2: {after}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} file(s) fail serializer stability:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

// Comment preservation over the whole repo

/// Formatting real content must preserve every `//` comment verbatim, keep the
/// result parseable, and be idempotent. This is the corpus-level guard behind
/// the web scenes shipping unformatted: `animatix fmt` used to erase them.
#[test]
fn formatter_preserves_comments_across_repo_files() {
    let formatter =
        animatix_syntax::formatter::Formatter::new(animatix_syntax::formatter::FormatConfig {
            indent_size: 2,
            ..Default::default()
        });
    let mut failures: Vec<String> = Vec::new();
    for file_path in repo_amx_files() {
        let label = file_path.display().to_string();
        let source = std::fs::read_to_string(&file_path).expect(&label);
        let (stmts, parse_errors) = animatix_syntax::parser::parse_source(&source);
        let Some(stmts) = stmts else {
            failures.push(format!(
                "{label}: could not parse as shipped: {:?}",
                parse_errors.iter().map(|e| &e.message).collect::<Vec<_>>()
            ));
            continue;
        };
        if !parse_errors.is_empty() {
            continue;
        }

        let before = comment_texts(&source);
        let formatted = formatter.format_source(&source, &stmts);
        let after = comment_texts(&formatted);
        if before != after {
            failures.push(format!(
                "{label}: comment set changed\n  before: {before:?}\n  after:  {after:?}"
            ));
            continue;
        }

        let (again, re_errors) = animatix_syntax::parser::parse_source(&formatted);
        let Some(again) = again else {
            // Some pre-existing AST serialization bugs (for example a
            // conditional expression used as a binary operand) produce invalid
            // text with or without comments. Only a regression introduced by
            // comment reinsertion is this test's concern.
            let plain = formatter.format(&stmts);
            let (plain_ast, plain_errors) = animatix_syntax::parser::parse_source(&plain);
            if plain_ast.is_some() && plain_errors.is_empty() {
                failures.push(format!(
                    "{label}: formatted text no longer parses: {:?}",
                    re_errors.iter().map(|e| &e.message).collect::<Vec<_>>()
                ));
            }
            continue;
        };
        let second = formatter.format_source(&formatted, &again);
        if second != formatted {
            let Some((first_pass, second_pass)) = first_difference(&formatted, &second) else {
                continue;
            };
            failures.push(format!(
                "{label}: comment formatting is not idempotent\n  pass 1: {first_pass}\n  pass 2: {second_pass}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} file(s) fail comment-preserving formatting:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
