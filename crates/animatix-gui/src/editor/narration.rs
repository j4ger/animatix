//! Source-backed projections and phrase binding for the Write pane.

use std::ops::Range;
use std::sync::Arc;

use animatix::narration::DraftCueTime;
use animatix_syntax::narration::{
    AnchorEdge, NarrationAnchor, NarrationParagraph, decode_anchor, paragraphs, phrase_ranges,
};
use animatix_syntax::token::LineIndex;

use super::EditorBuffer;

mod projection;
mod rebase;
mod repair;
mod targets;
mod widgets;

use projection::{CodeProjection, ScriptProjection};
pub(crate) use rebase::AnchorChange;
use repair::{RepairSession, StaleBinding, stale_bindings};
use targets::TimingTarget;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SourceView {
    #[default]
    Full,
    Script,
    Code,
}

impl SourceView {
    pub(crate) const ALL: [(Self, &'static str); 3] = [
        (Self::Full, "gui-source-view-full"),
        (Self::Script, "gui-source-view-script"),
        (Self::Code, "gui-source-view-code"),
    ];
}

#[derive(Default)]
pub(super) struct NarrationEditor {
    view: SourceView,
    index: Option<(u64, Arc<Index>)>,
    widgets_generation: u64,
    reveal: Option<RevealedPhrase>,
    selection: Option<ScriptSelection>,
    binding: Option<Binding>,
    repair: Option<RepairSession>,
    code_edit_rejected: bool,
    anchor_changes: Vec<Vec<AnchorChange>>,
    draft_cues: Vec<DraftCueTime>,
}

struct RevealedPhrase {
    paragraph: usize,
    range: Range<usize>,
}

struct ScriptSelection {
    paragraph: usize,
    range: Range<usize>,
    revision: u64,
}

struct Binding {
    scene: Option<String>,
    revision: u64,
    anchor: NarrationAnchor,
}

struct Index {
    paragraphs: Vec<NarrationParagraph>,
    script: Vec<ScriptProjection>,
    code: CodeProjection,
    targets: Vec<TimingTarget>,
    stale_bindings: Vec<StaleBinding>,
    scenes: Vec<(String, usize)>,
}

impl Index {
    fn new(source: &str) -> Self {
        let paragraphs = paragraphs(source);
        let script: Vec<_> = paragraphs
            .iter()
            .scan(None, |previous_end, paragraph| {
                let projection = ScriptProjection::new(source, paragraph, *previous_end);
                *previous_end = Some(paragraph.span.end);
                Some(projection)
            })
            .collect();
        let code = CodeProjection::new(source, &script);
        let (targets, scenes) = targets::inspect_source(source);
        let stale_bindings = stale_bindings(&targets, &paragraphs);
        Self {
            paragraphs,
            script,
            code,
            targets,
            stale_bindings,
            scenes,
        }
    }
}

impl NarrationEditor {
    pub(super) fn reset_widgets(&mut self) {
        self.widgets_generation = self.widgets_generation.wrapping_add(1);
        self.reveal = None;
        self.selection = None;
        self.binding = None;
        self.repair = None;
        self.code_edit_rejected = false;
    }
}

impl EditorBuffer {
    pub(crate) fn narration_anchor_changes(&self) -> &[Vec<AnchorChange>] {
        &self.narration.anchor_changes
    }

    pub(super) fn clear_narration_anchor_changes(&mut self) {
        self.narration.anchor_changes.clear();
        self.narration.draft_cues.clear();
    }

    pub(crate) fn draft_narration_cues(&self) -> &[DraftCueTime] {
        &self.narration.draft_cues
    }

    pub(crate) fn remember_narration_cues(&mut self, cues: Vec<DraftCueTime>) {
        for cue in cues {
            let previous = self.narration.draft_cues.iter_mut().find(|previous| {
                previous.scene == cue.scene
                    && previous.track == cue.track
                    && previous.key == cue.key
                    && previous.offset_ms == cue.offset_ms
            });
            if let Some(previous) = previous {
                previous.time_ms = cue.time_ms;
                if !cue.previous_keys.is_empty() {
                    previous.previous_keys = cue.previous_keys;
                }
            } else {
                self.narration.draft_cues.push(cue);
            }
        }
    }

    pub(crate) fn source_view(&self) -> SourceView {
        self.narration.view
    }

    pub(crate) fn set_source_view(&mut self, view: SourceView) {
        if self.narration.view != view {
            self.narration.reset_widgets();
            self.narration.view = view;
        }
    }

    fn narration_index(&mut self) -> Arc<Index> {
        if let Some((revision, index)) = &self.narration.index
            && self.source_revision == *revision
        {
            return Arc::clone(index);
        }
        let index = Arc::new(Index::new(self.text()));
        self.narration.index = Some((self.source_revision, Arc::clone(&index)));
        index
    }

    /// Open the projected script with the phrase or paragraph selected.
    pub(crate) fn reveal_narration(&mut self, scene: Option<&str>, marker: &str) -> bool {
        let Some(key) = decode_anchor(marker) else {
            return false;
        };
        let index = self.narration_index();
        let mut matched = index.paragraphs.iter().enumerate().filter(|(_, paragraph)| {
            paragraph.scene.as_deref() == scene && key.matches_paragraph(paragraph)
        });
        let Some((ordinal, paragraph)) = matched.next() else {
            return false;
        };
        if matched.next().is_some() {
            return false;
        }
        let phrase = key.phrase_range(&paragraph.text).unwrap_or(0..paragraph.text.len());
        let Some(range) = index.script[ordinal].script_selection(paragraph, phrase) else {
            return false;
        };
        self.set_source_view(SourceView::Script);
        self.narration.reveal = Some(RevealedPhrase {
            paragraph: ordinal,
            range,
        });
        true
    }

    pub(crate) fn cursor_narration_marker(&mut self) -> Option<(Option<String>, String)> {
        let line = self.cursor_line?;
        let index = LineIndex::new(self.text());
        let start = index.line_col_to_byte(line, 0);
        let end = index.line_col_to_byte(line + 1, 0);
        if !self.text().get(start..end)?.contains("__narration_") {
            return None;
        }
        let index = self.narration_index();
        let target = index.targets.iter().find(|target| target.line == line)?;
        let marker = target.marker.as_ref()?;
        decode_anchor(marker)?;
        Some((target.scene.clone(), marker.clone()))
    }

    fn projection_edit(&mut self, range: Range<usize>, replacement: &str) {
        // The full TextEdit maintains its own native undo history. A projected
        // edit breaks that history's source lineage, so retire the old widget.
        self.retire_widget_state();
        self.advance_widget_generation();
        let mut edited = self.text.clone();
        edited.replace_range(range, replacement);
        let rebase = (self.narration.view == SourceView::Script)
            .then(|| rebase::after_script_edit(&self.text, &edited, &self.narration.anchor_changes))
            .flatten();
        self.text = edited;
        if let Some(rebase) = rebase {
            self.apply_narration_rebase(rebase, None);
        }
        self.after_source_replaced();
        self.cursor_char = None;
        self.cursor_line = None;
        self.narration.binding = None;
        self.narration.repair = None;
        self.narration.selection = None;
    }

    pub(super) fn rebase_full_source(
        &mut self,
        previous: &str,
        cursor: Option<&mut egui::text::CCursorRange>,
    ) -> bool {
        let Some(rebase) =
            rebase::after_script_edit(previous, &self.text, &self.narration.anchor_changes)
        else {
            return false;
        };
        self.apply_narration_rebase(rebase, cursor)
    }

    fn apply_narration_rebase(
        &mut self,
        rebase: rebase::Rebase,
        mut cursor: Option<&mut egui::text::CCursorRange>,
    ) -> bool {
        let changed = !rebase.edits.is_empty();
        for (range, replacement) in rebase.edits.into_iter().rev() {
            if let Some(selection) = cursor.as_deref_mut() {
                let start = self.text[..range.start].chars().count();
                let removed = self.text[range.clone()].chars().count();
                let inserted = replacement.chars().count();
                for cursor in [&mut selection.primary, &mut selection.secondary] {
                    if cursor.index.0 > start {
                        cursor.index.0 =
                            start + inserted + cursor.index.0.saturating_sub(start + removed);
                    }
                }
            }
            self.text.replace_range(range, &replacement);
        }
        let updated_cues = self
            .narration
            .draft_cues
            .iter()
            .filter_map(|cue| {
                let change = rebase
                    .anchors
                    .iter()
                    .find(|change| change.scene == cue.scene && change.before == cue.key)?;
                let mut updated = cue.clone();
                updated.key = change.after.clone();
                updated.previous_keys.retain(|key| key != &cue.key && *key != updated.key);
                updated.previous_keys.insert(0, cue.key.clone());
                Some(updated)
            })
            .collect();
        self.remember_narration_cues(updated_cues);
        if !rebase.anchors.is_empty() {
            self.narration.anchor_changes.push(rebase.anchors);
        }
        changed
    }
}

fn anchor_for(
    paragraph: &NarrationParagraph,
    projection: &ScriptProjection,
    selection: Option<Range<usize>>,
) -> Option<NarrationAnchor> {
    let phrase_range = match selection {
        Some(range) => Some(projection.phrase_selection(paragraph, range)?),
        None => None,
    };
    anchor_for_phrase(paragraph, phrase_range, AnchorEdge::Start)
}

fn anchor_for_phrase(
    paragraph: &NarrationParagraph,
    phrase_range: Option<Range<usize>>,
    edge: AnchorEdge,
) -> Option<NarrationAnchor> {
    let (phrase, phrase_occurrence) = match phrase_range {
        Some(range) => {
            let phrase = paragraph.text.get(range.clone())?;
            let occurrence = phrase_ranges(&paragraph.text, phrase)
                .iter()
                .position(|found| range.start <= found.start && found.end <= range.end)?
                + 1;
            (Some(phrase.to_owned()), occurrence)
        },
        None => (None, 1),
    };
    Some(NarrationAnchor {
        paragraph: paragraph.text.clone(),
        paragraph_occurrence: paragraph.occurrence,
        phrase,
        phrase_occurrence,
        edge,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn repeated_cjk_phrase_round_trips_to_the_selected_source_occurrence() {
        let source = "# Opening\n/// 开场，欢迎开场。\n#1s\n";
        let mut editor = EditorBuffer::new(Path::new("opening.amx"), source.to_owned());
        let index = editor.narration_index();
        let paragraph = &index.paragraphs[0];
        let script = &index.script[0];
        let start = script.text.rfind("开场").unwrap();
        let selection = start..start + "开场".len();
        let mut anchor = anchor_for(paragraph, script, Some(selection.clone())).unwrap();
        anchor.edge = AnchorEdge::End;
        assert_eq!(anchor.phrase_occurrence, 2);

        let marker = animatix_syntax::narration::encode_anchor(&anchor).unwrap();
        assert!(editor.reveal_narration(Some("Opening"), &marker));
        assert_eq!(editor.source_view(), SourceView::Script);
        assert_eq!(editor.narration.reveal.as_ref().unwrap().range, selection);
        assert_eq!(editor.text(), source);
        assert!(!editor.reveal_narration(Some("WrongScene"), &marker));
    }

    #[test]
    fn overlapping_cjk_selection_binds_its_exact_range() {
        let source = "/// 哈哈哈\n#1s\n";
        let mut editor = EditorBuffer::new(Path::new("overlap.amx"), source.to_owned());
        let index = editor.narration_index();
        let selection = "哈".len().."哈哈哈".len();
        let anchor =
            anchor_for(&index.paragraphs[0], &index.script[0], Some(selection.clone())).unwrap();
        assert_eq!(anchor.phrase_occurrence, 2);
        let marker = animatix_syntax::narration::encode_anchor(&anchor).unwrap();
        assert!(editor.reveal_narration(None, &marker));
        assert_eq!(editor.narration.reveal.as_ref().unwrap().range, selection);
    }
}

#[cfg(test)]
mod retake_tests {
    use super::*;
    use crate::app::document::DocumentSession;
    use crate::app::stores::SourceStore;
    use crate::app::test_support::TestTempDir;
    use animatix_syntax::narration::encode_anchor;

    #[test]
    fn revised_cue_previews_previous_audio_and_restores_its_clock_after_undo() {
        let directory = TestTempDir::new("retake-preview");
        let path = directory.path().join("test.amx");
        let original = "Look at the card.";
        let revised = "Look closely at the card.";
        for (file, text, card, starts) in [
            ("old.json", original, 0.6, vec![0.1, 0.2, 0.3, 0.6]),
            ("new.json", revised, 1.1, vec![0.1, 0.25, 0.5, 0.7, 1.1]),
        ] {
            let words: Vec<_> = text.split_whitespace().zip(starts).map(|(word, start)| {
                serde_json::json!({"word": word, "start": start, "end": start + 0.06})
            }).collect();
            let transcript = serde_json::json!({
                "segments": [{"text": text, "start": 0.1, "end": card + 0.06, "words": words}],
                "duration": 1.8,
            });
            std::fs::write(directory.path().join(file), transcript.to_string()).unwrap();
        }
        let marker = encode_anchor(&NarrationAnchor {
            paragraph: original.into(),
            paragraph_occurrence: 1,
            phrase: Some("card".into()),
            phrase_occurrence: 1,
            edge: AnchorEdge::Start,
        })
        .unwrap();
        let before = format!(
            "voice: Audio, source: \"line.wav\", transcript: \"old.json\" [delay: 1s]\n             box: Rect, opacity: 0\n/// {original}\n#voice@{marker} + 250ms\n  box.opacity = 1\n",
        );
        let build = |source: &str, cues: &[DraftCueTime]| {
            let mut doc = DocumentSession::from_source(path.clone(), source.to_owned()).unwrap();
            doc.rebuild_with_caches(None, &[], cues).unwrap();
            doc
        };
        let cue_time = |doc: &DocumentSession| {
            doc.resolved_narration_cues()
                .into_iter()
                .find(|cue| cue.key.normalized_phrase().is_some())
                .unwrap()
                .time_ms
        };
        let doc = build(&before, &[]);
        assert_eq!(cue_time(&doc), 1850);
        let mut store = SourceStore::new(doc, EditorBuffer::new(&path, before.clone()));
        store.editor.set_source_view(SourceView::Script);
        let start = before.find(original).unwrap();
        store.editor.projection_edit(start..start + original.len(), revised);
        let edited = store.editor.text().to_owned();
        assert!(!edited.contains(&marker));

        let pending = build(&edited, store.editor.draft_narration_cues());
        assert_eq!(cue_time(&pending), 1850);
        assert!(pending.diagnostics.iter().any(|d| d.message.contains("previous timing")));

        let updated =
            build(&edited.replace("old.json", "new.json"), store.editor.draft_narration_cues());
        assert_eq!(cue_time(&updated), 2350);
        store.editor.remember_narration_cues(updated.resolved_narration_cues());
        let undone = build(&edited, store.editor.draft_narration_cues());
        assert_eq!(cue_time(&undone), 1850);
    }
}
