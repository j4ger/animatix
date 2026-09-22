//! Golden-file regression suite over deliberately broken `.amx` samples.
//!
//! Each `tests/broken_corpus/<case>.amx` ships with a `<case>.expected.json`
//! golden pinning the diagnostics the analyzer must produce (code, severity,
//! 0-based line/col). The corpus is the regression net for diagnostics-layer
//! work: a fix that changes what broken source reports must update the
//! corresponding golden in the same commit, so every behavior change is
//! reviewed instead of silent.
//!
//! Regenerate a golden after a deliberate change by pasting the `actual`
//! block from the assertion message.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use animatix_analyzer::Analyzer;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, serde::Serialize)]
struct ExpectedDiagnostic {
    code: String,
    severity: String,
    line: usize,
    col: usize,
}

#[derive(Debug, Deserialize)]
struct ExpectedFile {
    diagnostics: Vec<ExpectedDiagnostic>,
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/broken_corpus")
}

fn actual_diagnostics(source: &str) -> BTreeSet<ExpectedDiagnostic> {
    Analyzer::new(source)
        .diagnostics()
        .into_iter()
        .map(|d| ExpectedDiagnostic {
            code: d.code.unwrap_or_default(),
            severity: match d.severity {
                animatix_analyzer::DiagnosticSeverity::Error => "error".into(),
                animatix_analyzer::DiagnosticSeverity::Warning => "warning".into(),
                animatix_analyzer::DiagnosticSeverity::Info => "info".into(),
                animatix_analyzer::DiagnosticSeverity::Hint => "hint".into(),
            },
            line: d.line,
            col: d.col,
        })
        .collect()
}

#[test]
fn broken_corpus_matches_goldens() {
    let dir = corpus_dir();
    let mut cases: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("corpus directory exists")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension().is_some_and(|ext| ext == "amx")).then_some(path)
        })
        .collect();
    cases.sort();

    assert!(
        !cases.is_empty(),
        "corpus must not be empty: {}",
        dir.display()
    );

    for amx_path in cases {
        let source = fs::read_to_string(&amx_path).expect("read corpus source");
        let golden_path = amx_path.with_extension("expected.json");
        let expected: ExpectedFile = serde_json::from_str(
            &fs::read_to_string(&golden_path)
                .unwrap_or_else(|err| panic!("missing golden {}: {err}", golden_path.display())),
        )
        .expect("golden parses");

        let actual = actual_diagnostics(&source);
        let expected_set: BTreeSet<ExpectedDiagnostic> = expected.diagnostics.into_iter().collect();

        assert_eq!(
            actual, expected_set,
            "diagnostics drifted for {}\nactual = {}",
            amx_path.display(),
            serde_json::to_string_pretty(&actual).expect("serialize actual"),
        );
    }
}

/// Every `.amx` file shipped in the repository (examples, dogfood, probes)
/// must parse without errors and analyze without panicking. This pins the
/// invariant that shipped content is clean; a commit that breaks parsing of
/// real content fails here instead of at someone's next render.
#[test]
fn repo_content_parses_clean() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut checked = 0usize;
    let mut failures = Vec::new();

    for dir in ["examples", "dogfood"] {
        let root = repo_root.join(dir);
        if !root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || path.extension().is_none_or(|ext| ext != "amx") {
                continue;
            }
            let source = fs::read_to_string(path).expect("read repo content");
            let analyzer = Analyzer::new(&source);
            checked += 1;
            for error in analyzer.parse_errors() {
                failures.push(format!(
                    "{}:{}:{}: {}",
                    path.display(),
                    error.line,
                    error.column,
                    error.message
                ));
            }
        }
    }

    assert!(
        checked >= 100,
        "expected to sweep the full repo corpus, saw {checked} files"
    );
    assert!(
        failures.is_empty(),
        "shipped content must parse cleanly:\n{}",
        failures.join("\n")
    );
}
