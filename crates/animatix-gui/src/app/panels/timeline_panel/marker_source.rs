//! Conservative token-backed positions for authored top-level timing headers.
use super::markers::TimingMarker;
pub(super) use crate::app::document::source_timing::locate;

/// Cache tokenization by source and direct-marker identity, not by frame time.
pub(super) fn locate_cached(ui: &egui::Ui, source: &str, markers: &mut [TimingMarker]) {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hash);
    for marker in markers.iter() {
        marker.expression.hash(&mut hash);
        marker.scene.hash(&mut hash);
    }
    let revision = hash.finish();
    let id = ui.scope_id().with("source_marker_locations");
    let cached: Option<(u64, Vec<Option<usize>>)> = ui.data(|data| data.get_temp(id));
    if let Some((key, lines)) = cached
        && key == revision
        && lines.len() == markers.len()
    {
        for (marker, line) in markers.iter_mut().zip(lines) {
            marker.line = line;
        }
        return;
    }
    locate(source, markers);
    let lines: Vec<_> = markers.iter().map(|marker| marker.line).collect();
    ui.data_mut(|data| data.insert_temp(id, (revision, lines)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{ActionQueue, Command, ShellAction};
    use crate::app::panels::timeline_panel::markers::{collect_markers, navigate};

    #[test]
    fn real_document_markers_navigate_to_exact_authored_lines() {
        let source = "// fake #2s\n#0s\nbox: Rect, size: (20, 20)\n#2s\nbox.opacity = 0.5\n# +1s\nbox.opacity = 1";
        let store = crate::app::test_support::make_document_store(source);
        let doc = &store.source.document;
        let mut markers = collect_markers(
            doc.raw_statements.as_deref().unwrap(),
            doc.active_timeline(),
            doc.composition.as_ref(),
        );
        locate(&doc.source_text, &mut markers);
        assert_eq!(
            markers.iter().map(|m| m.line).collect::<Vec<_>>(),
            vec![Some(1), Some(3), Some(5)]
        );
        let mut commands = ActionQueue::default();
        navigate(&markers[1], &mut commands);
        assert!(matches!(
            commands.pop_front(),
            Some(ShellAction::Command(Command::ScrubTo(2.0)))
        ));
        assert!(matches!(
            commands.pop_front(),
            Some(ShellAction::Command(Command::ScrollToLine(3, 0)))
        ));
    }

    #[test]
    fn repeated_times_across_scenes_keep_distinct_source_lines() {
        let source = "# First\n#0s\na: Rect\n# Second\n#0s\nb: Rect";
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty());
        let mut markers = collect_markers(&ast.unwrap(), None, None);
        locate(source, &mut markers);
        assert_eq!(markers.iter().map(|m| m.line).collect::<Vec<_>>(), vec![Some(1), Some(4)]);
    }

    #[test]
    fn missing_authored_header_never_borrows_a_later_location() {
        let (ast, _) = animatix_syntax::parser::parse_source("#0s\n#0s");
        let mut markers = collect_markers(&ast.unwrap(), None, None);
        locate("// #0s\n#0s", &mut markers);
        assert!(markers.iter().all(|m| m.line.is_none()));
    }
}
