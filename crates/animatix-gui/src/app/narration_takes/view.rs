use std::ffi::OsString;
use std::path::{Path, PathBuf};

use egui::{CollapsingHeader, ComboBox, Context, RichText, ScrollArea, TextEdit, Ui};

use crate::app::i18n::{StatusText, current_language, render, text, text_for};

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
            Self::Edge => "gui-narration-provider-edge",
            Self::Qwen => "gui-narration-provider-qwen",
            Self::Local => "gui-narration-provider-local",
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
        egui::Window::new(text_for(current_language(ctx), "gui-narration-voiceover-title"))
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
                let detail =
                    self.history_error.as_deref().map(std::borrow::Cow::Borrowed).or_else(|| {
                        self.feedback
                            .as_ref()
                            .map(|message| message.display(current_language(ui.ctx())))
                    });
                if let Some(detail) = detail {
                    let summary = text(ui, "gui-narration-needs-attention");
                    ui.colored_label(ui.visuals().error_fg_color, summary).on_hover_text(detail);
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
                ui.weak(text(ui, "gui-narration-write-paragraph"));
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
                    .map_or_else(
                        || text(ui, "gui-narration-choose-paragraph"),
                        |paragraph| paragraph.text.clone(),
                    ),
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
                    || text(ui, "gui-narration-stop-recording"),
                    |elapsed| {
                        render(
                            ui,
                            "gui-narration-stop-timed",
                            &[("duration", format!("{:.1}", elapsed.as_secs_f32()))],
                        )
                    },
                );
                if ui.add_enabled(!capture.stopping, eparts::Button::primary(label)).clicked() {
                    *action = Some(Action::StopRecording);
                }
            } else if let Some((job, _)) = &self.job {
                ui.spinner();
                ui.label(text(ui, job.status));
            } else if ui
                .add_enabled(
                    enabled,
                    eparts::Button::primary(crate::app::i18n::text(ui, "gui-narration-record")),
                )
                .clicked()
            {
                *action = Some(Action::Record);
            }
            ui.menu_button(crate::app::i18n::text(ui, "gui-narration-more"), |ui| {
                for (item, label) in [
                    (Advanced::Import, "gui-narration-audio-import-menu"),
                    (Advanced::Synthesize, "gui-narration-generate-menu"),
                    (Advanced::Alignment, "gui-narration-align-menu"),
                ] {
                    if ui.add_enabled(!busy, egui::Button::new(text(ui, label))).clicked() {
                        self.inputs.advanced = Some(item);
                        ui.close();
                    }
                }
            });
        });
        if !busy && let Some(advanced) = self.inputs.advanced {
            ui.horizontal(|ui| {
                ui.strong(match advanced {
                    Advanced::Import => text(ui, "gui-narration-audio-import"),
                    Advanced::Synthesize => text(ui, "gui-narration-generate"),
                    Advanced::Alignment => text(ui, "gui-narration-align-settings"),
                });
                if ui.small_button(text(ui, "gui-narration-close-controls")).clicked() {
                    self.inputs.advanced = None;
                }
            });
            let operation = match advanced {
                Advanced::Import => self.inputs.import_ui(ui, &target.folder),
                Advanced::Synthesize => self.inputs.synthesis_ui(ui),
                Advanced::Alignment => {
                    self.inputs.alignment_ui(ui);
                    ui.horizontal(|ui| {
                        ui.label(crate::app::i18n::text(ui, "gui-narration-python"));
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
        let recoverable = |take: &TakeSummary| {
            binding.is_some_and(|binding| binding.text == take.text && binding.scene == take.scene)
                && !take.aligned
                && take.status != "interrupted"
        };
        let pending = takes.iter().copied().find(|take| recoverable(take));
        let timing = self.inputs.import_timing();
        if let Some(take) = pending {
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::app::i18n::text(ui, "gui-narration-awaiting-alignment"));
                if !busy {
                    if matches!(timing, ImportTiming::Missing) {
                        if ui.small_button(text(ui, "gui-narration-align-setup")).clicked() {
                            self.inputs.advanced = Some(Advanced::Alignment);
                        }
                    } else if ui.small_button(text(ui, "gui-narration-align-retry")).clicked() {
                        *action = Some(Action::Start(
                            Operation::Import {
                                audio: None,
                                timing: self.inputs.import_timing(),
                            },
                            Some(take.id.clone()),
                        ));
                    }
                }
            });
        }
        CollapsingHeader::new(render(
            ui,
            "gui-narration-history",
            &[("count", (takes.len() + unmatched.len()).to_string())],
        ))
        .id_salt("voiceover_history")
        .show(ui, |ui| {
            ScrollArea::vertical().max_height(270.0).show(ui, |ui| {
                if self.audition.is_some()
                    && ui.small_button(text(ui, "gui-narration-stop-playback")).clicked()
                {
                    *action = Some(Action::StopListening);
                }
                for (index, take) in takes.iter().enumerate() {
                    let matches = binding.is_some_and(|binding| binding.text == take.text);
                    let current = self.active.contains(&take.id);
                    take_row(ui, take, takes.len() - index, current, action);
                    if !busy && matches && !current {
                        if take.aligned
                            && ui.small_button(text(ui, "gui-narration-use-take")).clicked()
                        {
                            *action = Some(Action::Start(Operation::Use, Some(take.id.clone())));
                        }
                        if recoverable(take)
                            && pending.is_some_and(|pending| pending.id != take.id)
                            && !matches!(timing, ImportTiming::Missing)
                            && ui.small_button(text(ui, "gui-narration-align-retry")).clicked()
                        {
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
                if !unmatched.is_empty() {
                    CollapsingHeader::new(text(ui, "gui-narration-unmatched"))
                        .id_salt("voiceover_unmatched")
                        .show(ui, |ui| {
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
                "imported" => text(ui, "gui-narration-take-imported"),
                "synthesized" => text(ui, "gui-narration-take-generated"),
                _ => text(ui, "gui-narration-take"),
            };
            ui.label(render(
                ui,
                "gui-narration-take-duration",
                &[
                    ("kind", label),
                    ("number", number.to_string()),
                    ("duration", format!("{:.1}", take.seconds)),
                ],
            ));
            if current {
                ui.strong(text(ui, "gui-narration-in-use"));
            }
            if take.status == "interrupted" {
                ui.label(crate::app::i18n::text(ui, "gui-narration-interrupted"));
            }
            if ui.small_button(text(ui, "gui-narration-play-take")).clicked() {
                *action = Some(Action::Listen(take.id.clone()));
            }
        });
        ui.label(RichText::new(&take.text).weak());
        if let Some(issue) = &take.issue {
            let message = if take.status == "interrupted" {
                text(ui, "gui-narration-capture-saved")
            } else if take.aligned {
                text(ui, "gui-narration-take-scene-stale")
            } else {
                text(ui, "gui-narration-take-align-pending")
            };
            ui.label(RichText::new(message).weak()).on_hover_text(issue);
        }
    });
}

impl Inputs {
    fn synthesis_ui(&mut self, ui: &mut Ui) -> Option<Result<Operation, StatusText>> {
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::app::i18n::text(ui, "gui-narration-provider"));
            let previous = self.provider;
            ComboBox::from_id_salt("narration_provider")
                .selected_text(text(ui, self.provider.label()))
                .show_ui(ui, |ui| {
                    for provider in [
                        VoiceProvider::Edge,
                        VoiceProvider::Qwen,
                        VoiceProvider::Local,
                    ] {
                        ui.selectable_value(
                            &mut self.provider,
                            provider,
                            text(ui, provider.label()),
                        );
                    }
                });
            if self.provider != previous {
                self.voice = self.provider.default_voice().into();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::app::i18n::text(ui, "gui-narration-voice"));
            ui.add(TextEdit::singleline(&mut self.voice).desired_width(215.0));
            ui.label(crate::app::i18n::text(ui, "gui-narration-rate"));
            ui.add(TextEdit::singleline(&mut self.rate).desired_width(60.0).hint_text("+0%"));
        });
        match self.provider {
            VoiceProvider::Edge => {
                ui.label(crate::app::i18n::text(ui, "gui-narration-edge-help"));
            },
            VoiceProvider::Qwen => {
                path_ui(ui, "gui-narration-path-qwen", &mut self.qwen_model, &[], true);
                self.alignment_ui(ui);
            },
            VoiceProvider::Local => {
                path_ui(ui, "gui-narration-path-provider", &mut self.provider_program, &[], false);
                ui.label(crate::app::i18n::text(ui, "gui-narration-provider-args"));
                ui.add(
                    TextEdit::multiline(&mut self.provider_arguments)
                        .desired_rows(3)
                        .desired_width(400.0),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::app::i18n::text(ui, "gui-narration-provider-revision"));
                    ui.add(TextEdit::singleline(&mut self.provider_revision).desired_width(150.0));
                });
            },
        }
        ui.add(eparts::Button::primary(crate::app::i18n::text(ui, "gui-narration-generate")))
            .clicked()
            .then(|| self.synthesis_request())
    }

    fn synthesis_request(&self) -> Result<Operation, StatusText> {
        if self.voice.trim().is_empty() {
            return Err(StatusText::message("gui-narration-voice-required", []));
        }
        let provider = match self.provider {
            VoiceProvider::Edge => Provider::Edge,
            VoiceProvider::Qwen => Provider::Qwen {
                model: path(&self.qwen_model)
                    .ok_or(StatusText::message("gui-narration-qwen-required", []))?,
                alignment: self.alignment()?,
            },
            VoiceProvider::Local => {
                let program = self.provider_program.trim();
                if program.is_empty() {
                    return Err(StatusText::message("gui-narration-program-required", []));
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

    fn import_ui(&mut self, ui: &mut Ui, folder: &Path) -> Option<Result<Operation, StatusText>> {
        path_ui(
            ui,
            "gui-narration-path-audio",
            &mut self.recording,
            &["wav", "mp3", "m4a", "flac", "ogg", "opus", "aac"],
            false,
        );
        path_ui(ui, "gui-narration-path-timing", &mut self.timing, &["json"], false);
        ui.add_enabled(
            path(&self.recording).is_some(),
            eparts::Button::primary(crate::app::i18n::text(ui, "gui-action-import")),
        )
        .clicked()
        .then(|| {
            let audio = folder.join(
                path(&self.recording)
                    .ok_or(StatusText::message("gui-narration-audio-required", []))?,
            );
            if !audio.is_file() {
                return Err(StatusText::message("gui-narration-audio-exists", []));
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
        path_ui(ui, "gui-narration-path-sensevoice", &mut self.alignment_model, &["onnx"], false);
        path_ui(ui, "gui-narration-path-tokens", &mut self.alignment_tokens, &["txt"], false);
        path_ui(ui, "gui-narration-path-lexicon", &mut self.alignment_lexicon, &["json"], false);
    }

    fn alignment(&self) -> Result<LocalAlignment, StatusText> {
        Ok(LocalAlignment {
            model: path(&self.alignment_model)
                .ok_or(StatusText::message("gui-narration-model-required", []))?,
            tokens: path(&self.alignment_tokens)
                .ok_or(StatusText::message("gui-narration-tokens-required", []))?,
            lexicon: path(&self.alignment_lexicon),
        })
    }
}

fn path_ui(
    ui: &mut Ui,
    label_id: &str,
    value: &mut String,
    extensions: &[&str],
    folder: bool,
) -> bool {
    let mut picked = false;
    let label = text(ui, label_id);
    ui.push_id(label_id, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(&label);
            ui.add(TextEdit::singleline(value).desired_width(250.0));
            if ui
                .button(crate::app::i18n::text(ui, "gui-action-browse"))
                .on_hover_text(&label)
                .clicked()
            {
                let mut dialog = rfd::FileDialog::new().set_title(&label);
                if let Some(directory) = path(value).as_deref().and_then(Path::parent) {
                    dialog = dialog.set_directory(directory);
                }
                if !extensions.is_empty() {
                    dialog = dialog.add_filter(&label, extensions);
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
