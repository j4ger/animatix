//! Source-backed projections and phrase binding for the Write pane.

use std::ops::Range;
use std::sync::Arc;

use animatix_syntax::narration::{
    AnchorEdge, NarrationAnchor, NarrationParagraph, decode_anchor, paragraphs, phrase_ranges,
};
use animatix_syntax::token::LineIndex;

use super::EditorBuffer;

mod projection;
mod repair;
mod targets;
mod widgets;

use projection::{CodeProjection, ScriptProjection};
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
        (Self::Full, "Full source"),
        (Self::Script, "Only script"),
        (Self::Code, "Only code"),
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
        self.text.replace_range(range, replacement);
        self.after_source_replaced();
        self.cursor_char = None;
        self.cursor_line = None;
        self.narration.binding = None;
        self.narration.repair = None;
        self.narration.selection = None;
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
