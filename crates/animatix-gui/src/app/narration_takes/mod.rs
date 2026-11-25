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
