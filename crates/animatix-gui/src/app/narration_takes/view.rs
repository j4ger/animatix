use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Button, CollapsingHeader, ComboBox, Context, RichText, ScrollArea, TextEdit, Ui};

use super::NarrationTakes;
use super::capture::Recording;
use super::worker::{ImportTiming, LocalAlignment, Operation, Provider, RecordingInput};

pub(super) enum Action {
    Prepare(Operation),
    Audition(PathBuf),
    Stop,
    Record,
    StopRecording,
    DiscardRecording,
    Cancel,
    Refresh,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum InputMode {
    #[default]
    Synthesize,
    Import,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum VoiceProvider {
    #[default]
    Edge,
    Qwen,
    Local,
}

impl VoiceProvider {
    fn label(self) -> &'static str {
        match self {
            Self::Edge => "Edge TTS (online)",
            Self::Qwen => "Qwen3-TTS (local)",
            Self::Local => "Custom provider (local)",
        }
    }

    fn default_voice(self) -> &'static str {
        match self {
            Self::Edge => "en-US-AndrewMultilingualNeural",
            Self::Qwen => "Uncle_Fu",
            Self::Local => "",
        }
    }
}

pub(super) struct Inputs {
    mode: InputMode,
    provider: VoiceProvider,
    voice: String,
    rate: String,
    qwen_model: String,
    provider_program: String,
    provider_arguments: String,
    provider_revision: String,
    alignment_model: String,
    alignment_tokens: String,
    alignment_lexicon: String,
    pub recording: String,
    pub timing: String,
    pub python: String,
}

impl Default for Inputs {
    fn default() -> Self {
        let qwen_model = env("ANIMATIX_NARRATION_QWEN_MODEL");
        let alignment_model = env("ANIMATIX_NARRATION_ASR_MODEL");
        let alignment_tokens = env("ANIMATIX_NARRATION_ASR_TOKENS");
        let provider =
            if qwen_model.is_empty() || alignment_model.is_empty() || alignment_tokens.is_empty() {
                VoiceProvider::Edge
            } else {
                VoiceProvider::Qwen
            };
        Self {
            mode: InputMode::Synthesize,
            provider,
            voice: provider.default_voice().into(),
            rate: "+0%".into(),
            qwen_model,
            provider_program: String::new(),
            provider_arguments: String::new(),
            provider_revision: String::new(),
            alignment_model,
            alignment_tokens,
            alignment_lexicon: env("ANIMATIX_NARRATION_ASR_LEXICON"),
            recording: String::new(),
            timing: String::new(),
            python: env("ANIMATIX_NARRATION_PYTHON"),
        }
    }
}

impl NarrationTakes {
    pub(super) fn show(&mut self, ctx: &Context) -> Option<Action> {
        if !self.open {
            return None;
        }
        let mut open = self.open;
        let mut action = None;
        egui::Window::new("Narration Takes")
            .id(egui::Id::unique("narration_takes"))
            .open(&mut open)
            .default_width(640.0)
            .vscroll(true)
            .show(ctx, |ui| {
                self.status_ui(ui, &mut action);
                if self.target.is_none() {
                    return;
                }
                let busy = self.job.is_some() || self.capture.is_some();
                let current_scene = self.scene.as_deref().unwrap_or("Current timeline");
                ui.label(RichText::new(current_scene).strong());
                ui.separator();
                ui.add_enabled_ui(!busy, |ui| {
                    self.paragraph_ui(ui);
                    self.candidate_ui(ui, &mut action);
                });
                ui.add_enabled_ui(self.job.is_none(), |ui| {
                    self.inputs_ui(ui, &mut action);
                });
                self.review_ui(ui);
            });
        if !open {
            self.close();
            return None;
        }
        action
    }

    fn status_ui(&mut self, ui: &mut Ui, action: &mut Option<Action>) {
        if let Some(job) = &self.job {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                let cancelling = job.cancelling();
                ui.label(if cancelling {
                    "Cancelling…"
                } else {
                    job.status
                });
                if ui.add_enabled(!cancelling, Button::new("Cancel job")).clicked() {
                    *action = Some(Action::Cancel);
                }
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                if self.target.is_some()
                    && self.capture.is_none()
                    && ui.small_button("Refresh takes").clicked()
                {
                    *action = Some(Action::Refresh);
                }
                if self.audition.is_some() && ui.small_button("Stop audition").clicked() {
                    *action = Some(Action::Stop);
                }
            });
        }
        if let Some(feedback) = &self.feedback {
            let color = if feedback.error {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().text_color()
            };
            ui.label(RichText::new(&feedback.text).color(color));
        }
    }

    fn paragraph_ui(&mut self, ui: &mut Ui) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let mut selection = self.selected.clone();
        let current_index = self.selected_index(target);
        let selected_text = current_index
            .map_or_else(|| "Choose paragraph".into(), |i| format!("Paragraph {}", i + 1));
        let selected_status = current_index.and_then(|i| {
            let id = target.paragraphs[i].id();
            self.takes.as_ref()?.paragraphs.iter().find(|state| state.id == id)
        });
        ui.horizontal_wrapped(|ui| {
            ComboBox::from_id_salt("narration_paragraph")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for paragraph in &target.paragraphs {
                        let excerpt: String = paragraph.text.chars().take(52).collect();
                        let label = if excerpt.len() < paragraph.text.len() {
                            format!("{}. {excerpt}…", paragraph.index)
                        } else {
                            format!("{}. {excerpt}", paragraph.index)
                        };
                        ui.selectable_value(&mut selection, Some(paragraph.id()), label);
                    }
                });
            if let Some(state) = selected_status {
                let (label, color) = match state.status.as_str() {
                    "ready" => ("Adopted", ui.visuals().text_color()),
                    "stale" => ("Text changed since last take", ui.visuals().warn_fg_color),
                    _ => ("No adopted take", ui.visuals().weak_text_color()),
                };
                ui.label(RichText::new(label).color(color));
            }
        });
        if selection != self.selected
            && let Some(paragraph) = selection
        {
            self.set_paragraph(paragraph);
        }
        let Some(target) = self.target.as_ref() else {
            return;
        };
        if let Some(index) = self.selected_index(target) {
            ui.add_space(4.0);
            ui.add(egui::Label::new(&target.paragraphs[index].text).wrap().selectable(true));
        }
    }

    fn candidate_ui(&mut self, ui: &mut Ui, action: &mut Option<Action>) {
        let (Some(takes), Some(selected)) = (&self.takes, &self.selected) else {
            return;
        };
        let Some(state) = takes.paragraphs.iter().find(|state| state.id == *selected) else {
            return;
        };
        if state.candidates.is_empty() {
            return;
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Candidate takes").strong());
        ScrollArea::vertical()
            .id_salt("narration_candidates")
            .max_height(155.0)
            .show(ui, |ui| {
                for (i, candidate) in state.candidates.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        let label = match (&candidate.voice, &candidate.rate) {
                            (Some(voice), Some(rate)) => format!("{voice} · {rate}"),
                            _ if candidate.kind == "recorded" => "Recording".to_owned(),
                            _ => "Voice take".to_owned(),
                        };
                        let active = state.active_take.as_ref() == Some(&candidate.candidate);
                        let active_label = if active { " · adopted" } else { "" };
                        let label = format!(
                            "{}. {label} · {:.2}s · {} timed words{active_label}",
                            i + 1,
                            candidate.duration,
                            candidate.word_count,
                        );
                        ui.selectable_value(
                            &mut self.chosen,
                            Some(candidate.candidate.clone()),
                            label,
                        );
                        if ui.button("Play").on_hover_text("Audition this take").clicked() {
                            self.chosen = Some(candidate.candidate.clone());
                            *action = Some(Action::Audition(candidate.audio.clone()));
                        }
                    });
                }
            });
        let selected_candidate = self.chosen.as_ref().filter(|chosen| {
            state.active_take.as_ref() != Some(chosen)
                && state.candidates.iter().any(|candidate| &candidate.candidate == *chosen)
        });
        if ui
            .add_enabled(selected_candidate.is_some(), Button::new("Adopt selected take"))
            .clicked()
            && let Some(candidate) = selected_candidate
        {
            *action = Some(Action::Prepare(Operation::Adopt {
                paragraph: selected.clone(),
                candidate: candidate.clone(),
                expected_revision: takes.revision.clone(),
            }));
        }
    }

    fn inputs_ui(&mut self, ui: &mut Ui, action: &mut Option<Action>) {
        let Some(paragraph) = self.selected.as_deref() else {
            return;
        };
        ui.add_space(8.0);
        ui.separator();
        if let Some(capture) = &self.capture {
            let elapsed = capture.elapsed();
            ui.horizontal_wrapped(|ui| {
                let label = match (capture.stopping, elapsed) {
                    (true, _) => "Finishing recording…".into(),
                    (false, Some(elapsed)) => format!("Recording · {:.1}s", elapsed.as_secs_f64()),
                    (false, None) => "Opening microphone…".into(),
                };
                ui.label(RichText::new(label).strong());
                if ui
                    .add_enabled(elapsed.is_some() && !capture.stopping, Button::new("Stop"))
                    .clicked()
                {
                    *action = Some(Action::StopRecording);
                }
                if ui.button("Discard").clicked() {
                    *action = Some(Action::DiscardRecording);
                }
            });
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.inputs.mode, InputMode::Synthesize, "Generate a take");
            ui.selectable_value(&mut self.inputs.mode, InputMode::Import, "Record or import audio");
        });
        let operation = match self.inputs.mode {
            InputMode::Synthesize => self.inputs.synthesis_ui(ui, paragraph),
            InputMode::Import => {
                if let Some(recording) = &self.recording {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Microphone recording · {:.1}s", recording.seconds));
                        if ui.button("Play recording").clicked() {
                            *action = Some(Action::Audition(recording.path().to_owned()));
                        }
                        if ui.button("Discard").clicked() {
                            *action = Some(Action::DiscardRecording);
                        }
                    });
                } else if ui
                    .button("Record")
                    .on_hover_text("Use the default microphone for up to 10 minutes or 128 MiB")
                    .clicked()
                {
                    *action = Some(Action::Record);
                }
                self.inputs.recording_ui(ui, paragraph, self.recording.as_ref())
            },
        };
        if let Some(operation) = operation {
            match operation {
                Ok(operation) => *action = Some(Action::Prepare(operation)),
                Err(error) => self.error(error),
            }
        }
        CollapsingHeader::new("Python settings").show(ui, |ui| {
            path_ui(ui, "Python executable", &mut self.inputs.python, &[], false);
            ui.label("Leave blank to use Python from PATH.");
        });
    }

    fn review_ui(&self, ui: &mut Ui) {
        if self.affected.is_empty() {
            return;
        }
        ui.separator();
        CollapsingHeader::new("Changed speech timing")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Times are relative to the narration clip.");
                egui::Grid::new("retake_timing_changes").striped(true).show(ui, |ui| {
                    ui.strong("Anchor");
                    ui.strong("Before");
                    ui.strong("After");
                    ui.end_row();
                    for change in &self.affected {
                        ui.label(&change.marker);
                        ui.label(seconds(change.before));
                        ui.label(seconds(change.after));
                        ui.end_row();
                    }
                });
            });
    }
}

impl Inputs {
    fn synthesis_ui(&mut self, ui: &mut Ui, paragraph: &str) -> Option<Result<Operation, String>> {
        ui.horizontal_wrapped(|ui| {
            ui.label("Voice provider");
            let previous = self.provider;
            ComboBox::from_id_salt("narration_provider")
                .selected_text(self.provider.label())
                .show_ui(ui, |ui| {
                    for provider in [
                        VoiceProvider::Edge,
                        VoiceProvider::Qwen,
                        VoiceProvider::Local,
                    ] {
                        ui.selectable_value(&mut self.provider, provider, provider.label());
                    }
                });
            if self.provider != previous {
                self.voice = self.provider.default_voice().into();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Voice");
            ui.add(TextEdit::singleline(&mut self.voice).desired_width(215.0));
            ui.label("Rate");
            ui.add(TextEdit::singleline(&mut self.rate).desired_width(60.0).hint_text("+0%"));
        });
        match self.provider {
            VoiceProvider::Edge => {
                ui.label("Sends the selected paragraph to Edge TTS.");
            },
            VoiceProvider::Qwen => {
                path_ui(ui, "Qwen model folder", &mut self.qwen_model, &[], true);
                self.alignment_ui(ui);
            },
            VoiceProvider::Local => {
                path_ui(ui, "Provider executable", &mut self.provider_program, &[], false);
                ui.label("Provider arguments, one per line");
                ui.add(
                    TextEdit::multiline(&mut self.provider_arguments)
                        .desired_rows(3)
                        .desired_width(400.0),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label("Provider revision");
                    ui.add(TextEdit::singleline(&mut self.provider_revision).desired_width(150.0));
                });
            },
        }
        ui.button("Generate candidate")
            .clicked()
            .then(|| self.synthesis_request(paragraph))
    }

    fn synthesis_request(&self, paragraph: &str) -> Result<Operation, String> {
        if self.voice.trim().is_empty() {
            return Err("Enter a voice name for the selected provider.".into());
        }
        let provider = match self.provider {
            VoiceProvider::Edge => Provider::Edge,
            VoiceProvider::Qwen => Provider::Qwen {
                model: path(&self.qwen_model).ok_or("Choose a local Qwen model folder.")?,
                alignment: self.alignment()?,
            },
            VoiceProvider::Local => {
                let program = self.provider_program.trim();
                if program.is_empty() {
                    return Err("Choose a local provider executable.".into());
                }
                Provider::Local {
                    executable: OsString::from(program),
                    arguments: self
                        .provider_arguments
                        .lines()
                        .filter(|line| !line.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    revision: self.provider_revision.trim().to_owned(),
                }
            },
        };
        Ok(Operation::Synthesize {
            paragraph: paragraph.to_owned(),
            voice: self.voice.trim().to_owned(),
            rate: self.rate.trim().to_owned(),
            provider,
        })
    }

    fn recording_ui(
        &mut self,
        ui: &mut Ui,
        paragraph: &str,
        microphone: Option<&Arc<Recording>>,
    ) -> Option<Result<Operation, String>> {
        if microphone.is_none()
            && path_ui(
                ui,
                "Audio recording",
                &mut self.recording,
                &["wav", "mp3", "m4a", "flac", "ogg", "opus", "aac"],
                false,
            )
        {
            self.timing = path(&self.recording)
                .map(|recording| recording.with_extension("speech.json"))
                .filter(|timing| timing.is_file())
                .map(|timing| timing.display().to_string())
                .unwrap_or_default();
        }
        path_ui(ui, "Word timing JSON", &mut self.timing, &["json"], false);
        if self.timing.trim().is_empty() {
            ui.label(
                "Choose word timing JSON or align the recording with a local SenseVoice model.",
            );
            self.alignment_ui(ui);
        }
        ui.button("Prepare recording candidate").clicked().then(|| {
            let audio = match microphone {
                Some(recording) => RecordingInput::Microphone(Arc::clone(recording)),
                None => RecordingInput::File(
                    path(&self.recording)
                        .ok_or("Record or choose an audio file for the selected paragraph.")?,
                ),
            };
            let timing = match path(&self.timing) {
                Some(path) => ImportTiming::Sidecar(path),
                None => ImportTiming::Align(self.alignment()?),
            };
            Ok(Operation::Import {
                paragraph: paragraph.to_owned(),
                audio,
                timing,
            })
        })
    }

    fn alignment_ui(&mut self, ui: &mut Ui) {
        path_ui(ui, "SenseVoice ONNX model", &mut self.alignment_model, &["onnx"], false);
        path_ui(ui, "SenseVoice tokens", &mut self.alignment_tokens, &["txt"], false);
        path_ui(ui, "Lexicon JSON (optional)", &mut self.alignment_lexicon, &["json"], false);
    }

    fn alignment(&self) -> Result<LocalAlignment, String> {
        Ok(LocalAlignment {
            model: path(&self.alignment_model).ok_or("Choose a local SenseVoice ONNX model.")?,
            tokens: path(&self.alignment_tokens).ok_or("Choose the SenseVoice token file.")?,
            lexicon: path(&self.alignment_lexicon),
        })
    }
}

fn path_ui(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    extensions: &[&str],
    folder: bool,
) -> bool {
    let mut picked = false;
    ui.push_id(label, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            ui.add(TextEdit::singleline(value).desired_width(250.0));
            if ui.button("Browse…").on_hover_text(label).clicked() {
                let mut dialog = rfd::FileDialog::new().set_title(label);
                if let Some(directory) = path(value).as_deref().and_then(Path::parent) {
                    dialog = dialog.set_directory(directory);
                }
                if !extensions.is_empty() {
                    dialog = dialog.add_filter(label, extensions);
                }
                if let Some(selected) = if folder {
                    dialog.pick_folder()
                } else {
                    dialog.pick_file()
                } {
                    *value = selected.display().to_string();
                    picked = true;
                }
            }
        });
    });
    picked
}

fn path(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    (!value.is_empty()).then(|| PathBuf::from(value))
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn seconds(value: Option<f64>) -> String {
    value.map_or_else(|| "Unresolved".into(), |seconds| format!("{seconds:.3}s"))
}
