use animatix::composition::Composition;
use animatix::timeline::Timeline;
use animatix_syntax::narration::{NarrationParagraph, paragraphs};

use super::{ExportDialogState, ExportFormat, truncate_chars};
use crate::app::document::export_target::ExportTargetRef;
use crate::app::document::narration::{
    NarrationTrack, VoiceoverHistory, VoiceoverStatus, VoiceoverTiming, voiceover,
};
use crate::app::stores::SourceStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AffectedParagraph {
    pub title: String,
    pub effect: &'static str,
}

#[derive(Clone, Copy)]
enum Output {
    Still(f64),
    Motion {
        duration: f64,
        fps: f64,
        audio: bool,
    },
}

pub(super) fn affected_paragraphs(
    source: &SourceStore,
    history: &VoiceoverHistory,
    target: ExportTargetRef<'_>,
    settings: &ExportDialogState,
) -> Vec<AffectedParagraph> {
    let output = match settings.format {
        ExportFormat::Image | ExportFormat::WebP => Output::Still(f64::from(settings.time_s)),
        format => Output::Motion {
            duration: f64::from(settings.motion_duration_s(target.duration_s())),
            fps: f64::from(settings.fps),
            audio: format != ExportFormat::Gif,
        },
    };
    let mut affected = Vec::new();
    for (index, paragraph) in paragraphs(source.text()).into_iter().enumerate() {
        let scene = paragraph.scene.as_deref();
        let (timeline, composition, offset) = match target {
            ExportTargetRef::Timeline { timeline, .. } => {
                if !source
                    .document
                    .narration_timeline(scene)
                    .is_some_and(|selected| std::ptr::eq(timeline, selected))
                {
                    continue;
                }
                (timeline, None, 0.0)
            },
            ExportTargetRef::Composition { composition, .. } => {
                let Some((s, offset)) = scene.and_then(|name| {
                    Some((composition.scenes.get(name)?, *composition.scene_start_times.get(name)?))
                }) else {
                    continue;
                };
                (&s.timeline, Some(composition), offset)
            },
        };
        let track = NarrationTrack::find(timeline);
        let (status, timing) = voiceover(
            history,
            track.as_ref(),
            &paragraph,
            source.editor.narration_anchor_changes(),
        );
        if let Some(effect) = effect_for(
            output,
            timeline,
            composition,
            offset,
            &paragraph,
            status,
            track.as_ref().zip(timing.as_ref()),
        ) {
            let title = format!(
                "{} {} · {}",
                scene.unwrap_or("Paragraph"),
                index + 1,
                truncate_chars(&paragraph.text, 100)
            );
            affected.push(AffectedParagraph { title, effect });
        }
    }
    affected
}

fn effect_for(
    output: Output,
    timeline: &Timeline,
    composition: Option<&Composition>,
    offset: f64,
    paragraph: &NarrationParagraph,
    status: VoiceoverStatus,
    adopted: Option<(&NarrationTrack<'_>, &VoiceoverTiming)>,
) -> Option<&'static str> {
    use VoiceoverStatus::*;
    if status == Current {
        return None;
    }
    if matches!(status, Missing | Pending) {
        return match output {
            Output::Motion {
                duration,
                audio: true,
                ..
            } if offset < duration => Some(status.label()),
            _ => None,
        };
    }
    let (track, timing) = adopted?;
    let (start, end) = timing.interval;
    let local_caption = |time| {
        time >= start
            && time < end
            && timeline.has_narration_caption_at(track.id, timing.segments.as_ref(), time)
    };
    let caption = |global| {
        if let (Some(composition), Some(scene)) = (composition, paragraph.scene.as_deref()) {
            drawn_scene_time(composition, scene, global).is_some_and(local_caption)
        } else {
            local_caption(global)
        }
    };
    let (audio, captions) = match output {
        Output::Still(time) => (false, caption(time)),
        Output::Motion {
            duration,
            fps,
            audio,
        } => {
            let first = ((start + offset).max(0.0) * fps).ceil() as u32;
            let last = ((end + offset).min(duration) * fps).ceil() as u32;
            let captions = (first..last).any(|frame| caption(f64::from(frame) / fps));
            let audio_start = start.max(track.audio.start_time_s).max(-offset);
            let clip_end =
                track.audio.duration_s.map_or(f64::INFINITY, |d| track.audio.start_time_s + d);
            let audio_end = end.min(clip_end).min(duration - offset);
            let audible = audio
                && audio_start < audio_end
                && (track.audio.volume_at(audio_start) > 0.0
                    || track.audio.volume_at(audio_end) > 0.0
                    || track.audio.volume_envelope.as_ref().is_some_and(|points| {
                        points
                            .iter()
                            .any(|&(t, gain)| t > audio_start && t < audio_end && gain > 0.0)
                    }));
            (audible, captions)
        },
    };
    match (audio, captions) {
        (true, true) => Some("Includes previous audio and subtitles"),
        (true, false) => Some("Includes previous audio"),
        (false, true) => Some("Includes previous subtitles"),
        (false, false) => None,
    }
}

fn drawn_scene_time(composition: &Composition, scene: &str, time: f64) -> Option<f64> {
    let (name, local, transition) = composition.evaluate(time);
    if let Some(transition) = transition {
        if transition.to_scene == scene && transition.eased_progress > 0.0 {
            return Some(transition.to_local);
        }
        if transition.from_scene == scene && transition.eased_progress < 1.0 {
            return Some(transition.from_local);
        }
        None
    } else {
        (name == scene).then_some(local)
    }
}
