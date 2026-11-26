//! Source-linked review of the paragraphs in the active scene.
use std::sync::Arc;

use super::markers::TimingMarker;
use crate::app::commands::{ActionQueue, Command};
use crate::app::document::narration::{
    NarrationTrack, VoiceoverHistory, VoiceoverStatus, voiceover,
};
use crate::app::document::version::SourceEpoch;
use crate::app::i18n::{render, text};
use animatix::timeline::Timeline;
use animatix_syntax::narration::{self, AnchorEdge, NarrationAnchor};

type ParagraphCache = ((u64, u64, u64, bool), Arc<Vec<Paragraph>>);

struct Paragraph {
    text: String,
    marker: String,
    take_target: String,
    interval: Option<(f64, f64)>,
    status: VoiceoverStatus,
    bindings: Vec<(String, Option<f64>, String)>,
}

#[derive(Clone, Copy, Default)]
pub(crate) enum SpeechDetail {
    #[default]
    Words,
    Paragraphs,
    Selected(Option<(f64, f64)>),
}

pub(super) struct ScriptReview<'a> {
    pub history: &'a VoiceoverHistory,
    pub source: &'a str,
    pub anchor_changes: &'a [Vec<crate::editor::narration::AnchorChange>],
    pub current: bool,
    pub epoch: SourceEpoch,
    pub revision: u64,
    pub scene: Option<&'a str>,
    pub offset: f64,
    pub timeline: Option<&'a Timeline>,
    pub markers: &'a [TimingMarker],
}

pub(super) struct ReviewTrack {
    pub detail: SpeechDetail,
    paragraphs: Arc<Vec<Paragraph>>,
    selected: Option<String>,
    selection_id: egui::Id,
}
impl ReviewTrack {
    pub fn has_script(&self) -> bool {
        !self.paragraphs.is_empty()
    }

    pub fn draw(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        coords: &super::TimelineCoords,
        offset: f64,
        commands: &mut ActionQueue,
    ) {
        use egui::{Align2, Pos2, Sense};
        let theme = eparts::theme(ui);
        let font = egui::FontId::proportional(12.0);
        ui.painter().text(
            Pos2::new(rect.left() + 8., rect.center().y),
            Align2::LEFT_CENTER,
            text(ui, "gui-timeline-paragraphs"),
            font.clone(),
            theme.palette.text.secondary,
        );
        let mut aligned = false;
        for (index, p) in self.paragraphs.iter().enumerate() {
            let Some((start, end)) = p.interval else {
                continue;
            };
            aligned = true;
            let left = coords
                .time_to_x(animatix::time::add_seconds(offset, start))
                .max(coords.bar_origin_x);
            let right = coords.time_to_x(end + offset).min(rect.right());
            if right <= left {
                continue;
            }
            let segment = egui::Rect::from_min_max(
                Pos2::new(left, rect.top() + 3.),
                Pos2::new(right, rect.bottom() - 3.),
            );
            let active = self.selected.as_ref() == Some(&p.marker);
            let response = ui.interact(segment, self.selection_id.with(index), Sense::click());
            let painter = ui.painter().with_clip_rect(segment.intersect(ui.clip_rect()));
            painter.rect_filled(
                segment,
                4.,
                if active {
                    theme.palette.accent.primary
                } else {
                    theme.palette.surface.widget
                },
            );
            painter.text(
                Pos2::new(left + 6., segment.center().y),
                Align2::LEFT_CENTER,
                &p.text,
                font.clone(),
                if active {
                    theme.palette.surface.base
                } else {
                    theme.palette.text.primary
                },
            );
            if response.clicked() {
                ui.data_mut(|d| d.insert_temp(self.selection_id, p.marker.clone()));
                commands
                    .push_back(Command::ScrubTo(animatix::time::add_seconds(offset, start)).into());
            }
            response.on_hover_text(format!("{}\n{}", p.text, text(ui, p.status.message_id())));
        }
        if !aligned && let Some(first) = self.paragraphs.first() {
            let label = if self.paragraphs.iter().all(|p| p.status == first.status) {
                text(ui, first.status.message_id())
            } else {
                text(ui, "gui-narration-voiceover-select-status")
            };
            ui.painter().text(
                Pos2::new(coords.bar_origin_x + 8., rect.center().y),
                Align2::LEFT_CENTER,
                label,
                font,
                theme.palette.text.secondary,
            );
        }
    }
}

impl ScriptReview<'_> {
    fn paragraphs(&self) -> Vec<Paragraph> {
        let track = self.timeline.and_then(NarrationTrack::find);
        narration::paragraphs(self.source)
            .into_iter()
            .filter(|p| p.scene.as_deref() == self.scene)
            .filter_map(|p| {
                let marker = narration::encode_anchor(&NarrationAnchor {
                    paragraph: p.text.clone(),
                    paragraph_occurrence: p.occurrence,
                    phrase: None,
                    phrase_occurrence: 1,
                    edge: AnchorEdge::Start,
                })?;
                let (status, timing) =
                    voiceover(self.history, track.as_ref(), &p, self.anchor_changes);
                let interval = timing.map(|timing| timing.interval);
                let bindings =
                    self.markers
                        .iter()
                        .filter(|m| m.scene.as_deref() == self.scene)
                        .filter_map(|m| {
                            let (_, name) = m.expression.split_once('@')?;
                            let name = name.split_whitespace().next()?;
                            let key = narration::decode_anchor(name)?;
                            if !key.matches_paragraph(&p) {
                                return None;
                            }
                            let label = key
                                .phrase_range(&p.text)
                                .and_then(|range| p.text.get(range))
                                .unwrap_or(&p.text);
                            let action =
                                m.line
                                    .and_then(|line| {
                                        self.source.lines().skip(line + 1).map(str::trim).find(
                                            |line| !line.is_empty() && !line.starts_with("//"),
                                        )
                                    })
                                    .filter(|line| !line.starts_with('#'));
                            let title = match (action, m.action_count) {
                                (Some(action), 1) => format!("{label} · {action}"),
                                (Some(action), count) if count > 1 => {
                                    format!("{label} · {action} (+{})", count - 1)
                                },
                                (_, 1) => format!("{label} · 1 action"),
                                (_, count) => format!("{label} · {count} actions"),
                            };
                            Some((title, m.time_s, name.to_owned()))
                        })
                        .collect();
                Some(Paragraph {
                    take_target: p.id(),
                    text: p.text,
                    marker,
                    interval,
                    status,
                    bindings,
                })
            })
            .collect()
    }

    pub(super) fn show(
        &self,
        ui: &mut egui::Ui,
        commands: &mut ActionQueue,
        panel_id: egui::Id,
    ) -> ReviewTrack {
        // Selection and cache are independent of toolbar wrapping on resize.
        let id = panel_id.with(("script_review", self.scene));
        let key = (self.epoch.0, self.revision, self.history.revision, self.current);
        let data: Option<ParagraphCache> = ui.data(|d| d.get_temp(id));
        let paragraphs = match data {
            Some((cached, entries)) if cached == key => entries,
            _ => {
                let entries = Arc::new(self.paragraphs());
                ui.data_mut(|d| d.insert_temp(id, (key, Arc::clone(&entries))));
                entries
            },
        };
        let selection_id = id.with("selected");
        if paragraphs.is_empty() {
            return ReviewTrack {
                detail: SpeechDetail::Words,
                paragraphs,
                selected: None,
                selection_id,
            };
        }
        let mut selected: Option<String> = ui.data(|d| d.get_temp(selection_id));
        if let Some(marker) = &selected
            && !paragraphs.iter().any(|paragraph| paragraph.marker == *marker)
            && let Some(mut key) = narration::decode_anchor(marker)
        {
            for edits in self.anchor_changes {
                if let Some(change) = edits
                    .iter()
                    .find(|change| change.scene.as_deref() == self.scene && change.before == key)
                {
                    key = change.after.clone();
                }
            }
            if let Some(paragraph) = paragraphs
                .iter()
                .find(|p| narration::decode_anchor(&p.marker).as_ref() == Some(&key))
            {
                selected = Some(paragraph.marker.clone());
            }
        }
        let mut detail = SpeechDetail::Paragraphs;
        ui.horizontal_wrapped(|ui| {
            ui.strong(text(ui, "gui-narration-title"));
            let title = selected
                .as_ref()
                .and_then(|s| paragraphs.iter().position(|p| p.marker == *s))
                .map(|i| {
                    render(ui, "gui-timeline-paragraph-number", &[("number", (i + 1).to_string())])
                })
                .unwrap_or_else(|| text(ui, "gui-timeline-paragraphs-all"));
            egui::ComboBox::from_id_salt(id.with("paragraph"))
                .selected_text(title)
                .width(116.0)
                .wrap_mode(egui::TextWrapMode::Truncate)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut selected,
                        None,
                        text(ui, "gui-timeline-paragraphs-all"),
                    );
                    for (i, p) in paragraphs.iter().enumerate() {
                        if ui
                            .selectable_value(
                                &mut selected,
                                Some(p.marker.clone()),
                                render(
                                    ui,
                                    "gui-timeline-paragraph-option",
                                    &[
                                        ("number", (i + 1).to_string()),
                                        ("script", p.text.clone()),
                                        ("status", text(ui, p.status.message_id())),
                                    ],
                                ),
                            )
                            .clicked()
                            && let Some((start, _)) = p.interval
                            && self.current
                        {
                            commands.push_back(Command::ScrubTo(start + self.offset).into());
                        }
                    }
                });
            if let Some(p) =
                selected.as_ref().and_then(|s| paragraphs.iter().find(|p| p.marker == *s))
            {
                detail = SpeechDetail::Selected(p.interval);
                if ui
                    .add_enabled(self.current, egui::Button::new(text(ui, "gui-toolbar-voiceover")))
                    .clicked()
                {
                    commands.push_back(
                        Command::OpenParagraphTakes {
                            epoch: self.epoch,
                            scene: self.scene.map(str::to_owned),
                            paragraph: p.take_target.clone(),
                        }
                        .into(),
                    );
                }
                if ui
                    .add_enabled(
                        self.current,
                        egui::Button::new(text(ui, "gui-narration-go-to-script")),
                    )
                    .clicked()
                {
                    self.reveal(commands, &p.marker);
                }
                if self.current && !p.bindings.is_empty() {
                    ui.menu_button(
                        render(
                            ui,
                            "gui-timeline-paragraph-bindings",
                            &[("count", p.bindings.len().to_string())],
                        ),
                        |ui| {
                            for (label, time, marker) in &p.bindings {
                                if ui.button(label).clicked() {
                                    if let Some(t) = time {
                                        commands.push_back(Command::ScrubTo(*t).into());
                                    }
                                    self.reveal(commands, marker);
                                    ui.close();
                                }
                            }
                        },
                    );
                }
                if p.status != VoiceoverStatus::Current {
                    ui.weak(text(ui, p.status.message_id()));
                }
            }
        });
        ui.data_mut(|d| {
            if let Some(s) = selected.clone() {
                d.insert_temp(selection_id, s);
            } else {
                d.remove::<String>(selection_id)
            }
        });
        ReviewTrack {
            detail,
            selected,
            selection_id,
            paragraphs,
        }
    }

    fn reveal(&self, commands: &mut ActionQueue, marker: &str) {
        commands.push_back(
            Command::RevealNarration {
                epoch: self.epoch,
                scene: self.scene.map(str::to_owned),
                marker: marker.into(),
            }
            .into(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::document::DocumentSession;
    use crate::app::panels::timeline_panel::{marker_source, markers::collect_markers};

    #[test]
    fn second_chinese_phrase_uses_compiled_clip_delay_and_preserves_other_source_locations() {
        let dir = crate::app::test_support::TestTempDir::new("narration-review");
        let text = "零乘负一，零乘零，一乘一。";
        let phrase = narration::encode_anchor(&NarrationAnchor {
            paragraph: text.into(),
            paragraph_occurrence: 1,
            phrase: Some("乘".into()),
            phrase_occurrence: 2,
            edge: AnchorEdge::Start,
        })
        .unwrap();
        let transcript = serde_json::json!({"duration": 3.0, "segments": [{"start": 0.1, "end": 2.2, "text": text,
            "words": [{"word": "零乘负一", "start":0.1,"end":0.5}, {"word":"零","start":0.6,"end":0.7},
            {"word":"乘","start":0.8,"end":1.0}, {"word":"零一乘一","start":1.2,"end":2.2}]}]});
        std::fs::write(dir.path().join("line.speech.json"), transcript.to_string()).unwrap();
        let source = format!(
            "config {{ duration: 6 }}\nvoice: Audio, source: \"line.wav\", transcript: \"line.speech.json\" [delay: 1.5s]\nbox: Rect, opacity: 0\n/// {text}\n#voice@{phrase}\n  box.opacity = 1\n#4s\n  box.opacity = 0\n"
        );
        let mut doc =
            DocumentSession::from_source(dir.path().join("scene.amx"), source.clone()).unwrap();
        doc.rebuild().unwrap();
        let timeline = doc.active_timeline();
        let mut markers = collect_markers(doc.raw_statements.as_deref().unwrap(), timeline, None);
        marker_source::locate(&source, &mut markers);
        let bound = markers.iter().find(|m| m.expression.contains(&phrase)).unwrap();
        assert_eq!(bound.line, Some(4));
        assert_eq!(
            bound.time_s,
            Some(2.3),
            "diagnostics {:?}, transcript {:?}",
            doc.diagnostics,
            timeline.and_then(|tl| tl.transcript("voice"))
        );
        assert_eq!(markers.last().unwrap().line, Some(6));
        let review = ScriptReview {
            history: &VoiceoverHistory::default(),
            source: &source,
            anchor_changes: &[],
            current: true,
            epoch: SourceEpoch::initial(),
            revision: 0,
            scene: None,
            offset: 0.,
            timeline,
            markers: &markers,
        };
        let entries = review.paragraphs();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].interval, Some((1.6, 3.7)));
        assert!(
            entries[0]
                .bindings
                .iter()
                .any(|(label, t, _)| label.contains('乘') && *t == Some(2.3))
        );
    }
}
