use std::ops::Range;
use std::path::PathBuf;

use super::EditorBuffer;
use crate::app::document::version::SourceEpoch;
use crate::app::stores::SourceStore;

/// One visible edit to the canonical `.amx` source. Both the live editor and
/// the synchronized document must still be at the revision used to plan it.
#[derive(Clone, Debug)]
pub struct SourcePatch {
    epoch: SourceEpoch,
    path: PathBuf,
    instance_id: u64,
    revision: u64,
    range: Range<usize>,
    expected: String,
    replacement: String,
    label: &'static str,
}

impl SourcePatch {
    pub(crate) fn new(
        editor: &EditorBuffer,
        epoch: SourceEpoch,
        range: Range<usize>,
        replacement: String,
        label: &'static str,
    ) -> Option<Self> {
        let expected = editor.text().get(range.clone())?;
        (expected != replacement).then(|| Self {
            epoch,
            path: editor.document_path().to_owned(),
            instance_id: editor.instance_id,
            revision: editor.source_revision,
            range,
            expected: expected.to_owned(),
            replacement,
            label,
        })
    }

    pub(crate) fn label(&self) -> &'static str {
        self.label
    }

    pub(crate) fn apply(&self, source: &SourceStore) -> Option<String> {
        let editor = &source.editor;
        if self.epoch != source.epoch()
            || self.instance_id != editor.instance_id
            || self.revision != editor.source_revision
            || self.path != source.file_path()
            || editor.text() != source.text()
            || source.text().get(self.range.clone())? != self.expected
        {
            return None;
        }
        let mut text = source.text().to_owned();
        text.replace_range(self.range.clone(), &self.replacement);
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::make_document_store;

    #[test]
    fn stale_patch_is_rejected_even_if_range_and_text_are_unchanged() {
        let original = "// 开场\n#1s\nbox: Rect\n";
        let mut store = make_document_store(original);
        let start = original.find("1s").unwrap();
        let patch = SourcePatch::new(
            &store.source.editor,
            store.source.epoch(),
            start..start + 2,
            "2s".into(),
            "Bind narration",
        )
        .unwrap();
        assert_eq!(patch.apply(&store.source).as_deref(), Some("// 开场\n#2s\nbox: Rect\n"));
        store.source.editor.replace_text(original.to_owned());
        assert!(patch.apply(&store.source).is_none());
    }
}
