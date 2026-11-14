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
            composition
                .scenes
                .iter()
                .filter_map(|(name, scene)| {
                    composition.scene_start_times.get(name).map(|offset| (scene, *offset))
                })
                .flat_map(|(scene, offset)| {
                    scene
                        .timeline
                        .audio_segments()
                        .iter()
                        .cloned()
                        .map(move |segment| offset_segment(segment, offset))
                })
                .collect()
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

fn offset_segment(
    mut segment: animatix::timeline::AudioSegment,
    offset: f64,
) -> animatix::timeline::AudioSegment {
    segment.start_time_s += offset;
    if let Some(envelope) = &mut segment.volume_envelope {
        for (time, _) in envelope {
            *time += offset;
        }
    }
    segment
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
        let segment = offset_segment(
            animatix::timeline::AudioSegment {
                source: "voice.wav".into(),
                start_time_s: 1.0,
                duration_s: Some(4.0),
                volume: 0.8,
                volume_envelope: Some(vec![(1.0, 0.0), (3.0, 1.0)]),
            },
            5.0,
        );
        assert_eq!(segment.start_time_s, 6.0);
        assert_eq!(segment.duration_s, Some(4.0));
        assert_eq!(segment.volume_at(7.0), 0.5);
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
