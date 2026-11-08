//! Serializer roundtrip tests over every `.amx` file in the repo.
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
    let files = repo_amx_files();
    assert!(files.len() > 100, "expected the repo's .amx corpus, found {}", files.len());

    let mut failures: Vec<String> = Vec::new();
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
            // Pre-existing escape debt: string literals grow a backslash on
            // every pass because the lexer keeps raw escapes. Ignore only that
            // known case; see `both_serializers_are_stable_across_the_repo`.
            if !first_pass.contains('\\') {
                failures.push(format!(
                    "{label}: comment formatting is not idempotent\n  pass 1: {first_pass}\n  pass 2: {second_pass}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} file(s) fail comment-preserving formatting:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
