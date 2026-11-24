use std::ops::Range;

use animatix_syntax::narration::{
    AnchorEdge, AnchorKey, NarrationAnchor, NarrationParagraph, decode_anchor, encode_anchor,
    is_implicit_anchor,
};
use animatix_syntax::token::LineIndex;
use egui::text::{LayoutJob, TextFormat};

use super::{EditorBuffer, Index, TimingTarget, anchor_for_phrase};
use crate::app::commands::{ActionQueue, Command};
use crate::app::components::button::Button;
use crate::app::document::version::SourceEpoch;
use crate::editor::source_patch::SourcePatch;

pub(super) struct StaleBinding {
    target: usize,
    key: AnchorKey,
    candidates: Vec<Candidate>,
}

struct Candidate {
    paragraph: usize,
    phrase: Range<usize>,
}

#[derive(Clone, Copy)]
pub(super) struct RepairSession {
    binding: usize,
    revision: u64,
    manual: bool,
}

pub(super) fn stale_bindings(
    targets: &[TimingTarget],
    paragraphs: &[NarrationParagraph],
) -> Vec<StaleBinding> {
    targets
        .iter()
        .enumerate()
        .filter_map(|(target_index, target)| {
            let marker = target.marker.as_deref()?;
            target.marker_span.as_ref()?;
            if is_implicit_anchor(marker) {
                return None;
            }
            let key = decode_anchor(marker)?;
            let in_scene = || {
                paragraphs
                    .iter()
                    .enumerate()
                    .filter(|(_, paragraph)| paragraph.scene == target.scene)
            };
            if in_scene().filter(|(_, paragraph)| key.matches_paragraph(paragraph)).count() == 1 {
                return None;
            }
            let candidates = in_scene()
                .flat_map(|(paragraph, source)| {
                    key.repair_ranges(&source.text)
                        .into_iter()
                        .map(move |phrase| Candidate { paragraph, phrase })
                })
                .collect();
            Some(StaleBinding {
                target: target_index,
                key,
                candidates,
            })
        })
        .collect()
}

impl EditorBuffer {
    pub(super) fn show_repair_control(&mut self, ui: &mut egui::Ui, index: &Index) {
        if index.stale_bindings.is_empty() {
            return;
        }
        if let Some((session, binding)) = self.current_repair(index)
            && session.manual
        {
            let target = &index.targets[binding.target];
            let instruction = if binding.key.normalized_phrase().is_some() {
                "Select the replacement phrase"
            } else {
                "Choose the replacement paragraph"
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{instruction} for line {}.", target.line + 1));
                if ui.add(Button::ghost("Cancel")).clicked() {
                    self.narration.repair = None;
                }
            });
            return;
        }

        if index.stale_bindings.len() == 1 {
            let target = &index.targets[index.stale_bindings[0].target];
            ui.horizontal_wrapped(|ui| {
                ui.weak(target_label(target));
                if ui.add(Button::ghost("Rebind timing…")).clicked() {
                    self.start_repair(0);
                }
            });
            return;
        }
        let response =
            ui.add(Button::ghost(format!("{} cues need timing…", index.stale_bindings.len())));
        egui::Popup::menu(&response).show(|ui| {
            egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                for (ordinal, binding) in index.stale_bindings.iter().enumerate() {
                    if ui.add(Button::ghost(target_label(&index.targets[binding.target]))).clicked()
                    {
                        self.start_repair(ordinal);
                        ui.close();
                    }
                }
            });
        });
    }

    fn start_repair(&mut self, binding: usize) {
        self.narration.binding = None;
        self.narration.repair = Some(RepairSession {
            binding,
            revision: self.source_revision,
            manual: false,
        });
    }

    fn current_repair<'a>(&self, index: &'a Index) -> Option<(RepairSession, &'a StaleBinding)> {
        let session = self.narration.repair?;
        let binding = index.stale_bindings.get(session.binding)?;
        (session.revision == self.source_revision).then_some((session, binding))
    }

    pub(super) fn manual_repair_action(
        &mut self,
        ui: &mut egui::Ui,
        index: &Index,
        paragraph_index: usize,
        epoch: SourceEpoch,
        commands: &mut ActionQueue,
    ) -> bool {
        let Some((session, binding)) = self.current_repair(index) else {
            return false;
        };
        if !session.manual {
            return false;
        }
        let paragraph = &index.paragraphs[paragraph_index];
        let target = &index.targets[binding.target];
        if paragraph.scene != target.scene {
            return true;
        }
        let (label, anchor) = if binding.key.normalized_phrase().is_some() {
            let anchor = self.narration.selection.as_ref().and_then(|selection| {
                if selection.paragraph != paragraph_index || selection.revision != session.revision
                {
                    return None;
                }
                let phrase = index.script[paragraph_index]
                    .phrase_selection(paragraph, selection.range.clone())?;
                anchor_for_phrase(paragraph, Some(phrase), binding.key.edge())
            });
            ("Use selected phrase", anchor)
        } else {
            ("Use this paragraph", anchor_for_phrase(paragraph, None, binding.key.edge()))
        };
        if ui
            .add_enabled(anchor.is_some(), Button::ghost(label).with_size(eparts::Size::Sm))
            .clicked()
            && let Some(anchor) = anchor
            && let Some(patch) = patch_binding(self, epoch, target, &anchor)
        {
            commands.push_back(Command::ApplySourcePatch(patch).into());
            self.narration.repair = None;
        }
        true
    }

    pub(super) fn show_binding_choices(
        &mut self,
        ui: &mut egui::Ui,
        index: &Index,
        epoch: SourceEpoch,
        commands: &mut ActionQueue,
    ) {
        let Some((mut session, binding)) = self.current_repair(index) else {
            return;
        };
        if session.manual {
            return;
        }
        let target = &index.targets[binding.target];
        let mut close = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.strong("Which words should time this action?");
            ui.label(target_label(target));
            ui.label(match binding.key.edge() {
                AnchorEdge::Start => "Timing uses the start of the selection.",
                AnchorEdge::End => "Timing uses the end of the selection.",
            });
            if binding.candidates.is_empty() {
                ui.label("Choose replacement text in the script.");
            } else {
                ui.label("Choose the intended occurrence.");
                egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                    let lines = LineIndex::new(self.text());
                    for (ordinal, candidate) in binding.candidates.iter().enumerate() {
                        let paragraph = &index.paragraphs[candidate.paragraph];
                        let line = lines.byte_to_line_col(paragraph.span.start).0 + 1;
                        ui.label(format!("Paragraph at line {line}"));
                        ui.label(candidate_preview(ui, &paragraph.text, candidate.phrase.clone()));
                        if ui.add(Button::ghost(format!("Use match {}", ordinal + 1))).clicked()
                            && let Some(anchor) = anchor_for_phrase(
                                paragraph,
                                Some(candidate.phrase.clone()),
                                binding.key.edge(),
                            )
                            && let Some(patch) = patch_binding(self, epoch, target, &anchor)
                        {
                            commands.push_back(Command::ApplySourcePatch(patch).into());
                            close = true;
                        }
                        ui.separator();
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                session.manual = ui.add(Button::ghost("Select in script")).clicked();
                close |= ui.add(Button::ghost("Cancel")).clicked();
            });
        });
        self.narration.repair = (!close).then_some(session);
    }
}

fn patch_binding(
    editor: &EditorBuffer,
    epoch: SourceEpoch,
    target: &TimingTarget,
    anchor: &NarrationAnchor,
) -> Option<SourcePatch> {
    SourcePatch::new(
        editor,
        epoch,
        target.marker_span.clone()?,
        encode_anchor(anchor)?,
        "Choose narration timing",
    )
}

fn target_label(target: &TimingTarget) -> String {
    target.scene.as_ref().map_or_else(
        || format!("Line {} · {}", target.line + 1, target.label),
        |scene| format!("{scene} · Line {} · {}", target.line + 1, target.label),
    )
}

fn candidate_preview(ui: &egui::Ui, text: &str, phrase: Range<usize>) -> LayoutJob {
    let context_chars = 24;
    let (before, remainder) = text.split_at(phrase.start);
    let (selected, after) = remainder.split_at(phrase.len());
    let left = before.char_indices().rev().nth(context_chars - 1).map_or(0, |(byte, _)| byte);
    let right = after.char_indices().nth(context_chars).map_or(after.len(), |(byte, _)| byte);
    let plain = TextFormat {
        font_id: egui::TextStyle::Body.resolve(ui.style()),
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    let accent = TextFormat {
        background: ui.visuals().selection.bg_fill,
        color: ui.visuals().selection.stroke.color,
        ..plain.clone()
    };
    let mut job = LayoutJob::default();
    if left > 0 {
        job.append("…", 0.0, plain.clone());
    }
    job.append(&before[left..], 0.0, plain.clone());
    job.append(selected, 0.0, accent);
    job.append(&after[..right], 0.0, plain.clone());
    if right < after.len() {
        job.append("…", 0.0, plain);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::app::document::DocumentSession;
    use crate::app::stores::SourceStore;

    #[test]
    fn repair_displays_both_occurrences_and_only_updates_the_chosen_marker() {
        let previous = "开场，欢迎开场。";
        let edited = "新稿开场，欢迎开场。";
        let old = encode_anchor(&NarrationAnchor {
            paragraph: previous.to_owned(),
            paragraph_occurrence: 1,
            phrase: Some("开场".to_owned()),
            phrase_occurrence: 2,
            edge: AnchorEdge::End,
        })
        .unwrap();
        let original = format!(
            "# First\r\n/// {edited}\r\n# voice@{old} + 250ms: // keep\r\nbox.opacity = 1\r\n# Second\r\n/// 开场。\r\n",
        );
        let path = PathBuf::from("repair.amx");
        let document = DocumentSession::from_source(path.clone(), original.clone()).unwrap();
        let source = SourceStore::new(document, EditorBuffer::new(&path, original.clone()));
        let index = Index::new(source.text());
        assert_eq!(index.stale_bindings.len(), 1);
        let binding = &index.stale_bindings[0];
        assert_eq!(binding.candidates.len(), 2);
        assert!(binding.candidates.iter().all(|c| c.paragraph == 0));

        let mut displayed = Vec::new();
        egui::Context::default()
            .run_ui(egui::RawInput::default(), |ui| {
                for candidate in &binding.candidates {
                    let view = candidate_preview(ui, edited, candidate.phrase.clone());
                    let highlighted = view
                        .sections
                        .iter()
                        .find(|section| section.format.background == ui.visuals().selection.bg_fill)
                        .unwrap();
                    let start = highlighted.byte_range.start.0;
                    let end = highlighted.byte_range.end.0;
                    assert_eq!(&view.text[start..end], "开场");
                    displayed.push(start);
                }
            })
            .drop_without_applying_deltas();
        displayed.sort_unstable();
        assert_eq!(displayed, [edited.find("开场").unwrap(), edited.rfind("开场").unwrap()]);
        assert_eq!(source.text(), original);

        let chosen = binding.candidates.iter().find(|c| c.phrase.start == displayed[1]).unwrap();
        let paragraph = &index.paragraphs[chosen.paragraph];
        let replacement =
            anchor_for_phrase(paragraph, Some(chosen.phrase.clone()), binding.key.edge()).unwrap();
        assert_eq!(replacement.edge, AnchorEdge::End);
        assert_eq!(replacement.phrase_occurrence, 2);
        let target = &index.targets[binding.target];
        let patch = patch_binding(&source.editor, source.epoch(), target, &replacement).unwrap();
        let after = patch.apply(&source).unwrap();
        let new_marker = encode_anchor(&replacement).unwrap();
        assert_eq!(after, original.replacen(&old, &new_marker, 1));
        assert!(Index::new(&after).stale_bindings.is_empty());
    }
}
