//! Audio-segment collection with document-relative path resolution.

use super::DocumentSession;

impl DocumentSession {
    /// Collect all audio segments from the timeline or composition, resolving
    /// relative source paths relative to the document's directory.
    pub fn all_audio_segments(&self) -> Vec<animatix::timeline::AudioSegment> {
        use std::path::Path;

        use animatix::timeline::AudioSegment;

        let doc_dir = self.file_path.parent().unwrap_or(Path::new(""));

        let segments: Vec<AudioSegment> = if let Some(composition) = &self.composition {
            composition
                .scenes
                .values()
                .flat_map(|s| s.timeline.audio_segments().to_vec())
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
                    if let Ok(resolved) = doc_dir.join(path).canonicalize() {
                        seg.source = resolved.to_string_lossy().to_string();
                    }
                }
                seg
            })
            .collect()
    }
}
