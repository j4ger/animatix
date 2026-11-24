use std::ffi::OsString;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Duration;

use egui::Context;
use rodio::{Decoder, OutputStream, Sink};
use uuid::Uuid;

use super::NarrationTakes;
use super::assets::{self, Pair};
use super::capture::Capture;
use super::history::Take;
use super::session::Session;
use super::source::{Target, selected_paragraph, source_scene};
use super::view::Action;
use super::worker::{Finished, Job, ManifestParagraph, Operation, Request};
use crate::app::GuiShell;
use crate::app::commands::Command;
use crate::app::stores::SourceStore;

pub(super) struct Origin {
    path: PathBuf,
    editor: u64,
    take: Take,
    base: Option<Pair>,
    script: Vec<ManifestParagraph>,
}

impl NarrationTakes {
    pub(crate) fn open_paragraph(
        &mut self,
        source: &SourceStore,
        scene: Option<String>,
        selector: &str,
    ) {
        self.open_for(source, scene);
        if self.target.as_ref().is_some_and(|target| {
            target.paragraphs.iter().any(|paragraph| paragraph.id() == selector)
        }) {
            self.selected = Some(selector.to_owned());
        }
    }

    fn open_for(&mut self, source: &SourceStore, scene: Option<String>) {
        self.update_voiceover_history(source);
        self.open = true;
        self.scene = scene;
        self.source_revision = None;
        self.feedback = None;
        self.history_error = None;
        self.active_stamp = None;
        self.retarget(source, true);
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.audition = None;
        if let Some((capture, _, _)) = &mut self.capture {
            capture.stop();
        }
    }

    pub(super) fn sync_session(&mut self, source: &SourceStore) -> Result<(), String> {
        if self.session.as_ref().is_none_or(|session| !session.is_for(source.file_path())) {
            self.session = Some(Session::open(source)?);
        }
        if let Some(session) = &mut self.session {
            session.update(source)?;
        }
        Ok(())
    }

    pub(in crate::app) fn checkpoint(&mut self, source: &SourceStore, saved: bool) {
        if self.session.is_none() && !super::history::path_for(source.file_path()).is_file() {
            return;
        }
        let result = self.sync_session(source).and_then(|()| {
            self.session
                .as_mut()
                .map_or(Ok(()), |session| session.checkpoint(source, saved))
        });
        if let Err(error) = result {
            tracing::warn!(%error, "Could not save narration paragraph bindings");
            self.history_error = Some(error);
        }
    }

    pub(super) fn retarget(&mut self, source: &SourceStore, from_cursor: bool) {
        let revision =
            (source.file_path().to_owned(), source.epoch(), source.editor.source_identity());
        if self.source_revision.as_ref() == Some(&revision) {
            return;
        }
        self.source_revision = Some(revision);
        let old_binding = self.selected.as_deref().and_then(|selector| {
            self.session
                .as_ref()?
                .binding(self.scene.as_deref(), selector)
                .map(|binding| binding.id.clone())
        });
        if let Err(error) = self.sync_session(source) {
            self.session = None;
            self.history_error = Some(error);
        }
        self.feedback = None;
        let has_script = self.session.as_ref().is_none_or(|session| {
            session.bindings.iter().any(|binding| binding.scene == self.scene)
        });
        self.target = match has_script.then(|| Target::open(source, self.scene.clone())) {
            Some(Ok(target)) => Some(target),
            Some(Err(error)) => {
                self.feedback = Some(error);
                None
            },
            None => None,
        };
        self.selected = self.target.as_ref().and_then(|target| {
            from_cursor
                .then(|| selected_paragraph(source, target))
                .flatten()
                .or_else(|| {
                    let binding = old_binding.as_ref()?;
                    self.session
                        .as_ref()?
                        .bindings
                        .iter()
                        .find(|candidate| candidate.id == *binding && candidate.scene == self.scene)
                        .map(|binding| binding.selector.clone())
                })
                .or_else(|| target.paragraphs.first().map(|paragraph| paragraph.id()))
        });
        self.refresh();
    }

    fn refresh(&mut self) {
        self.takes.clear();
        let Some(history) = self.session.as_ref().and_then(|session| session.history.as_ref())
        else {
            self.active.clear();
            self.active_stamp = None;
            return;
        };
        match history.takes() {
            Ok(takes) => self.takes = takes,
            Err(error) => self.history_error = Some(error.to_string()),
        }
        let target = self.target.as_ref();
        let pair = target.and_then(|target| target.base.as_ref());
        let stamp = pair.and_then(|pair| assets::PairStamp::read(pair).ok());
        if stamp.is_some() && stamp == self.active_stamp {
            return;
        }
        self.active.clear();
        self.active_stamp = None;
        let Some((target, pair)) = target.zip(pair) else {
            return;
        };
        let (Ok(audio), Ok(timing)) =
            (pair.0.strip_prefix(&target.folder), pair.1.strip_prefix(&target.folder))
        else {
            return;
        };
        match history.mix(audio, timing) {
            Ok(Some(mix))
                if assets::fingerprint(pair).is_ok_and(|hashes| {
                    hashes == (mix.audio_digest.clone(), mix.transcript_digest.clone())
                }) =>
            {
                self.active.extend(mix.parts.into_values());
                self.active_stamp = stamp;
            },
            Err(error) => self.history_error = Some(error.to_string()),
            _ => {},
        }
    }

    fn request(
        &mut self,
        source: &SourceStore,
        operation: Operation,
        existing: Option<String>,
    ) -> Result<(Request, Origin), String> {
        self.sync_session(source)?;
        let target = self.target.as_ref().ok_or("Select a narration paragraph first.")?;
        let selector = self.selected.clone().ok_or("Select a narration paragraph first.")?;
        let session = self.session.as_mut().ok_or("Could not open voiceover history.")?;
        let history = session.history_for_write()?;
        let binding = session
            .binding(self.scene.as_deref(), &selector)
            .ok_or("The selected paragraph is no longer in the script.")?;
        let kind = match &operation {
            Operation::Synthesize { .. } => "synthesized",
            Operation::Import { audio: Some(_), .. } => "imported",
            _ => "recorded",
        };
        if existing.as_ref().is_some_and(|id| {
            !self.takes.iter().any(|take| {
                take.id == *id && take.paragraph == binding.id && take.text == binding.text
            })
        }) {
            return Err("This recording belongs to a different version of the script.".into());
        }
        let take = Take {
            id: existing.unwrap_or_else(|| Uuid::new_v4().simple().to_string()),
            paragraph: binding.id.clone(),
            scene: binding.scene.clone(),
            text: binding.text.clone(),
            kind,
        };
        let paragraphs = target.manifest();
        let origin = Origin {
            path: source.file_path().to_owned(),
            editor: source.editor.source_identity().0,
            take: take.clone(),
            base: target.base.clone(),
            script: paragraphs.clone(),
        };
        let python = self.inputs.python.trim();
        let python = if python.is_empty() {
            OsString::from(if cfg!(windows) { "python" } else { "python3" })
        } else {
            OsString::from(python)
        };
        self.feedback = None;
        self.history_error = None;
        Ok((
            Request {
                folder: target.folder.clone(),
                name: target.name.clone(),
                paragraphs,
                bindings: session
                    .bindings
                    .iter()
                    .filter(|binding| binding.scene == self.scene)
                    .cloned()
                    .collect(),
                base: target.base.clone(),
                python,
                operation,
                take,
                selector,
                history,
            },
            origin,
        ))
    }

    fn start_job(&mut self, ctx: &Context, request: Request, origin: Origin) -> Result<(), String> {
        let job = Job::start(request, ctx.clone())?;
        self.job = Some((job, origin));
        Ok(())
    }
}

impl GuiShell {
    pub(crate) fn open_narration_takes(&mut self, _ctx: &Context) {
        let source = &self.document_store.source;
        self.narration_takes.open_for(source, source_scene(source));
    }

    pub(crate) fn narration_takes_ui(&mut self, ctx: &Context) {
        let source = &self.document_store.source;
        let state = &mut self.narration_takes;
        if self.pending_open.is_some()
            || self.ui_store.view.welcome_open
            || state
                .session
                .as_ref()
                .is_some_and(|session| !session.is_for(source.file_path()))
        {
            state.close();
            state.session = None;
            state.source_revision = None;
            state.active_stamp = None;
        } else if state.open {
            state.retarget(source, false);
        }
        self.finish_narration_capture(ctx);
        self.finish_narration_job();

        let state = &mut self.narration_takes;
        if self.preview_store.preview.playback.is_playing
            || state.audition.as_ref().is_some_and(|audition| audition.sink.empty())
        {
            state.audition = None;
        }
        if state.audition.is_some() || state.capture.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(action) = state.show(ctx)
            && let Err(error) = self.narration_action(ctx, action)
        {
            self.narration_takes.feedback = Some(error);
        }
    }

    fn finish_narration_capture(&mut self, ctx: &Context) {
        let state = &mut self.narration_takes;
        let Some(result) = state.capture.as_ref().and_then(|(capture, _, _)| capture.poll()) else {
            return;
        };
        let (_, mut request, origin) = state.capture.take().unwrap();
        let was_current = origin.path == self.document_store.source.file_path();
        let history = request.history.clone();
        let id = request.take.id.clone();
        let outcome = result.and_then(|()| {
            request.operation = Operation::Import {
                audio: None,
                timing: state.inputs.import_timing(),
            };
            state.start_job(ctx, request, origin)
        });
        if let Err(error) = &outcome
            && let Err(write_error) = history.issue(&id, Some(error))
        {
            tracing::warn!(%write_error, "Could not save the recording issue");
        }
        if was_current {
            state.history_revision = None;
            state.refresh();
            if let Err(error) = outcome
                && !state.takes.iter().any(|take| take.id == id && take.issue.is_some())
            {
                state.feedback = Some(error);
            }
        }
    }

    fn finish_narration_job(&mut self) {
        let state = &mut self.narration_takes;
        let Some(result) = state.job.as_ref().and_then(|(job, _)| job.poll()) else {
            return;
        };
        let (_, origin) = state.job.take().unwrap();
        let source = &self.document_store.source;
        if origin.path != source.file_path()
            || origin.editor != source.editor.source_identity().0
            || self.pending_open.is_some()
        {
            return;
        }
        state.history_revision = None;
        state.refresh();
        match result {
            Ok(finished) => self.apply_narration(origin, finished),
            Err(error)
                if !state
                    .takes
                    .iter()
                    .any(|take| take.id == origin.take.id && take.issue.is_some()) =>
            {
                state.feedback = Some(error);
            },
            Err(_) => {},
        }
    }

    fn apply_narration(&mut self, origin: Origin, finished: Finished) {
        let source = &self.document_store.source;
        let state = &mut self.narration_takes;
        let Ok(target) = Target::open(source, origin.take.scene.clone()) else {
            return;
        };
        let still_matches = state.session.as_ref().is_some_and(|session| {
            session.bindings.iter().any(|binding| {
                binding.id == origin.take.paragraph
                    && binding.scene == origin.take.scene
                    && binding.text == origin.take.text
            })
        });
        let hash_matches = || {
            target
                .base
                .as_ref()
                .map(assets::fingerprint)
                .transpose()
                .is_ok_and(|hashes| hashes == finished.base_hashes)
        };
        if !still_matches
            || target.manifest() != origin.script
            || target.base != origin.base
            || !hash_matches()
        {
            return;
        }
        match target.patch(source, &finished.adopted) {
            Ok(patch) => {
                let before = source.epoch();
                let effects = self.handle_command(Command::ApplySourcePatch(patch));
                self.apply_effects(effects);
                let after = self.document_store.source.epoch();
                if before != after {
                    self.preview_store.quiet_rebuild = Some(after);
                }
            },
            Err(error) => self.narration_takes.feedback = Some(error),
        }
    }

    fn narration_action(&mut self, ctx: &Context, action: Action) -> Result<(), String> {
        let state = &mut self.narration_takes;
        let source = &self.document_store.source;
        match action {
            Action::Record => {
                let operation = Operation::Import {
                    audio: None,
                    timing: state.inputs.import_timing(),
                };
                let (request, origin) = state.request(source, operation, None)?;
                let capture =
                    Capture::start(ctx.clone(), request.history.clone(), request.take.clone())?;
                state.capture = Some((capture, request, origin));
                self.pause_for_narration();
            },
            Action::StopRecording => {
                if let Some((capture, _, _)) = &mut state.capture {
                    capture.stop();
                }
            },
            Action::Start(operation, existing) => {
                let (request, origin) = state.request(source, operation, existing)?;
                state.start_job(ctx, request, origin)?;
            },
            Action::Listen(id) => {
                let audition = {
                    let history = state
                        .session
                        .as_ref()
                        .and_then(|session| session.history.as_ref())
                        .ok_or("Could not open voiceover history.")?;
                    let wave = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
                    history.raw_wave(&id, wave.path()).map_err(|error| error.to_string())?;
                    Audition::start(wave)?
                };
                self.pause_for_narration();
                self.narration_takes.audition = Some(audition);
            },
            Action::StopListening => state.audition = None,
            Action::Select(selector) => {
                state.selected = Some(selector);
                state.feedback = None;
                state.audition = None;
            },
        }
        Ok(())
    }

    fn pause_for_narration(&mut self) {
        self.preview_store.preview.playback.is_playing = false;
        self.preview_store.preview_dirty = true;
        self.narration_takes.audition = None;
    }
}

pub(super) struct Audition {
    sink: Sink,
    _stream: OutputStream,
    _wave: tempfile::NamedTempFile,
}

impl Audition {
    fn start(wave: tempfile::NamedTempFile) -> Result<Self, String> {
        let file = File::open(wave.path())
            .map_err(|error| format!("Could not open this recording: {error}"))?;
        let decoded = Decoder::new(BufReader::new(file))
            .map_err(|error| format!("Could not decode this recording: {error}"))?;
        let (stream, handle) = OutputStream::try_default()
            .map_err(|error| format!("Could not open audio output: {error}"))?;
        let sink = Sink::try_new(&handle)
            .map_err(|error| format!("Could not play this recording: {error}"))?;
        sink.append(decoded);
        Ok(Self {
            sink,
            _stream: stream,
            _wave: wave,
        })
    }
}

#[cfg(test)]
mod tests;
