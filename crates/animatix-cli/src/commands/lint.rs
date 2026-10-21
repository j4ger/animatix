use std::path::PathBuf;

use tracing::error;

use crate::diagnostics::{OutputFormat, fold_hints, semantic_diagnostic_to_json};
use crate::extensions::CliExtensions;

pub fn run(
    paths: &[PathBuf],
    format: &OutputFormat,
    deny_warnings: bool,
    config: Option<&PathBuf>,
    extensions: &CliExtensions,
) {
    // Load lint config from file if specified
    let file_config =
        config.map(|p| animatix_analyzer::LintConfig::from_file(p)).unwrap_or_default();

    let mut total_errors = 0;
    let mut total_warnings = 0;
    let mut all_diagnostics = Vec::new();

    for path in paths {
        let files = if path.is_dir() {
            walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "amx"))
                .map(|e| e.path().to_path_buf())
                .collect::<Vec<_>>()
        } else {
            vec![path.clone()]
        };

        for file in files {
            let source = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    error!("{}: Failed to read: {}", file.display(), e);
                    total_errors += 1;
                    continue;
                },
            };

            let mut analyzer =
                animatix_analyzer::Analyzer::new_with_path(&source, Some(file.clone()))
                    .with_extension_manifest(extensions.merged_manifest());
            analyzer.merge_import_symbols();
            // Merge inline config with file config
            let mut config = animatix_analyzer::LintConfig::from_source(&source);
            config.merge(&file_config);
            let diagnostics = analyzer.diagnostics_with_config(&config);

            if !diagnostics.is_empty() {
                match format {
                    OutputFormat::Text => {
                        let (shown, folded) = fold_hints(&diagnostics);
                        for diag in shown {
                            println!("{}:{}", file.display(), diag);
                        }
                        if folded > 0 {
                            println!(
                                "{}: {folded} hint(s) folded — --format json lists them",
                                file.display()
                            );
                        }
                    },
                    OutputFormat::Json => {
                        for diag in &diagnostics {
                            // Same per-diagnostic shape as `check
                            // --format json`, plus the file it came
                            // from; lint sweeps many files.
                            let mut value = semantic_diagnostic_to_json(diag);
                            if let Some(object) = value.as_object_mut() {
                                object.insert(
                                    "file".to_string(),
                                    serde_json::Value::String(file.display().to_string()),
                                );
                            }
                            all_diagnostics.push(value);
                        }
                    },
                }

                total_errors += diagnostics.iter().filter(|d| d.is_error()).count();
                total_warnings += diagnostics.iter().filter(|d| d.is_warning()).count();
            }
        }
    }

    match format {
        OutputFormat::Json => match serde_json::to_string_pretty(&all_diagnostics) {
            Ok(json) => println!("{json}"),
            Err(err) => {
                tracing::error!("Failed to serialize diagnostics JSON: {err}");
                std::process::exit(1);
            },
        },
        OutputFormat::Text => {
            if total_errors > 0 || total_warnings > 0 {
                println!("\n{} error(s), {} warning(s)", total_errors, total_warnings);
            }
        },
    }

    if total_errors > 0 || (deny_warnings && total_warnings > 0) {
        std::process::exit(1);
    }
}
