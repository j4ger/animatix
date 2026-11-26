//! Chapter boundaries on the full composition's video frame grid.

use animatix::composition::Composition;
use animatix::time::frame_at_or_after;

use super::ExportError;

/// A half-open interval of frames on the full video's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoFrameRange {
    start_frame: u32,
    end_frame: u32,
}

impl VideoFrameRange {
    pub(super) fn new(start_frame: u32, end_frame: u32) -> Self {
        Self {
            start_frame,
            end_frame,
        }
    }

    /// First rendered frame, inclusive.
    pub fn start_frame(self) -> u32 {
        self.start_frame
    }

    /// First frame after the interval.
    pub fn end_frame(self) -> u32 {
        self.end_frame
    }

    /// Number of sampled frames in the interval.
    pub fn len(self) -> u32 {
        self.end_frame - self.start_frame
    }

    /// Whether the interval contains no sampled frames.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

/// The existing full-video frame count, shared by chapter planning and encoding.
pub(super) fn video_frame_count(duration: f64, fps: u32) -> Result<u32, ExportError> {
    if !duration.is_finite() || duration <= 0.0 || fps == 0 {
        return Err(ExportError::VideoEncode("Duration and FPS must be positive".into()));
    }
    Ok(frame_at_or_after(duration, f64::from(fps)))
}

/// Partition all frames of a full composition export in playback order.
///
/// A chapter starts with the first sample at or after the scene's scheduled
/// start. The incoming transition belongs to that chapter. `duration` is the
/// full export duration, including any hold. A truncated or shorter-than-one-
/// frame chapter may have an empty interval.
pub fn video_chapter_frame_ranges(
    composition: &Composition,
    fps: u32,
    duration: f64,
) -> Result<Vec<(&str, VideoFrameRange)>, ExportError> {
    let total_frames = video_frame_count(duration, fps)?;
    let mut starts = composition
        .playback_order()
        .iter()
        .filter_map(|name| {
            let start = *composition.scene_start_times.get(name)?;
            Some((name.as_str(), frame_at_or_after(start, f64::from(fps)).min(total_frames)))
        })
        .peekable();
    let mut chapters = Vec::with_capacity(composition.playback_order().len());
    while let Some((name, start_frame)) = starts.next() {
        let end_frame = starts.peek().map_or(total_frames, |(_, start)| *start);
        if end_frame < start_frame {
            return Err(ExportError::VideoEncode(format!(
                "Scene '{name}' has a decreasing chapter boundary. Check its configured duration."
            )));
        }
        chapters.push((name, VideoFrameRange::new(start_frame, end_frame)));
    }
    Ok(chapters)
}

#[cfg(test)]
mod tests {
    use animatix::composition::BuildTarget;
    use animatix::timeline::BuildOptions;

    use super::*;

    fn composition(source: &str) -> Composition {
        let (statements, _) = animatix_syntax::parser::parse_source(source);
        let report = BuildTarget::build_with_options(
            &statements.unwrap(),
            &Default::default(),
            None,
            BuildOptions::default(),
        );
        assert!(!report.diagnostics.iter().any(|diagnostic| diagnostic.is_error()));
        let BuildTarget::MultiScene(composition) = report.output else {
            panic!("expected a composition");
        };
        composition
    }

    #[test]
    fn chapters_partition_global_frames_and_put_blends_in_the_incoming_chapter() {
        let composition = composition(concat!(
            "# First\nconfig { duration: 0.85 }\nplay Middle [fade, 200ms]\n",
            "# Last\nconfig { duration: 0.8 }\n",
            "# Middle\nconfig { duration: 0.8 }\nplay Last [fade, 200ms]\n",
        ));
        let chapters = video_chapter_frame_ranges(&composition, 10, 2.5).unwrap();
        assert_eq!(
            chapters
                .iter()
                .map(|(name, range)| (*name, range.start_frame, range.end_frame))
                .collect::<Vec<_>>(),
            [("First", 0, 7), ("Middle", 7, 13), ("Last", 13, 25)]
        );
        let transition = composition.evaluate(7.0 / 10.0).2.expect("incoming blend");
        assert_eq!(
            (transition.from_scene.as_str(), transition.to_scene.as_str()),
            ("First", "Middle")
        );
        let transition = composition.evaluate(13.0 / 10.0).2.expect("incoming blend");
        assert_eq!(
            (transition.from_scene.as_str(), transition.to_scene.as_str()),
            ("Middle", "Last")
        );
    }

    #[test]
    fn chapters_keep_tied_scene_starts_in_playback_order_and_respect_the_full_cutoff() {
        let composition = composition(concat!(
            "# Zebra\nconfig { duration: 0.2 }\nplay Alpha [fade, 200ms]\n",
            "# Alpha\nconfig { duration: 0.5 }\n",
            "# Later\nconfig { duration: 0.8 }\n",
        ));
        let chapters = video_chapter_frame_ranges(&composition, 10, 0.4).unwrap();
        assert_eq!(
            chapters
                .iter()
                .map(|(name, range)| (*name, range.start_frame, range.end_frame))
                .collect::<Vec<_>>(),
            [("Zebra", 0, 0), ("Alpha", 0, 4), ("Later", 4, 4)]
        );
        assert_eq!(frame_at_or_after(0.28, 25.0), 7);
        assert_eq!(frame_at_or_after(0.281, 25.0), 8);
    }

    #[test]
    fn concatenating_chapter_frames_matches_the_full_render_with_transitions() {
        use animatix::timeline::DebugRenderOptions;

        use crate::offscreen::OffscreenRenderer;
        use crate::render_pipeline::render_frames_streaming_composition_range;

        if OffscreenRenderer::new().is_err() {
            crate::testing::skip_if_no_gpu();
            return;
        }
        let composition = composition(concat!(
            "# First\nconfig { duration: 0.65 }\nplay Second [fade, 250ms]\n",
            "#0s\na: Rect, size: (128, 128), at: (32, 32), color: (1, 0, 0, 1)\n",
            "# Second\nconfig { duration: 0.55 }\nplay Third [wipe_left, 200ms]\n",
            "#0s\nb: Rect, size: (128, 128), at: (32, 32), color: (0, 1, 0, 1)\n",
            "# Third\nconfig { duration: 0.5 }\n",
            "#0s\nc: Rect, size: (128, 128), at: (32, 32), color: (0, 0, 1, 1)\n",
        ));
        let render = |range: VideoFrameRange| {
            let mut frames = Vec::new();
            render_frames_streaming_composition_range(
                &composition,
                64,
                64,
                10,
                range.start_frame,
                range.len(),
                1,
                DebugRenderOptions::default(),
                None,
                None,
                |index, frame| {
                    assert_eq!(index, frames.len());
                    frames.push(frame.rgba);
                    Ok(())
                },
            )
            .unwrap();
            frames
        };
        let chapters = video_chapter_frame_ranges(&composition, 10, 1.5).unwrap();
        let full = render(VideoFrameRange {
            start_frame: 0,
            end_frame: 15,
        });
        let chapter_frames =
            chapters.into_iter().flat_map(|(_, range)| render(range)).collect::<Vec<_>>();
        assert_ne!(full[0], full[5]); // fade in progress
        assert_ne!(full[5], full[7]); // fade complete
        assert_ne!(full[7], full[8]); // wipe in progress
        assert_eq!(chapter_frames, full);
    }
}
