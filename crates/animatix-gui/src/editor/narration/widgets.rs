use animatix_syntax::narration::{AnchorEdge, decode_anchor, encode_anchor, paragraphs};
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;

use super::projection::cursor_to_byte;
use super::{Binding, EditorBuffer, Index, ScriptSelection, anchor_for};
use crate::app::commands::{ActionQueue, Command};
use crate::app::components::button::Button;
use crate::app::document::narration::{NarrationTrack, VoiceoverHistory, voiceover};
use crate::app::document::version::SourceEpoch;
use crate::editor::source_patch::SourcePatch;
use crate::highlighting::highlight_source;

impl EditorBuffer {
    pub(crate) fn show_script(
        &mut self,
        ui: &mut egui::Ui,
        epoch: SourceEpoch,
        document: &crate::app::document::DocumentSession,
        history: &VoiceoverHistory,
        commands: &mut ActionQueue,
    ) {
        let index = self.narration_index();
        let revealed = self.narration.reveal.take();
        let generation = self.narration.widgets_generation;
        let reveal_scroll_id = ("script-projection-scroll", self.instance_id, generation);
        egui::ScrollArea::vertical().id_salt(reveal_scroll_id).show(ui, |ui| {
            self.show_repair_control(ui, &index);
            self.show_binding_choices(ui, &index, epoch, commands);
            let mut last_scene = None;
            for (ordinal, paragraph) in index.paragraphs.iter().enumerate() {
                if let Some(scene) = paragraph.scene.as_deref()
                    && last_scene != Some(scene)
                {
                    ui.heading(scene);
                    last_scene = Some(scene);
                }
                let script = &index.script[ordinal];
                let mut text = script.text.clone();
                let id =
                    egui::Id::unique(("script-paragraph", self.instance_id, generation, ordinal));
                let pending = revealed.as_ref().filter(|reveal| reveal.paragraph == ordinal);
                if let Some(reveal) = pending {
                    let range = &reveal.range;
                    let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
                    state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::new(text[..range.start].chars().count()),
                        CCursor::new(text[..range.end].chars().count()),
                    )));
                    state.store(ui.ctx(), id);
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
                let draft = egui::TextEdit::multiline(&mut text)
                    .id(id)
                    .font(egui::FontId::proportional(16.0))
                    .margin(egui::Margin::symmetric(9, 7))
                    .desired_width(f32::INFINITY)
                    .desired_rows(1)
                    .show(ui);
                if pending.is_some() {
                    draft.response.scroll_to_me(Some(egui::Align::Center));
                }
                if draft.response.changed()
                    && let Some((range, replacement)) = script.edit(&text)
                {
                    let mut candidate = self.text().to_owned();
                    candidate.replace_range(range.clone(), &replacement);
                    if paragraphs(&candidate).len() == index.paragraphs.len() {
                        self.projection_edit(range, &replacement);
                    } else if let Some(patch) = SourcePatch::new(
                        self,
                        epoch,
                        range,
                        replacement,
                        "Edit narration paragraphs",
                    ) {
                        // A split, merge or removal changes the paragraph IDs
                        // used by projected widgets, including their native
                        // undo state. Record these edits in document history.
                        commands.push_back(Command::ApplySourcePatch(patch).into());
                    }
                    // Ranges for later paragraphs derive from the pre-edit source.
                    return;
                }
                if draft.response.has_focus() {
                    self.narration.selection = draft.cursor_range.and_then(|range| {
                        let [start, end] = range.sorted_cursors();
                        let start = cursor_to_byte(&text, start.index.0);
                        let end = cursor_to_byte(&text, end.index.0);
                        (start < end).then_some(ScriptSelection {
                            paragraph: ordinal,
                            range: start..end,
                            revision: self.source_revision,
                        })
                    });
                }
                let track = document
                    .narration_timeline(paragraph.scene.as_deref())
                    .and_then(NarrationTrack::find);
                let (status, _) =
                    voiceover(history, track.as_ref(), paragraph, self.narration_anchor_changes());
                let status = status.label();
                self.script_actions(ui, &index, ordinal, epoch, commands, status);
                ui.add_space(ui.spacing().item_spacing.y);
            }
            self.add_paragraph_ui(ui, &index, epoch, document.active_scene.as_deref(), commands);
        });
        self.binding_dialog(ui, &index, epoch, commands);
    }

    pub(crate) fn show_code_projection(&mut self, ui: &mut egui::Ui) {
        if self.narration.code_edit_rejected {
            let full_source = ui
                .horizontal_wrapped(|ui| {
                    ui.label("This edit crosses hidden script.");
                    ui.add(Button::ghost("Edit in full source")).clicked()
                })
                .inner;
            if full_source {
                self.set_source_view(super::SourceView::Full);
                return;
            }
        }
        let index = self.narration_index();
        let generation = self.narration.widgets_generation;
        let mut text = index.code.text.clone();
        let output = egui::ScrollArea::both()
            .id_salt(("code-projection-scroll", self.instance_id, generation))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _wrap: f32| {
                    let job = highlight_source(buffer.as_str(), ui.style(), &[], None, &[]);
                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                };
                egui::TextEdit::multiline(&mut text)
                    .id(egui::Id::unique(("code-projection", self.instance_id, generation)))
                    .code_editor()
                    .desired_width(ui.available_width())
                    .desired_rows(index.code.text.lines().count().max(1))
                    .layouter(&mut layouter)
                    .show(ui)
            })
            .inner;
        if !output.response.changed() || text == index.code.text {
            return;
        }
        if let Some((range, replacement)) = index.code.edit(self.text(), &text, &index.paragraphs) {
            self.projection_edit(range, &replacement);
            self.narration.code_edit_rejected = false;
        } else {
            // Egui saw a local edit, so discard the projected undo branch.
            self.narration.reset_widgets();
            self.narration.code_edit_rejected = true;
            ui.ctx().request_repaint();
        }
    }

    fn script_actions(
        &mut self,
        ui: &mut egui::Ui,
        index: &Index,
        ordinal: usize,
        epoch: SourceEpoch,
        commands: &mut ActionQueue,
        voiceover_status: &str,
    ) {
        let paragraph = &index.paragraphs[ordinal];
        let selection = self
            .narration
            .selection
            .as_ref()
            .filter(|selection| {
                selection.paragraph == ordinal && selection.revision == self.source_revision
            })
            .map(|selection| selection.range.clone());
        let label = if selection.is_some() {
            "Bind phrase"
        } else {
            "Bind paragraph"
        };
        let anchor = anchor_for(paragraph, &index.script[ordinal], selection);
        let has_targets = index.targets.iter().any(|target| target.scene == paragraph.scene);
        ui.horizontal_wrapped(|ui| {
            if !self.manual_repair_action(ui, index, ordinal, epoch, commands) {
                let response = ui
                    .add_enabled(
                        anchor.is_some() && has_targets,
                        Button::ghost(label).with_size(eparts::Size::Sm),
                    )
                    .on_disabled_hover_text(
                        "Select spoken text in a scene with a direct action or keyframe.",
                    );
                if response.clicked()
                    && let Some(anchor) = anchor
                {
                    self.narration.binding = Some(Binding {
                        scene: paragraph.scene.clone(),
                        revision: self.source_revision,
                        anchor,
                    });
                }
            }
            if ui.add(Button::ghost("Voiceover…").with_size(eparts::Size::Sm)).clicked() {
                commands.push_back(
                    Command::OpenParagraphTakes {
                        epoch,
                        scene: paragraph.scene.clone(),
                        paragraph: paragraph.id(),
                    }
                    .into(),
                );
            }
            ui.weak(voiceover_status);
            let mut linked = index
                .targets
                .iter()
                .filter(|target| {
                    target.scene == paragraph.scene
                        && target
                            .marker
                            .as_deref()
                            .and_then(decode_anchor)
                            .is_some_and(|key| key.matches_paragraph(paragraph))
                })
                .peekable();
            if linked.peek().is_none() {
                return;
            }
            let response = ui.add(Button::ghost("Bound cues").with_size(eparts::Size::Sm));
            egui::Popup::menu(&response).show(|ui| {
                for target in linked {
                    let Some(marker) = target.marker.as_deref() else {
                        continue;
                    };
                    let Some(key) = decode_anchor(marker) else {
                        continue;
                    };
                    let phrase = key
                        .phrase_range(&paragraph.text)
                        .map_or(paragraph.text.as_str(), |range| &paragraph.text[range]);
                    ui.label(format!("{} · {}", target.label, edge_label(key.edge())));
                    ui.label(phrase);
                    ui.horizontal(|ui| {
                        if ui.add(Button::ghost("Show source")).clicked() {
                            commands.push_back(Command::ScrollToLine(target.line, 0).into());
                            ui.close();
                        }
                        if ui.add(Button::ghost("Review cue")).clicked() {
                            commands.push_back(
                                Command::ReviewNarrationCue {
                                    epoch,
                                    scene: target.scene.clone(),
                                    line: target.line,
                                    marker: marker.to_owned(),
                                }
                                .into(),
                            );
                            ui.close();
                        }
                    });
                }
            });
        });
    }

    fn binding_dialog(
        &mut self,
        ui: &mut egui::Ui,
        index: &Index,
        epoch: SourceEpoch,
        commands: &mut ActionQueue,
    ) {
        let Some(mut binding) = self.narration.binding.take() else {
            return;
        };
        if binding.revision != self.source_revision {
            return;
        }
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique(("phrase-binding", self.instance_id))).show(
            ui.ctx(),
            |ui| {
                ui.set_width(360.0);
                ui.heading("Bind narration timing");
                ui.label(binding.anchor.phrase.as_deref().unwrap_or(&binding.anchor.paragraph));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut binding.anchor.edge, AnchorEdge::Start, "Start");
                    ui.selectable_value(&mut binding.anchor.edge, AnchorEdge::End, "End");
                });
                let Some(marker) = encode_anchor(&binding.anchor) else {
                    return;
                };
                egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                    for target in
                        index.targets.iter().filter(|target| target.scene == binding.scene)
                    {
                        let (range, replacement) = target.change(&marker);
                        let input = target.current_timing(self.text());
                        let source_timing = input.map(|input| match &target.marker {
                            Some(marker) if decode_anchor(marker).is_some() => "Bound cue",
                            _ => input,
                        });
                        let title = source_timing.map_or_else(
                            || format!("Line {} · {}", target.line + 1, target.label),
                            |timing| {
                                format!("Line {} · {timing} · {}", target.line + 1, target.label)
                            },
                        );
                        let timing = input.map_or_else(
                            || format!("Insert timing at the {}", edge_label(binding.anchor.edge)),
                            |input| {
                                format!(
                                    "Replace {input} with the {}",
                                    edge_label(binding.anchor.edge)
                                )
                            },
                        );
                        if ui
                            .add_enabled(
                                self.text()
                                    .get(range.clone())
                                    .is_some_and(|value| value != replacement),
                                Button::ghost(title),
                            )
                            .on_hover_text(timing)
                            .on_disabled_hover_text("This timing is already bound here.")
                            .clicked()
                            && let Some(patch) = SourcePatch::new(
                                self,
                                epoch,
                                range,
                                replacement,
                                "Bind narration timing",
                            )
                        {
                            commands.push_back(Command::ApplySourcePatch(patch).into());
                            close = true;
                        }
                    }
                });
                close |= ui.add(Button::ghost("Cancel")).clicked();
            },
        );
        if !close && !modal.should_close() {
            self.narration.binding = Some(binding);
        }
    }

    fn add_paragraph_ui(
        &self,
        ui: &mut egui::Ui,
        index: &Index,
        epoch: SourceEpoch,
        active_scene: Option<&str>,
        commands: &mut ActionQueue,
    ) {
        let active_scene = active_scene
            .filter(|scene| index.scenes.iter().any(|(name, _)| name == scene))
            .or_else(|| index.scenes.first().map(|(name, _)| name.as_str()));
        let title = if let Some(scene) = active_scene {
            format!("Add paragraph to {scene}")
        } else {
            "Add paragraph".to_owned()
        };
        let response = ui.add(Button::ghost(title).with_icon(egui_phosphor::regular::PLUS));
        let draft_id = response.id.with(("new-paragraph", self.source_revision));
        egui::Popup::from_toggle_button_response(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                let mut draft: String =
                    ui.data_mut(|data| data.get_temp(draft_id)).unwrap_or_default();
                ui.add(
                    egui::TextEdit::multiline(&mut draft).desired_rows(3).hint_text("Narration"),
                );
                if ui
                    .add_enabled(!draft.trim().is_empty(), Button::ghost("Insert paragraph"))
                    .clicked()
                {
                    let (range, text) =
                        paragraph_insert(self.text(), index, active_scene, draft.trim());
                    if let Some(patch) =
                        SourcePatch::new(self, epoch, range, text, "Add narration paragraph")
                    {
                        commands.push_back(Command::ApplySourcePatch(patch).into());
                    }
                    draft.clear();
                    ui.close();
                }
                ui.data_mut(|data| data.insert_temp(draft_id, draft));
            });
    }
}

fn edge_label(edge: AnchorEdge) -> &'static str {
    match edge {
        AnchorEdge::Start => "phrase start",
        AnchorEdge::End => "phrase end",
    }
}

fn paragraph_insert(
    source: &str,
    index: &Index,
    scene: Option<&str>,
    text: &str,
) -> (std::ops::Range<usize>, String) {
    let end = scene
        .and_then(|scene| {
            let scene_index = index.scenes.iter().position(|(name, _)| name == scene)?;
            index.scenes.get(scene_index + 1).map(|(_, start)| *start)
        })
        .unwrap_or(source.len());
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut insertion = String::new();
    if end > 0 {
        if !source[..end].ends_with('\n') {
            insertion.push_str(newline);
        }
        insertion.push_str(newline);
    }
    for line in text.lines() {
        insertion.push_str("/// ");
        insertion.push_str(line.trim_end());
        insertion.push_str(newline);
    }
    debug_assert!(!paragraphs(&insertion).is_empty());
    (end..end, insertion)
}

#[cfg(test)]
mod tests {
    use super::*;
    use animatix_syntax::narration::NarrationAnchor;
    use std::path::Path;

    #[test]
    fn add_first_paragraph_at_tail_of_the_selected_scene() {
        let source = "# A\r\n#1s\r\na: Rect\r\n# B\r\nb: Rect\r\n";
        let index = Index::new(source);
        let (range, insertion) = paragraph_insert(source, &index, Some("A"), "第一句\n第二句");
        let mut edited = source.to_owned();
        edited.replace_range(range, &insertion);
        assert_eq!(
            edited,
            "# A\r\n#1s\r\na: Rect\r\n\r\n/// 第一句\r\n/// 第二句\r\n# B\r\nb: Rect\r\n"
        );
        assert_eq!(paragraphs(&edited)[0].scene.as_deref(), Some("A"));
    }

    #[test]
    fn review_reveals_actual_text_edit_selection_and_typing_changes_only_that_phrase() {
        let source = "/// 开场，欢迎开场。\n#1s\n";
        let document = crate::app::document::DocumentSession::from_source(
            Path::new("script.amx").to_owned(),
            source.to_owned(),
        )
        .unwrap();
        let mut editor = EditorBuffer::new(Path::new("script.amx"), source.to_owned());
        let marker = encode_anchor(&NarrationAnchor {
            paragraph: "开场，欢迎开场。".into(),
            paragraph_occurrence: 1,
            phrase: Some("开场".into()),
            phrase_occurrence: 2,
            edge: AnchorEdge::End,
        })
        .unwrap();
        assert!(editor.reveal_narration(None, &marker));

        let ctx = egui::Context::default();
        let widget_id = egui::Id::unique((
            "script-paragraph",
            editor.instance_id,
            editor.narration.widgets_generation,
            0_usize,
        ));
        let mut commands = ActionQueue::default();
        let mut render = |events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show_script(
                        ui,
                        SourceEpoch::initial(),
                        &document,
                        &VoiceoverHistory::default(),
                        &mut commands,
                    );
                },
            )
            .drop_without_applying_deltas();
        };
        render(vec![]);
        let state = TextEditState::load(&ctx, widget_id).unwrap();
        let [start, end] = state.cursor.char_range().unwrap().sorted_cursors();
        assert_eq!((start.index.0, end.index.0), (5, 7));

        render(vec![egui::Event::Text("结束".into())]);
        assert_eq!(editor.text(), "/// 开场，欢迎结束。\n#1s\n");
        assert!(commands.is_empty());
    }
}
