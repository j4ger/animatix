use std::fs::{self, File};
use std::path::{Path, PathBuf};

use super::assets;
use super::history::{History, path_for};
use super::source::managed_pairs;

pub(in crate::app) struct SaveAsNarration {
    installed: Option<PathBuf>,
    _lease: Option<File>,
}

impl SaveAsNarration {
    pub fn prepare(from: &Path, to: &Path, text: &str) -> Result<Self, String> {
        let mut change = Self {
            installed: None,
            _lease: None,
        };
        let old = parent(from)?;
        let new = parent(to)?;
        if old == new && from.file_name() == to.file_name() {
            return Ok(change);
        }
        let history = path_for(to);
        if history.try_exists().map_err(|error| error.to_string())? {
            return Err("The destination already has a voiceover history.".into());
        }
        let source_history = History::read_only(from).map_err(|error| error.to_string())?;
        if source_history.is_some() {
            let name = history.file_name().ok_or("Invalid voiceover history path.")?;
            let lock = History::lease(&new.join(name)).map_err(|error| error.to_string())?;
            change._lease = Some(lock.ok_or("The destination voiceover history is in use.")?);
        }
        if old != new {
            for (audio, transcript) in managed_pairs(text) {
                assets::copy_pair(
                    &(old.join(&audio), old.join(&transcript)),
                    &(new.join(&audio), new.join(&transcript)),
                )
                .map_err(|error| format!("Could not copy narration: {error}"))?;
            }
        }
        if let Some(source_history) = source_history {
            source_history.backup(&history).map_err(|error| error.to_string())?;
            change.installed = Some(history);
        }
        Ok(change)
    }

    pub fn commit(mut self) {
        self.installed = None;
    }
}

impl Drop for SaveAsNarration {
    fn drop(&mut self) {
        if let Some(path) = self.installed.take()
            && let Err(error) = fs::remove_file(&path)
        {
            tracing::warn!(%error, ?path, "Could not remove the staged voiceover history");
        }
    }
}

fn parent(path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(
        path.parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::narration_takes::history::Take;

    #[test]
    fn save_as_copies_current_pair_and_backups_history_independently() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let original = first.path().join("source.amx");
        let new = second.path().join("fork.amx");
        let live = History::open(&original, true).unwrap().unwrap();
        let take = Take {
            id: "take".into(),
            paragraph: "paragraph".into(),
            scene: None,
            text: "Hello.".into(),
            kind: "recorded",
        };
        live.snapshot("saved source", &[], true).unwrap();
        live.start_recording(&take, 24_000, 1).unwrap();
        live.append_recording(&take.id, &[1, 2], 1).unwrap();
        live.finish_recording(&take.id, true).unwrap();
        fs::create_dir(first.path().join("narration")).unwrap();
        fs::write(first.path().join("narration/main-aabbccdd.wav"), b"wav").unwrap();
        fs::write(first.path().join("narration/main-aabbccdd.speech.json"), "{}").unwrap();
        let text = "narration: Audio, source: \"narration/main-aabbccdd.wav\", transcript: \"narration/main-aabbccdd.speech.json\"\n";
        drop(SaveAsNarration::prepare(&original, &new, text).unwrap());
        assert!(!path_for(&new).exists());
        SaveAsNarration::prepare(&original, &new, text).unwrap().commit();
        assert_eq!(fs::read(second.path().join("narration/main-aabbccdd.wav")).unwrap(), b"wav");
        let copied = History::open(&new, false).unwrap().unwrap();
        assert!(copied.bindings("saved source").unwrap().0);
        copied.snapshot("new destination source", &[], true).unwrap();
        assert!(!live.bindings("new destination source").unwrap().0);
        copied.issue(&take.id, Some("different")).unwrap();
        assert!(live.takes().unwrap()[0].issue.is_none());
        assert!(SaveAsNarration::prepare(&original, &new, text).is_err());
    }

    #[test]
    fn save_as_preserves_unreadable_source_and_occupied_destination_histories() {
        let folder = tempfile::tempdir().unwrap();
        let original = folder.path().join("source.amx");
        let fork = folder.path().join("fork.amx");
        let legacy = b"SQLite format 3\0legacy fixture";
        fs::write(path_for(&original), legacy).unwrap();

        assert!(SaveAsNarration::prepare(&original, &fork, "").is_err());
        assert_eq!(fs::read(path_for(&original)).unwrap(), legacy);
        assert!(!path_for(&fork).exists());

        let live = folder.path().join("live.amx");
        let history = History::open(&live, true).unwrap().unwrap();
        history.snapshot("saved source", &[], true).unwrap();
        fs::write(path_for(&fork), legacy).unwrap();

        assert!(SaveAsNarration::prepare(&live, &fork, "").is_err());
        assert_eq!(fs::read(path_for(&fork)).unwrap(), legacy);
        assert!(history.bindings("saved source").unwrap().0);
    }
}
