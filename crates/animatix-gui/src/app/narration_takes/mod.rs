mod capture;
mod controller;
mod source;
mod view;
mod worker;

use std::path::PathBuf;
use std::sync::Arc;

use self::capture::{Capture, Recording};
use self::controller::Audition;
use self::source::Target;
use self::view::Inputs;
use self::worker::{Job, MarkerChange, SceneTakes};
use crate::app::document::version::SourceEpoch;

type SourceRevision = (PathBuf, SourceEpoch, (u64, u64));

#[derive(Default)]
pub(super) struct NarrationTakes {
    open: bool,
    scene: Option<String>,
    source_revision: Option<SourceRevision>,
    target: Option<Target>,
    selected: Option<String>,
    chosen: Option<PathBuf>,
    takes: Option<SceneTakes>,
    job: Option<Job>,
    discard_job: bool,
    refresh_pending: bool,
    feedback: Option<Feedback>,
    affected: Vec<MarkerChange>,
    updated_epoch: Option<SourceEpoch>,
    inputs: Inputs,
    audition: Option<Audition>,
    capture: Option<Capture>,
    recording: Option<Arc<Recording>>,
}

struct Feedback {
    text: String,
    error: bool,
}
