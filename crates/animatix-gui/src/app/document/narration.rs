use animatix::narration::DraftCueTime;
use animatix::timeline::Timeline;
use animatix_syntax::ast::{Stmt, Time};
use animatix_syntax::diagnostics::DiagnosticCode;
use animatix_syntax::narration::{TRACK, decode_anchor};

use super::DocumentSession;

impl DocumentSession {
    pub(crate) fn narration_transcript(
        &self,
        scene: Option<&str>,
    ) -> Option<&animatix::narration::SpeechTranscript> {
        let timeline = match scene {
            Some(name) => &self.composition.as_ref()?.scenes.get(name)?.timeline,
            None => self.timeline.as_ref()?,
        };
        [TRACK, "voice"]
            .into_iter()
            .find_map(|label| {
                timeline.audio_segment_for_actor(timeline.get_entity_id(label)?)?;
                timeline.transcript(label)
            })
            .or_else(|| {
                let mut audio = timeline.transcripts_iter().filter_map(|(actor, transcript)| {
                    timeline.audio_segment_for_actor(actor).map(|_| transcript)
                });
                let transcript = audio.next()?;
                audio.next().is_none().then_some(transcript)
            })
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
