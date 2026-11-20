//! Audio-segment collection with document-relative path resolution.

use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use super::DocumentSession;

pub(super) fn fresh_waveform_identity() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl DocumentSession {
    /// Distinguish document switches even when their rebuild counters coincide.
    pub(crate) fn audio_waveform_revision(&self, generation: u64) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.waveform_identity.hash(&mut hash);
        self.file_path.hash(&mut hash);
        generation.hash(&mut hash);
        hash.finish()
    }

    /// Collect all audio segments from the timeline or composition, resolving
    /// relative source paths relative to the document's directory.
    pub fn all_audio_segments(&self) -> Vec<animatix::timeline::AudioSegment> {
        use std::path::Path;

        use animatix::timeline::AudioSegment;

        let doc_dir = self.file_path.parent().unwrap_or(Path::new(""));

        let segments: Vec<AudioSegment> = if let Some(composition) = &self.composition {
            composition.audio_segments()
        } else if let Some(timeline) = &self.timeline {
            timeline.audio_segments().to_vec()
        } else {
            Vec::new()
        };

        // Resolve relative source paths relative to the document directory
        segments
            .into_iter()
            .map(|mut seg| {
                let path = Path::new(&seg.source);
                if path.is_relative() {
                    let resolved = doc_dir.join(path);
                    seg.source =
                        resolved.canonicalize().unwrap_or(resolved).to_string_lossy().to_string();
                }
                seg
            })
            .collect()
    }
}

impl DocumentSession {
    pub(crate) fn narration_asset_paths(&self) -> std::collections::BTreeSet<std::path::PathBuf> {
        let mut paths: std::collections::BTreeSet<_> = self
            .all_audio_segments()
            .into_iter()
            .map(|segment| std::path::PathBuf::from(segment.source))
            .collect();
        let timelines = self.timeline.iter().chain(
            self.composition
                .iter()
                .flat_map(|composition| composition.scenes.values().map(|scene| &scene.timeline)),
        );
        for timeline in timelines {
            paths.extend(timeline.asset_cache_arc().transcripts().map(|(name, _)| name.into()));
        }
        paths
            .retain(|path| !path.as_os_str().is_empty() && !path.to_string_lossy().contains("://"));
        paths
    }

    pub(crate) fn invalidate_media_identity(&mut self) {
        self.waveform_identity = fresh_waveform_identity();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_audio_uses_global_time() {
        let mut document = DocumentSession::from_source(
            "audio.amx".into(),
            "# First\nconfig { duration: 2 }\n#0s\na: Audio, source: \"a.wav\"\n# Second\nconfig { duration: 3 }\n#1s\nb: Audio, source: \"b.wav\"".into(),
        ).unwrap();
        document.rebuild().unwrap();
        let composition = document.composition.as_ref().unwrap();
        let second_start = composition.scene_start_times["Second"];
        let local_start = composition.scenes["Second"].timeline.audio_segments()[0].start_time_s;
        let segments = document.all_audio_segments();
        let second = segments.iter().find(|segment| segment.source.ends_with("b.wav")).unwrap();
        assert_eq!(second.start_time_s, second_start + local_start);
        assert!(second_start > 0.0);
    }

    #[test]
    fn volume_automation_moves_with_scene_without_changing_duration() {
        let mut document = DocumentSession::from_source(
            "audio.amx".into(),
            "# First
config { duration: 2 }
# Second
config { duration: 3 }
#0s
a: Audio, source: \"a.wav\", volume: 0.25 [2s]
#1s
a.volume = 0.75"
                .into(),
        )
        .unwrap();
        document.rebuild().unwrap();
        let composition = document.composition.as_ref().unwrap();
        let offset = composition.scene_start_times["Second"];
        let local = &composition.scenes["Second"].timeline.audio_segments()[0];
        let segments = document.all_audio_segments();
        let segment = &segments[0];
        assert_eq!(segment.start_time_s, offset + local.start_time_s);
        assert_eq!(segment.duration_s, local.duration_s);
        assert_eq!(segment.volume_at(offset + 0.2), 0.25);
        assert_eq!(segment.volume_at(offset + 1.1), 0.75);
        let expected: Vec<_> = local
            .volume_envelope
            .as_ref()
            .unwrap()
            .iter()
            .map(|(time, volume)| (time + offset, *volume))
            .collect();
        assert_eq!(segment.volume_envelope.as_ref().unwrap(), &expected);
    }

    #[test]
    fn delayed_audio_preserves_authored_start_and_duration() {
        let mut document = DocumentSession::from_source(
            "audio.amx".into(),
            "#0s\nvoice: Audio, source: \"voice.wav\" [delay: 2s, 3s]".into(),
        )
        .unwrap();
        document.rebuild().unwrap();
        let segments = document.all_audio_segments();
        assert_eq!(segments[0].start_time_s, 2.0);
        assert_eq!(segments[0].duration_s, Some(3.0));
    }

    #[test]
    fn opening_a_relative_document_resolves_audio_and_transcript_once() {
        let file = std::path::Path::new("../../examples/animation/41_speech_sync_captions.amx");
        let doc = DocumentSession::load(file.to_owned()).unwrap();
        let paths = doc.narration_asset_paths();
        assert!(paths.iter().any(|path| path.ends_with("narration.mp3") && path.is_file()));
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with("narration.speech.json") && path.is_file())
        );
    }

    #[test]
    fn missing_asset_paths_are_still_relative_to_document() {
        let mut document = DocumentSession::from_source(
            "/tmp/animatix-audio-fixture/scene.amx".into(),
            "#0s\na: Audio, source: \"missing.wav\"".into(),
        )
        .unwrap();
        document.rebuild().unwrap();
        assert_eq!(
            document.all_audio_segments()[0].source,
            "/tmp/animatix-audio-fixture/missing.wav"
        );
    }

    #[test]
    fn waveform_generation_changes_when_document_changes() {
        let mut document = DocumentSession::from_error("one.amx".into());
        let initial = document.audio_waveform_revision(1);
        let reopened = DocumentSession::from_error("one.amx".into());
        assert_ne!(initial, reopened.audio_waveform_revision(1));
        assert_ne!(initial, document.audio_waveform_revision(2));
        document.file_path = "two.amx".into();
        assert_ne!(initial, document.audio_waveform_revision(1));
    }
}
