use std::ffi::OsString;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use egui::Context;
use rodio::{Decoder, OutputStream, Sink};

use super::capture::Capture;
use super::source::{Target, selected_paragraph, source_scene};
use super::view::Action;
use super::worker::{Adopted, Job, Operation, Output, Request};
use super::{Feedback, NarrationTakes};
use crate::app::GuiShell;
use crate::app::commands::Command;
use crate::app::stores::SourceStore;

impl NarrationTakes {
    fn open(&mut self, source: &SourceStore, ctx: &Context) {
        self.open = true;
        self.scene = source_scene(source);
        self.feedback = None;
        self.affected.clear();
        self.inputs.recording.clear();
        self.inputs.timing.clear();
        self.retarget(source, true);
        self.try_refresh(ctx);
    }

    pub(in crate::app) fn open_paragraph(
        &mut self,
        source: &SourceStore,
        scene: Option<String>,
        id: &str,
    ) {
        self.close();
        self.open = true;
        self.scene = scene;
        self.feedback = None;
        self.affected.clear();
        self.inputs.recording.clear();
        self.inputs.timing.clear();
        self.retarget(source, false);
        if self.target.as_ref().is_some_and(|t| t.paragraphs.iter().any(|p| p.id() == id)) {
            self.selected = Some(id.to_owned());
        }
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.audition = None;
        self.clear_recording();
        self.refresh_pending = false;
        self.cancel_for_target_change();
        self.target = None;
        self.source_revision = None;
        self.takes = None;
        self.chosen = None;
    }

    fn cancel_for_target_change(&mut self) {
        if let Some(job) = &self.job {
            job.cancel();
            self.discard_job = true;
        }
    }

    fn retarget(&mut self, source: &SourceStore, from_cursor: bool) {
        let epoch = source.epoch();
        let identity = source.editor.source_identity();
        if let Some((path, old_epoch, old_identity)) = &self.source_revision {
            if path != source.file_path() {
                self.close();
                return;
            }
            if !from_cursor && *old_epoch == epoch && *old_identity == identity {
                return;
            }
        }
        self.source_revision = Some((source.file_path().to_owned(), epoch, identity));
        let previous = self.target.as_ref().and_then(|target| self.selected_index(target));
        self.cancel_for_target_change();
        self.audition = None;
        self.clear_recording();
        self.takes = None;
        self.chosen = None;
        match Target::open(source, self.scene.clone()) {
            Ok(target) => {
                self.selected = from_cursor
                    .then(|| selected_paragraph(source, &target))
                    .flatten()
                    .or_else(|| {
                        self.selected.take().filter(|selected| {
                            target.paragraphs.iter().any(|paragraph| paragraph.id() == *selected)
                        })
                    })
                    .or_else(|| target.paragraphs.get(previous.unwrap_or(0)).map(|item| item.id()))
                    .or_else(|| target.paragraphs.first().map(|item| item.id()));
                self.target = Some(target);
                self.refresh_pending = true;
            },
            Err(error) => {
                self.target = None;
                self.refresh_pending = false;
                self.error(error);
            },
        }
    }

    pub(super) fn selected_index(&self, target: &Target) -> Option<usize> {
        let selected = self.selected.as_deref()?;
        target.paragraphs.iter().position(|paragraph| paragraph.id() == selected)
    }

    fn try_refresh(&mut self, ctx: &Context) {
        if self.refresh_pending && self.job.is_none() {
            self.refresh_pending = false;
            self.start(Operation::Inspect, ctx);
        }
    }

    fn start(&mut self, operation: Operation, ctx: &Context) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        if self.job.is_some() {
            return;
        }
        self.discard_job = false;
        let python = self.inputs.python.trim();
        let python = if python.is_empty() {
            OsString::from(if cfg!(windows) { "python" } else { "python3" })
        } else {
            OsString::from(python)
        };
        let request = Request {
            folder: target.folder.clone(),
            output_prefix: target.prefix.clone(),
            paragraphs: target.manifest(),
            base: target.base.clone(),
            anchors: if matches!(operation, Operation::Adopt { .. }) {
                target.anchors()
            } else {
                Vec::new()
            },
            python,
            operation,
        };
        match Job::start(request, ctx.clone()) {
            Ok(job) => self.job = Some(job),
            Err(error) => self.error(error),
        }
    }

    fn collect(&mut self) -> Option<Adopted> {
        let result = self.job.as_ref()?.poll()?;
        let cancelled = self.job.take().is_some_and(|job| job.cancelling());
        if std::mem::take(&mut self.discard_job) {
            return None;
        }
        if cancelled {
            self.info("Narration job cancelled.");
            return None;
        }
        match result {
            Ok(Output::Scene(takes, latest)) => {
                if let Some(candidate) = latest {
                    self.chosen = Some(candidate);
                    self.info("Take ready to audition.");
                }
                self.takes = Some(takes);
                None
            },
            Ok(Output::Adopted(result)) => Some(result),
            Err(error) => {
                self.error(error);
                None
            },
        }
    }

    pub(super) fn set_paragraph(&mut self, paragraph: String) {
        self.selected = Some(paragraph);
        self.chosen = None;
        self.audition = None;
        self.clear_recording();
        self.inputs.recording.clear();
        self.inputs.timing.clear();
    }

    fn clear_recording(&mut self) {
        let capturing = self.capture.take().is_some();
        let recorded = self.recording.take().is_some();
        if capturing || recorded {
            self.inputs.timing.clear();
        }
    }

    fn collect_recording(&mut self) {
        let Some(result) = self.capture.as_ref().and_then(Capture::poll) else {
            return;
        };
        self.capture = None;
        match result {
            Ok(recording) => {
                let status = if recording.reached_limit {
                    "Recording reached its limit and is ready to align."
                } else {
                    "Recording ready to align."
                };
                self.recording = Some(Arc::new(recording));
                self.info(status);
            },
            Err(error) => self.error(error),
        }
    }

    fn info(&mut self, text: impl Into<String>) {
        self.feedback = Some(Feedback {
            text: text.into(),
            error: false,
        });
    }

    pub(super) fn error(&mut self, text: impl Into<String>) {
        self.feedback = Some(Feedback {
            text: text.into(),
            error: true,
        });
    }
}

impl GuiShell {
    pub(crate) fn open_narration_takes(&mut self, ctx: &Context) {
        self.narration_takes.open(&self.document_store.source, ctx);
    }

    pub(crate) fn narration_takes_ui(&mut self, ctx: &Context) {
        if self.pending_open.is_some() || self.ui_store.view.welcome_open {
            if self.narration_takes.open {
                self.narration_takes.close();
            }
        } else if self.narration_takes.open {
            self.narration_takes.retarget(&self.document_store.source, false);
        }
        if let Some(adopted) = self.narration_takes.collect() {
            self.apply_narration_take(adopted);
        }
        self.narration_takes.collect_recording();
        self.narration_takes.try_refresh(ctx);
        if self.preview_store.preview.playback.is_playing
            || self
                .narration_takes
                .audition
                .as_ref()
                .is_some_and(|audition| audition.sink.empty())
        {
            self.narration_takes.audition = None;
        }
        if self.narration_takes.audition.is_some() || self.narration_takes.capture.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        let Some(action) = self.narration_takes.show(ctx) else {
            return;
        };
        match action {
            Action::Prepare(operation) => {
                self.narration_takes.audition = None;
                self.narration_takes.feedback = None;
                self.narration_takes.start(operation, ctx);
            },
            Action::Audition(path) => {
                self.preview_store.preview.playback.is_playing = false;
                self.preview_store.preview_dirty = true;
                self.narration_takes.audition = None;
                match Audition::start(&path) {
                    Ok(audition) => self.narration_takes.audition = Some(audition),
                    Err(error) => self.narration_takes.error(error),
                }
            },
            Action::Stop => self.narration_takes.audition = None,
            Action::Record => {
                self.preview_store.preview.playback.is_playing = false;
                self.preview_store.preview_dirty = true;
                self.narration_takes.audition = None;
                self.narration_takes.feedback = None;
                match Capture::start(ctx.clone()) {
                    Ok(capture) => {
                        self.narration_takes.inputs.timing.clear();
                        self.narration_takes.capture = Some(capture);
                    },
                    Err(error) => self.narration_takes.error(error),
                }
            },
            Action::StopRecording => {
                if let Some(capture) = &mut self.narration_takes.capture {
                    capture.stop();
                }
            },
            Action::DiscardRecording => {
                self.narration_takes.audition = None;
                self.narration_takes.clear_recording();
                self.narration_takes.info("Recording discarded.");
            },
            Action::Cancel => {
                if let Some(job) = &self.narration_takes.job {
                    job.cancel();
                }
            },
            Action::Refresh => {
                self.narration_takes.feedback = None;
                self.narration_takes.refresh_pending = true;
                self.narration_takes.try_refresh(ctx);
            },
        }
    }

    fn apply_narration_take(&mut self, adopted: Adopted) {
        let Some(target) = self.narration_takes.target.as_ref() else {
            return;
        };
        match target.patch(&self.document_store.source, &adopted) {
            Ok(patch) => {
                let before = self.document_store.source.epoch();
                let effects = self.handle_command(Command::ApplySourcePatch(patch));
                self.apply_effects(effects);
                if before == self.document_store.source.epoch() {
                    self.narration_takes.error("The current source could not adopt this take.");
                } else {
                    self.narration_takes
                        .info("Take adopted. Save the scene to keep the updated narration.");
                    self.narration_takes.affected = adopted.affected_markers;
                }
                self.narration_takes.retarget(&self.document_store.source, false);
            },
            Err(error) => self.narration_takes.error(error),
        }
    }
}

pub(super) struct Audition {
    sink: Sink,
    _stream: OutputStream,
}

impl Audition {
    fn start(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| format!("Could not open the take: {error}"))?;
        let decoded = Decoder::new(BufReader::new(file))
            .map_err(|error| format!("Could not decode the take: {error}"))?;
        let (_stream, handle) = OutputStream::try_default()
            .map_err(|error| format!("Could not open audio output: {error}"))?;
        let sink = Sink::try_new(&handle)
            .map_err(|error| format!("Could not start take playback: {error}"))?;
        sink.append(decoded);
        Ok(Self { sink, _stream })
    }
}
