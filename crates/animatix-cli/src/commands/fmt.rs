use std::path::{Path, PathBuf};
use tracing::{error, info};

pub fn run(paths: &[PathBuf], check: bool, indent: usize) {
    let config = animatix_syntax::formatter::FormatConfig {
        indent_size: indent,
        ..Default::default()
    };
    let formatter = animatix_syntax::formatter::Formatter::new(config);
    let mut has_changes = false;

    for path in paths {
        if path.is_dir() {
            // Format all .amx files in directory
            for entry in walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "amx"))
            {
                if let Err(e) = format_file(entry.path(), &formatter, check) {
                    error!("{}: {}", entry.path().display(), e);
                    has_changes = true;
                }
            }
        } else {
            if let Err(e) = format_file(path, &formatter, check) {
                error!("{}: {}", path.display(), e);
                has_changes = true;
            }
        }
    }

    if check && has_changes {
        error!("Some files are not formatted. Run 'animatix fmt' to fix.");
        std::process::exit(1);
    }
}

/// Format a single .amx file.
///
/// If `check` is true, only checks if the file is formatted (doesn't modify).
/// Returns Ok(()) if the file is already formatted or was formatted successfully.
pub fn format_file(
    path: &Path,
    formatter: &animatix_syntax::formatter::Formatter,
    check: bool,
) -> Result<(), String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("Failed to read: {}", e))?;

    let (stmts, errors) = animatix_syntax::parser::parse_simple(&source);
    let stmts = stmts.ok_or_else(|| format!("Parse error: {:?}", errors))?;

    let formatted = formatter.format(&stmts);

    if check {
        if source != formatted {
            return Err("File is not formatted".into());
        }
    } else {
        if source != formatted {
            std::fs::write(path, &formatted).map_err(|e| format!("Failed to write: {}", e))?;
            info!("Formatted: {}", path.display());
        }
    }

    Ok(())
}
