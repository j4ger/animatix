mod assets;
mod bindings;
mod capture;
mod controller;
mod history;
mod save_as;
mod session;
mod source;
mod status;
mod view;
mod worker;

use std::collections::HashMap;
use std::path::PathBuf;

use self::capture::Capture;
use self::controller::{Audition, Origin};
use self::history::TakeSummary;
use self::session::Session;
use self::source::Target;
use self::view::Inputs;
use self::worker::{Job, Request};
use crate::app::document::narration::VoiceoverHistory;
use crate::app::document::version::SourceEpoch;

pub(super) use self::save_as::SaveAsNarration;

type SourceRevision = (PathBuf, SourceEpoch, (u64, u64));

#[derive(Default)]
pub(super) struct NarrationTakes {
    open: bool,
    scene: Option<String>,
    source_revision: Option<SourceRevision>,
    history_revision: Option<SourceRevision>,
    history_media_revision: u64,
    pub(super) voiceover_history: VoiceoverHistory,
    verified_mixes: HashMap<assets::Pair, status::VerifiedMix>,
    target: Option<Target>,
    session: Option<Session>,
    selected: Option<String>,
    takes: Vec<TakeSummary>,
    active: Vec<String>,
    active_stamp: Option<assets::PairStamp>,
    job: Option<(Job, Origin)>,
    feedback: Option<crate::app::i18n::StatusText>,
    history_error: Option<String>,
    inputs: Inputs,
    audition: Option<Audition>,
    capture: Option<(Capture, Request, Origin)>,
}

#[cfg(test)]
pub(super) fn seed_history_for_save_as_test(source: &std::path::Path) -> PathBuf {
    let history = history::History::open(source, true).unwrap().unwrap();
    let take = history::Take {
        id: "test-take".into(),
        paragraph: "test-paragraph".into(),
        scene: None,
        text: "Hello.".into(),
        kind: "recorded",
    };
    history.start_recording(&take, 8_000, 1).unwrap();
    history.append_recording(&take.id, &[-1, 2], 1).unwrap();
    history.finish_recording(&take.id, true).unwrap();
    history::path_for(source)
}
