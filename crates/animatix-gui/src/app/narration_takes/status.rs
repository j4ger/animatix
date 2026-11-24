use std::collections::{HashMap, HashSet};

use super::NarrationTakes;
use super::assets::{self, PairStamp};
use super::history::path_for;
use super::source::Target;
use crate::app::document::narration::{AdoptedVoiceover, VoiceoverHistory};
use crate::app::stores::SourceStore;

pub(super) struct VerifiedMix {
    stamp: PairStamp,
    parts: Option<HashMap<String, String>>,
}

impl NarrationTakes {
    pub(in crate::app) fn update_voiceover_history(&mut self, source: &SourceStore) {
        let revision =
            (source.file_path().to_owned(), source.epoch(), source.editor.source_identity());
        let media_revision = source.document.audio_waveform_revision(0);
        let media_changed = self.history_media_revision != media_revision;
        if self.history_revision.as_ref() == Some(&revision) && !media_changed {
            return;
        }
        let changed_document = self
            .history_revision
            .as_ref()
            .is_some_and(|previous| previous.0 != revision.0 || previous.2.0 != revision.2.0);
        self.history_revision = Some(revision);
        self.history_media_revision = media_revision;
        if changed_document {
            self.close();
            self.session = None;
            self.source_revision = None;
            self.active_stamp = None;
            self.verified_mixes.clear();
            self.takes.clear();
        }
        if self.open {
            if media_changed {
                self.source_revision = None;
            }
            self.retarget(source, false);
        }
        let mut status = VoiceoverHistory {
            revision: self.voiceover_history.revision.wrapping_add(1),
            ..Default::default()
        };
        if self.session.is_some() || path_for(source.file_path()).is_file() {
            match self.sync_session(source) {
                Ok(()) => self.load_voiceover_history(source, &mut status),
                Err(error) => self.history_error = Some(error),
            }
        }
        self.voiceover_history = status;
    }

    fn load_voiceover_history(&mut self, source: &SourceStore, status: &mut VoiceoverHistory) {
        let Some(session) = &self.session else { return };
        let Some(history) = &session.history else {
            return;
        };
        let takes = match history.takes() {
            Ok(takes) => takes,
            Err(error) => {
                self.history_error = Some(error.to_string());
                return;
            },
        };
        let bindings: HashMap<_, _> =
            session.bindings.iter().map(|binding| (binding.id.as_str(), binding)).collect();
        for take in takes.iter().filter(|take| !take.aligned && take.status != "interrupted") {
            if let Some(binding) = bindings.get(take.paragraph.as_str())
                && take.scene == binding.scene
                && take.text == binding.text
            {
                status.pending.insert((binding.scene.clone(), binding.selector.clone()));
            }
        }

        let scenes: HashSet<_> = session.bindings.iter().map(|binding| &binding.scene).collect();
        let takes_by_id: HashMap<_, _> =
            takes.iter().map(|take| (take.id.as_str(), take)).collect();
        for scene in scenes {
            let Ok(target) = Target::open(source, scene.clone()) else {
                continue;
            };
            let Some(pair) = target.base else { continue };
            let Ok(stamp) = PairStamp::read(&pair) else {
                continue;
            };
            let (Ok(audio), Ok(timing)) =
                (pair.0.strip_prefix(&target.folder), pair.1.strip_prefix(&target.folder))
            else {
                continue;
            };
            let cached = self.verified_mixes.get(&pair);
            if cached.is_none_or(|mix| mix.stamp != stamp) {
                let parts = match history.mix(audio, timing) {
                    Ok(Some(mix))
                        if assets::fingerprint(&pair).is_ok_and(|hashes| {
                            hashes.0 == mix.audio_digest && hashes.1 == mix.transcript_digest
                        }) =>
                    {
                        Some(mix.parts)
                    },
                    Ok(_) => None,
                    Err(error) => {
                        self.history_error = Some(error.to_string());
                        None
                    },
                };
                self.verified_mixes.insert(pair.clone(), VerifiedMix { stamp, parts });
            }
            let Some(parts) = &self.verified_mixes[&pair].parts else {
                continue;
            };
            let active = status.mixes.entry(scene.clone()).or_default();
            for (selector, take) in parts {
                if let Some(take) = takes_by_id.get(take.as_str())
                    && let Some(binding) = bindings.get(take.paragraph.as_str())
                    && binding.scene == *scene
                {
                    active.insert(
                        binding.selector.clone(),
                        AdoptedVoiceover {
                            selector: selector.clone(),
                            text: take.text.clone(),
                        },
                    );
                }
            }
        }
        self.takes = takes;
    }
}
