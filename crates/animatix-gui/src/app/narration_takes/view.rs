use std::ffi::OsString;
use std::path::{Path, PathBuf};

use egui::{CollapsingHeader, ComboBox, Context, RichText, ScrollArea, TextEdit, Ui};

use super::NarrationTakes;
use super::history::TakeSummary;
use super::worker::{ImportTiming, LocalAlignment, Operation, Provider};

pub(super) enum Action {
    Record,
    StopRecording,
    Select(String),
    Listen(String),
    StopListening,
    Start(Operation, Option<String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Advanced {
    Import,
    Synthesize,
    Alignment,
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
    advanced: Option<Advanced>,
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
    recording: String,
    timing: String,
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
            advanced: None,
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
        egui::Window::new("Voiceover")
            .id(egui::Id::unique("narration_takes"))
            .open(&mut open)
            .auto_sized()
            .default_width(416.0)
            .default_pos(ctx.content_rect().left_top() + egui::vec2(185.0, 54.0))
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_width(416.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                self.controls(ui, &mut action);
                self.history_ui(ui, &mut action);
                if let Some(error) = self.history_error.as_ref().or(self.feedback.as_ref()) {
                    let summary = if error.contains('\n') || error.len() > 150 {
                        "Voiceover needs attention."
                    } else {
                        error
                    };
                    ui.colored_label(ui.visuals().error_fg_color, summary).on_hover_text(error);
                }
            });
        if !open {
            self.close();
        }
        action
    }

    fn controls(&mut self, ui: &mut Ui, action: &mut Option<Action>) {
        let Some(target) = self.target.as_ref() else {
            if self.feedback.is_none() && self.history_error.is_none() {
                ui.weak("Write a narration paragraph in Script view.");
            }
            return;
        };
        ComboBox::from_id_salt("voiceover_paragraph")
            .width(400.0)
            .selected_text(
                target
                    .paragraphs
                    .iter()
                    .find(|paragraph| self.selected.as_deref() == Some(paragraph.id().as_str()))
                    .map_or("Select a paragraph", |paragraph| paragraph.text.as_str()),
            )
            .show_ui(ui, |ui| {
                for paragraph in &target.paragraphs {
                    let id = paragraph.id();
                    if ui
                        .selectable_label(self.selected.as_ref() == Some(&id), &paragraph.text)
                        .clicked()
                    {
                        *action = Some(Action::Select(id));
                    }
                }
            });
        let busy = self.capture.is_some() || self.job.is_some();
        let enabled = self.selected.is_some() && self.history_error.is_none();
        ui.horizontal(|ui| {
            if let Some((capture, _, _)) = &self.capture {
                let label = capture.elapsed().map_or_else(
                    || "Stop".into(),
                    |elapsed| format!("Stop · {:.1}s", elapsed.as_secs_f32()),
                );
                if ui.add_enabled(!capture.stopping, eparts::Button::primary(label)).clicked() {
                    *action = Some(Action::StopRecording);
                }
            } else if let Some((job, _)) = &self.job {
                ui.spinner();
                ui.label(job.status);
            } else if ui.add_enabled(enabled, eparts::Button::primary("Record")).clicked() {
                *action = Some(Action::Record);
            }
            ui.menu_button("More", |ui| {
                for (item, label) in [
                    (Advanced::Import, "Import audio…"),
                    (Advanced::Synthesize, "Generate voiceover…"),
                    (Advanced::Alignment, "Speech alignment settings…"),
                ] {
                    if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                        self.inputs.advanced = Some(item);
                        ui.close();
                    }
                }
            });
        });
        if !busy && let Some(advanced) = self.inputs.advanced {
            ui.horizontal(|ui| {
                ui.strong(match advanced {
                    Advanced::Import => "Import audio",
                    Advanced::Synthesize => "Generate voiceover",
                    Advanced::Alignment => "Speech alignment settings",
                });
                if ui.small_button("Close").clicked() {
                    self.inputs.advanced = None;
                }
            });
            let operation = match advanced {
                Advanced::Import => self.inputs.import_ui(ui, &target.folder),
                Advanced::Synthesize => self.inputs.synthesis_ui(ui),
                Advanced::Alignment => {
                    self.inputs.alignment_ui(ui);
                    ui.horizontal(|ui| {
                        ui.label("Python");
                        ui.add(
                            TextEdit::singleline(&mut self.inputs.python)
                                .hint_text(if cfg!(windows) { "python" } else { "python3" }),
                        );
                    });
                    None
                },
            };
            match operation {
                Some(Ok(operation)) => {
                    self.inputs.advanced = None;
                    *action = Some(Action::Start(operation, None));
                },
                Some(Err(error)) => self.feedback = Some(error),
                None => {},
            }
        }
    }

    fn history_ui(&mut self, ui: &mut Ui, action: &mut Option<Action>) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let binding = self
            .selected
            .as_deref()
            .and_then(|selector| session.binding(self.scene.as_deref(), selector));
        let takes: Vec<_> = self
            .takes
            .iter()
            .filter(|take| binding.is_some_and(|binding| take.paragraph == binding.id))
            .collect();
        let unmatched: Vec<_> = self
            .takes
            .iter()
            .filter(|take| !session.bindings.iter().any(|binding| binding.id == take.paragraph))
            .collect();
        if takes.is_empty() && unmatched.is_empty() {
            return;
        }
        let busy = self.capture.is_some() || self.job.is_some();
        let history = CollapsingHeader::new(format!("History ({})", takes.len() + unmatched.len()))
            .id_salt("voiceover_history")
            .show(ui, |ui| {
                ScrollArea::vertical().max_height(270.0).show(ui, |ui| {
                    if self.audition.is_some() && ui.small_button("Stop playback").clicked() {
                        *action = Some(Action::StopListening);
                    }
                    for (index, take) in takes.iter().enumerate() {
                        let matches = binding.is_some_and(|binding| binding.text == take.text);
                        let current = self.active.contains(&take.id);
                        take_row(ui, take, takes.len() - index, current, action);
                        if !busy && matches && !current {
                            if take.aligned && ui.small_button("Use this take").clicked() {
                                *action =
                                    Some(Action::Start(Operation::Use, Some(take.id.clone())));
                            }
                            if !take.aligned && take.issue.is_some() && take.status != "interrupted"
                            {
                                if matches!(self.inputs.import_timing(), ImportTiming::Missing) {
                                    if ui.small_button("Set up speech alignment").clicked() {
                                        self.inputs.advanced = Some(Advanced::Alignment);
                                    }
                                } else if ui.small_button("Retry alignment").clicked() {
                                    *action = Some(Action::Start(
                                        Operation::Import {
                                            audio: None,
                                            timing: self.inputs.import_timing(),
                                        },
                                        Some(take.id.clone()),
                                    ));
                                }
                            }
                        }
                    }
                    if !unmatched.is_empty() {
                        CollapsingHeader::new("Unmatched recordings").show(ui, |ui| {
                            for (index, take) in unmatched.iter().enumerate() {
                                if let Some(scene) = &take.scene {
                                    ui.weak(scene);
                                }
                                take_row(
                                    ui,
                                    take,
                                    unmatched.len() - index,
                                    self.active.contains(&take.id),
                                    action,
                                );
                            }
                        });
                    }
                });
            });
        if !busy
            && history.body_response.is_none()
            && let Some(take) = takes.first().filter(|take| take.issue.is_some() && !take.aligned)
        {
            if take.status == "interrupted" {
                ui.weak("Recording interrupted. Audio kept.");
                return;
            }
            ui.horizontal(|ui| {
                ui.label("Recording saved. Alignment needed.");
                if matches!(self.inputs.import_timing(), ImportTiming::Missing) {
                    if ui.small_button("Set up").clicked() {
                        self.inputs.advanced = Some(Advanced::Alignment);
                    }
                } else if ui.small_button("Retry").clicked()
                    && binding.is_some_and(|b| b.text == take.text)
                {
                    *action = Some(Action::Start(
                        Operation::Import {
                            audio: None,
                            timing: self.inputs.import_timing(),
                        },
                        Some(take.id.clone()),
                    ));
                }
            });
        }
    }
}

fn take_row(
    ui: &mut Ui,
    take: &TakeSummary,
    number: usize,
    current: bool,
    action: &mut Option<Action>,
) {
    ui.push_id(&take.id, |ui| {
        ui.horizontal_wrapped(|ui| {
            let label = match take.kind.as_str() {
                "imported" => "Imported",
                "synthesized" => "Generated",
                _ => "Take",
            };
            ui.label(format!("{label} {number} · {:.1}s", take.seconds));
            if current {
                ui.strong("In use");
            }
            if take.status == "interrupted" {
                ui.label("Interrupted");
            }
            if ui.small_button("Play").clicked() {
                *action = Some(Action::Listen(take.id.clone()));
            }
        });
        ui.label(RichText::new(&take.text).weak());
        if let Some(issue) = &take.issue {
            let message = if take.status == "interrupted" {
                "Captured audio saved"
            } else if take.aligned {
                "Saved to history · scene not updated"
            } else {
                "Saved to history · alignment needed"
            };
            ui.label(RichText::new(message).weak()).on_hover_text(issue);
        }
    });
}

impl Inputs {
    fn synthesis_ui(&mut self, ui: &mut Ui) -> Option<Result<Operation, String>> {
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
        ui.add(eparts::Button::primary("Generate voiceover"))
            .clicked()
            .then(|| self.synthesis_request())
    }

    fn synthesis_request(&self) -> Result<Operation, String> {
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
            voice: self.voice.trim().to_owned(),
            rate: self.rate.trim().to_owned(),
            provider,
        })
    }

    pub fn import_timing(&self) -> ImportTiming {
        self.alignment().map_or(ImportTiming::Missing, ImportTiming::Align)
    }

    fn import_ui(&mut self, ui: &mut Ui, folder: &Path) -> Option<Result<Operation, String>> {
        path_ui(
            ui,
            "Audio",
            &mut self.recording,
            &["wav", "mp3", "m4a", "flac", "ogg", "opus", "aac"],
            false,
        );
        path_ui(ui, "Timing file (optional)", &mut self.timing, &["json"], false);
        ui.add_enabled(path(&self.recording).is_some(), eparts::Button::primary("Import"))
            .clicked()
            .then(|| {
                let audio = folder.join(path(&self.recording).ok_or("Choose an audio file.")?);
                if !audio.is_file() {
                    return Err("Choose an existing audio file.".into());
                }
                let timing = path(&self.timing)
                    .map(|path| folder.join(path))
                    .or_else(|| {
                        let sidecar = audio.with_extension("speech.json");
                        sidecar.is_file().then_some(sidecar)
                    })
                    .map_or_else(|| self.import_timing(), ImportTiming::Sidecar);
                self.recording.clear();
                self.timing.clear();
                Ok(Operation::Import {
                    audio: Some(audio),
                    timing,
                })
            })
    }

    fn alignment_ui(&mut self, ui: &mut Ui) {
        path_ui(ui, "SenseVoice ONNX model", &mut self.alignment_model, &["onnx"], false);
        path_ui(ui, "SenseVoice tokens", &mut self.alignment_tokens, &["txt"], false);
        path_ui(ui, "Lexicon (optional)", &mut self.alignment_lexicon, &["json"], false);
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
