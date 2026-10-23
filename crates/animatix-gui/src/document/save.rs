//! Saving, disk reloads, atomic writes, and crash-recovery sidecars.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::DocumentSession;
use crate::error::GuiError;

impl DocumentSession {
    pub fn set_source_text(&mut self, source_text: String) {
        self.source_text = source_text;
        self.is_dirty = true;
        self.cached_expanded = None;
    }

    pub fn reload_from_disk(&mut self) -> Result<(), GuiError> {
        let path = self.file_path.clone();
        self.source_text =
            fs::read_to_string(&path).map_err(|err| GuiError::Io { path, source: err })?;
        self.is_dirty = false;
        Ok(())
    }

    pub fn save_to_disk(&mut self) -> Result<(), GuiError> {
        let path = self.file_path.clone();
        atomic_write(&path, &self.source_text)?;
        self.is_dirty = false;
        Ok(())
    }
}

pub fn default_file_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("untitled.amx")
}

pub(super) fn atomic_write(path: &Path, contents: &str) -> Result<(), GuiError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("animatix");
    let temp_path = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);

        #[cfg(windows)]
        if path.exists() {
            fs::remove_file(path)?;
        }

        fs::rename(&temp_path, path)
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result.map_err(|err| GuiError::Io {
        path: path.to_path_buf(),
        source: err,
    })
}

/// Sidecar path used for crash-recovery autosaves of `source_path`.
///
/// The recovery file is a sibling of the document so a whole-project copy or
/// delete moves it along with the source, and appending `.autosave` keeps it
/// from ever being confused with a normal `.amx` save.
pub(crate) fn recovery_path(source_path: &Path) -> PathBuf {
    let file_name = source_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "animatix.amx".to_string());
    source_path.with_file_name(format!("{file_name}.autosave"))
}

/// Atomically write `text` to `source_path`'s recovery sidecar.
pub(crate) fn write_recovery(source_path: &Path, text: &str) -> Result<(), GuiError> {
    atomic_write(&recovery_path(source_path), text)
}

/// Remove the recovery sidecar for `source_path`, if present.
///
/// Returns `true` when a file was removed. A missing sidecar is not an error:
/// the common case is a document that was saved normally and needed no
/// recovery, or a discard that already cleaned up.
pub(crate) fn clear_recovery(source_path: &Path) -> bool {
    let path = recovery_path(source_path);
    match fs::remove_file(&path) {
        Ok(()) => {
            tracing::debug!("Removed recovery file {}", path.display());
            true
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            tracing::warn!("Failed to remove recovery file {}: {}", path.display(), err);
            false
        },
    }
}

/// Whether a recovery sidecar for `source_path` exists and is strictly newer
/// than the source file itself.
///
/// A sidecar that is older than (or identical in age to) the source cannot hold
/// anything the source does not already have, so it is never offered.
pub(crate) fn recovery_is_newer(source_path: &Path) -> bool {
    let recovery = recovery_path(source_path);
    let (Ok(recovery_meta), Ok(source_meta)) = (fs::metadata(&recovery), fs::metadata(source_path))
    else {
        return false;
    };
    match (recovery_meta.modified(), source_meta.modified()) {
        (Ok(recovery_mtime), Ok(source_mtime)) => {
            recovery_is_newer_than(recovery_mtime, source_mtime)
        },
        _ => false,
    }
}

/// Pure mtime comparison behind [`recovery_is_newer`], split out for tests.
pub(crate) fn recovery_is_newer_than(
    recovery_mtime: std::time::SystemTime,
    source_mtime: std::time::SystemTime,
) -> bool {
    recovery_mtime > source_mtime
}
