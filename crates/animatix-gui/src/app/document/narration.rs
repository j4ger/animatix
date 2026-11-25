use std::collections::{HashMap, HashSet};
use std::ops::Range;

use animatix::narration::{DraftCueTime, SpeechTranscript, resolve_anchor};
use animatix::timeline::Timeline;
use animatix::timeline::{AudioSegment, EntityId};
use animatix_syntax::ast::{Stmt, Time};
use animatix_syntax::diagnostics::DiagnosticCode;
use animatix_syntax::narration::{AnchorEdge, NarrationAnchor, NarrationParagraph, encode_anchor};
use animatix_syntax::narration::{TRACK, decode_anchor};

use super::DocumentSession;

impl DocumentSession {
    pub(crate) fn narration_timeline(&self, scene: Option<&str>) -> Option<&Timeline> {
        match scene {
            Some(name) => Some(&self.composition.as_ref()?.scenes.get(name)?.timeline),
            None => self.timeline.as_ref(),
        }
    }

    pub(crate) fn resolved_narration_cues(&self) -> Vec<DraftCueTime> {
        let mut cues = Vec::new();
        if let Some(statements) = self.raw_statements.as_deref() {
            self.collect_cues(statements, None, self.timeline.as_ref(), &mut cues);
        }
        cues
    }

    fn collect_cues(
        &self,
        statements: &[Stmt],
        scene: Option<&str>,
        timeline: Option<&Timeline>,
        cues: &mut Vec<DraftCueTime>,
    ) {
        for statement in statements {
            match statement {
                Stmt::Scene { name, body, .. } => {
                    let timeline = self
                        .composition
                        .as_ref()
                        .and_then(|composition| composition.scenes.get(name))
                        .map(|scene| &scene.timeline);
                    self.collect_cues(body, Some(name), timeline, cues);
                },
                Stmt::Keyframe {
                    time:
                        Time::SpeechMarker {
                            track,
                            marker,
                            offset_ms,
                        },
                    span: Some(span),
                    ..
                } => {
                    let Some(key) = decode_anchor(marker) else {
                        continue;
                    };
                    let Some(time) = timeline.and_then(|timeline| timeline.marker_time(*span))
                    else {
                        continue;
                    };
                    // A draft with no known timing may have built this keyframe at zero.
                    // Reuse zero only when that was a resolved timestamp.
                    if time == 0.0
                        && self.diagnostics.iter().any(|diagnostic| {
                            diagnostic.location.line == Some(span.start_line)
                                && matches!(
                                    diagnostic.code,
                                    DiagnosticCode::NarrationBindingUnresolved
                                        | DiagnosticCode::InvalidConfigValue
                                )
                        })
                    {
                        continue;
                    }
                    cues.push(DraftCueTime {
                        scene: scene.map(str::to_owned),
                        track: track.clone(),
                        key,
                        offset_ms: *offset_ms,
                        time_ms: time.round() as u64,
                        previous_keys: Vec::new(),
                    });
                },
                _ => {},
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct VoiceoverHistory {
    pub revision: u64,
    pub pending: HashSet<(Option<String>, String)>,
    // A scene with a verified managed mix owns its paragraph identity, even
    // when a paragraph is absent from that mix or its text duplicates another.
    pub mixes: HashMap<Option<String>, HashMap<String, AdoptedVoiceover>>,
}

pub(crate) struct AdoptedVoiceover {
    pub selector: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoiceoverStatus {
    Missing,
    Pending,
    Previous,
    Current,
}

impl VoiceoverStatus {
    pub(crate) fn message_id(self) -> &'static str {
        match self {
            Self::Missing => "gui-narration-voiceover-missing",
            Self::Pending => "gui-narration-awaiting-alignment",
            Self::Previous => "gui-narration-voiceover-previous",
            Self::Current => "gui-narration-voiceover-current",
        }
    }
}

pub(crate) struct VoiceoverTiming {
    pub interval: (f64, f64),
    pub segments: Option<Range<usize>>,
}

pub(crate) struct NarrationTrack<'a> {
    pub id: EntityId,
    pub transcript: &'a SpeechTranscript,
    pub audio: &'a AudioSegment,
}

impl<'a> NarrationTrack<'a> {
    pub fn find(timeline: &'a Timeline) -> Option<Self> {
        let track = |id, transcript| {
            Some(Self {
                id,
                transcript,
                audio: timeline.audio_segment_for_actor(id)?,
            })
        };
        [TRACK, "voice"]
            .into_iter()
            .find_map(|label| track(timeline.get_entity_id(label)?, timeline.transcript(label)?))
            .or_else(|| {
                let mut tracks = timeline.transcripts_iter().filter_map(|(id, tx)| track(id, tx));
                let first = tracks.next()?;
                tracks.next().is_none().then_some(first)
            })
    }

    fn managed_timing(&self, selector: &str) -> Option<VoiceoverTiming> {
        let range = self.transcript.narration_takes.as_ref()?.get(selector)?.clone();
        let segments = self.transcript.segments.get(range.clone())?;
        let start = segments.first()?.start_s;
        let end = segments.iter().map(|s| s.end_s).max_by(f64::total_cmp)?;
        Some(VoiceoverTiming {
            interval: (self.audio.start_time_s + start, self.audio.start_time_s + end),
            segments: Some(range),
        })
    }

    fn anchor_timing(&self, anchor: &NarrationAnchor) -> Option<VoiceoverTiming> {
        let mut anchor = anchor.clone();
        let key = decode_anchor(&encode_anchor(&anchor)?)?;
        let start = resolve_anchor(self.transcript, &key)?;
        anchor.edge = AnchorEdge::End;
        let end = resolve_anchor(self.transcript, &decode_anchor(&encode_anchor(&anchor)?)?)?;
        Some(VoiceoverTiming {
            interval: (self.audio.start_time_s + start, self.audio.start_time_s + end),
            segments: self
                .transcript
                .narration_takes
                .as_ref()
                .and_then(|takes| takes.get(&key.paragraph_id()).cloned()),
        })
    }
}

pub(crate) fn voiceover(
    history: &VoiceoverHistory,
    track: Option<&NarrationTrack<'_>>,
    paragraph: &NarrationParagraph,
    changes: &[Vec<crate::editor::narration::AnchorChange>],
) -> (VoiceoverStatus, Option<VoiceoverTiming>) {
    use VoiceoverStatus::*;
    let selector = paragraph.id();
    let missing = || {
        if history.pending.contains(&(paragraph.scene.clone(), selector.clone())) {
            Pending
        } else {
            Missing
        }
    };

    let Some(track) = track else {
        return (missing(), None);
    };
    if let Some(mix) = history.mixes.get(&paragraph.scene) {
        if let Some(take) = mix.get(&selector)
            && let Some(timing) = track.managed_timing(&take.selector)
        {
            let status = if take.text == paragraph.text {
                Current
            } else {
                Previous
            };
            return (status, Some(timing));
        }
        return (missing(), None);
    }

    let mut anchor = NarrationAnchor {
        paragraph: paragraph.text.clone(),
        paragraph_occurrence: paragraph.occurrence,
        phrase: None,
        phrase_occurrence: 1,
        edge: AnchorEdge::Start,
    };
    if let Some(timing) = track.anchor_timing(&anchor) {
        return (Current, Some(timing));
    }
    let Some(mut key) = encode_anchor(&anchor).and_then(|marker| decode_anchor(&marker)) else {
        return (missing(), None);
    };
    for edits in changes.iter().rev() {
        if let Some(change) = edits
            .iter()
            .find(|change| change.scene == paragraph.scene && change.after == key)
        {
            key = change.before.clone();
            // A managed transcript is keyed by the selector from adoption.
            if let Some(timing) = track.managed_timing(&key.paragraph_id()) {
                return (Previous, Some(timing));
            }
            anchor.paragraph = change.before_text.clone();
            anchor.paragraph_occurrence = key.paragraph_occurrence();
            if let Some(timing) = track.anchor_timing(&anchor) {
                return (Previous, Some(timing));
            }
        }
    }
    (missing(), None)
}
