//! Observe media revisions independently of dirty source edits and undo history.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub(crate) const CHECK_INTERVAL: Duration = Duration::from_millis(500);

#[derive(PartialEq, Eq)]
struct FileStamp {
    modified: SystemTime,
    length: u64,
    #[cfg(unix)]
    identity: (u64, i64, i64),
}

fn stamp(path: &Path) -> Option<FileStamp> {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        modified: meta.modified().ok()?,
        length: meta.len(),
        #[cfg(unix)]
        identity: (meta.ino(), meta.ctime(), meta.ctime_nsec()),
    })
}

#[derive(Default)]
pub(crate) struct MediaWatch {
    revision: Option<u64>,
    stamps: BTreeMap<PathBuf, Option<FileStamp>>,
    next_check: Option<Instant>,
}

impl MediaWatch {
    pub(crate) fn active(&self) -> bool {
        !self.stamps.is_empty()
    }

    pub(crate) fn changed(
        &mut self,
        now: Instant,
        revision: u64,
        paths: impl FnOnce() -> BTreeSet<PathBuf>,
    ) -> bool {
        if self.revision != Some(revision) {
            let paths = paths();
            self.stamps.retain(|path, _| paths.contains(path));
            for path in paths {
                self.stamps.entry(path).or_insert_with_key(|p| stamp(p));
            }
            self.revision = Some(revision);
        }
        if self.next_check.is_some_and(|deadline| deadline > now) {
            return false;
        }
        self.next_check = Some(now + CHECK_INTERVAL);
        let mut changed = false;
        for (path, previous) in &mut self.stamps {
            let current = stamp(path);
            if *previous != current {
                *previous = current;
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replace_delete_and_recreate_without_source_edits() {
        let dir = crate::app::test_support::TestTempDir::new("media-watch");
        let path = dir.path().join("speech.json");
        std::fs::write(&path, "old").unwrap();
        let mut watch = MediaWatch::default();
        let mut now = Instant::now();
        assert!(!watch.changed(now, 1, || BTreeSet::from([path.clone()])));
        for content in [Some("a new take"), None, Some("recreated")] {
            now += CHECK_INTERVAL;
            if let Some(text) = content {
                std::fs::write(&path, text).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
            assert!(watch.changed(now, 1, || unreachable!()));
            assert!(!watch.changed(now, 1, || unreachable!()));
        }
    }
    #[test]
    fn external_transcript_rebuild_keeps_dirty_editor_and_playback_clock() {
        use crate::app::GuiShell;
        let dir = crate::app::test_support::TestTempDir::new("retake-dirty");
        let path = dir.path().join("demo.amx");
        let transcript = dir.path().join("voice.speech.json");
        let disk = "config { duration: 5 }\nvoice: Audio, source: \"a.wav\", transcript: \"voice.speech.json\"\nbox: Rect, opacity: 0\n#voice@cue\n  box.opacity = 1";
        let write_take = |time: f64| {
            std::fs::write(
                &transcript,
                serde_json::json!({"markers":{"cue":time},"segments":[]}).to_string(),
            )
            .unwrap()
        };
        write_take(1.0);
        std::fs::write(&path, disk).unwrap();
        let mut shell = GuiShell::load(path.clone(), false);
        let start = Instant::now();
        shell.check_hot_reload(start);
        let text = format!("{disk}\n// unsaved edit");
        shell.document_store.source.replace_text(text.clone());
        shell.preview_store.preview.playback.current_time_s = 2.5;
        write_take(3.25);
        shell.check_hot_reload(start + CHECK_INTERVAL);
        assert!(shell.document_store.source.media_changed);
        // Debouncing on the watcher has elapsed; run the normal worker/accept path.
        shell.preview_store.pending_rebuild_at = Some(Instant::now());
        let deadline = Instant::now() + Duration::from_secs(10);
        while shell.document_store.source.media_changed && Instant::now() < deadline {
            shell.prepare_frame();
            std::thread::sleep(Duration::from_millis(5));
        }
        let source = &shell.document_store.source;
        assert!(!source.media_changed, "{:?}", source.document.last_rebuild_error);
        assert_eq!(source.editor.text(), text);
        assert!(source.is_dirty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), disk);
        assert_eq!(
            source
                .document
                .active_timeline()
                .unwrap()
                .transcript("voice")
                .unwrap()
                .find_marker("cue"),
            Some(3.25)
        );
        assert_eq!(shell.preview_store.preview.playback.current_time_s(), 2.5);
    }
}
